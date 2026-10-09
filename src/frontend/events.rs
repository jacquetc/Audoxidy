//! The event hub: how the facade tells a UI that something changed.
//!
//! Background threads (the playback monitor, the workers, the long operations,
//! the commands) publish typed [`Event`]s; a UI subscribes per [`Origin`]. The
//! contract maps one-to-one onto Teksilo's `EventSource` (the Teksilo UI wraps
//! [`EventHub`] in an adapter):
//!
//! - [`Origin`] is `Clone + Eq + Hash + 'static`;
//! - [`Event`] is `Clone + Send + 'static`, so each subscriber gets its own copy;
//! - [`EventHub::subscribe`] takes an
//!   `Arc<dyn Fn(Event) + Send + Sync + 'static>` and returns a
//!   [`SubscriptionToken`]; dropping the token unsubscribes.
//!
//! Callbacks run on the hub's dispatch thread, never on the publisher's. A
//! publisher only pushes into an unbounded channel, so it is never blocked by a
//! slow subscriber, and the events of one publisher are delivered in the order
//! it published them.
//!
//! Payloads are typed Rust values, never JSON.

#![warn(missing_docs)]

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::RwLock;

use super::error::FrontendError;
use super::threading::WorkerThread;
use crate::audio::engine::AudioState;
use crate::integrations::control::ExternalControlEvent;
use crate::logic::playlist_nav::RepeatMode;

/// The groups of events a UI can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// The audio engine's state: play state, position, track, end of track,
    /// notices, output device.
    Playback,
    /// The library database: content changes and scans.
    Library,
    /// The playlists: the list itself and the content of one playlist.
    Playlists,
    /// The playback session: the startup restore and the autosave cadence.
    Session,
    /// Transport requests from the system media controls (MPRIS/SMTC).
    ExternalControl,
    /// Progress and outcome of the long operations.
    LongOperation,
    /// Memory maintenance requests.
    Maintenance,
}

/// An event published on the hub. Its [`Origin`] is [`Event::origin`].
#[derive(Debug, Clone)]
pub enum Event {
    /// See [`PlaybackEvent`].
    Playback(PlaybackEvent),
    /// See [`LibraryEvent`].
    Library(LibraryEvent),
    /// See [`PlaylistsEvent`].
    Playlists(PlaylistsEvent),
    /// See [`SessionEvent`].
    Session(SessionEvent),
    /// A transport request from the system media controls. The UI resolves it
    /// with `logic::transport::external_control_action`.
    ExternalControl(ExternalControlEvent),
    /// See [`LongOpEvent`].
    LongOperation(LongOpEvent),
    /// See [`MaintenanceEvent`].
    Maintenance(MaintenanceEvent),
}

impl Event {
    /// The group this event is delivered to.
    pub fn origin(&self) -> Origin {
        match self {
            Event::Playback(_) => Origin::Playback,
            Event::Library(_) => Origin::Library,
            Event::Playlists(_) => Origin::Playlists,
            Event::Session(_) => Origin::Session,
            Event::ExternalControl(_) => Origin::ExternalControl,
            Event::LongOperation(_) => Origin::LongOperation,
            Event::Maintenance(_) => Origin::Maintenance,
        }
    }
}

/// Changes in the audio engine's state, published by the playback monitor.
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    /// The play state, the volume, the source sample rate or the ReplayGain
    /// values changed.
    StateChanged(Arc<PlaybackSnapshot>),
    /// The position or the duration changed.
    PositionChanged {
        /// Current position, in seconds.
        position_secs: f64,
        /// Duration of the current track, in seconds (0 while unknown).
        duration_secs: f64,
    },
    /// The current track changed (path, title, artist, album or cover).
    TrackChanged(Arc<PlaybackSnapshot>),
    /// The engine reached the end of a track: either it promoted the
    /// preloaded track (the snapshot is still playing, showing the new track)
    /// or playback stopped. The monitor never clears the flag: the UI calls
    /// [`PlayerPort::take_eof`](super::ports::PlayerPort::take_eof), which
    /// returns `true` once per end of track, and advances only then.
    EndOfTrack(Arc<PlaybackSnapshot>),
    /// The engine's non-modal notice appeared, changed or went away.
    NoticeChanged(Option<String>),
    /// The output device or its stream format changed.
    DeviceChanged(DeviceSnapshot),
}

/// The subset of [`AudioState`] a UI shows, as published by the monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackSnapshot {
    /// Whether the engine is playing.
    pub is_playing: bool,
    /// Volume, 0.0 to 1.0.
    pub volume: f32,
    /// Current position, in seconds.
    pub position_secs: f64,
    /// Duration of the current track, in seconds (0 while unknown).
    pub duration_secs: f64,
    /// Path of the current track (empty when nothing is loaded).
    pub path: String,
    /// Title of the current track.
    pub title: String,
    /// Artist of the current track.
    pub artist: String,
    /// Album of the current track.
    pub album: String,
    /// Cached cover of the current track, if any.
    pub cover_path: Option<String>,
    /// Whether the engine has flagged the end of a track that the UI has not
    /// taken yet.
    pub eof_reached: bool,
    /// Sample rate of the source file, in Hz.
    pub source_sample_rate: u32,
    /// Track ReplayGain applied to the current track, in dB.
    pub replay_gain_track: Option<f32>,
    /// Album ReplayGain applied to the current track, in dB.
    pub replay_gain_album: Option<f32>,
    /// The output device and stream format.
    pub device: DeviceSnapshot,
    /// The engine's non-modal notice, if any.
    pub audio_notice: Option<String>,
}

impl PlaybackSnapshot {
    /// Copies the UI-relevant fields out of the engine state.
    pub fn from_state(state: &AudioState) -> Self {
        Self {
            is_playing: state.is_playing,
            volume: state.volume,
            position_secs: state.current_pos_sec,
            duration_secs: state.total_duration_sec,
            path: state.path.clone(),
            title: state.title.clone(),
            artist: state.artist.clone(),
            album: state.album.clone(),
            cover_path: state.cover_path.clone(),
            eof_reached: state.eof_reached,
            source_sample_rate: state.sample_rate,
            replay_gain_track: state.replay_gain_track,
            replay_gain_album: state.replay_gain_album,
            device: DeviceSnapshot::from_state(state),
            audio_notice: state.audio_notice.clone(),
        }
    }
}

/// The output device and the format of its stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSnapshot {
    /// Stable id of the output device, when known.
    pub device_id: Option<String>,
    /// Sample rate of the output stream, in Hz.
    pub sample_rate: u32,
    /// Channel count of the output stream.
    pub channels: u16,
    /// Human-readable sample format ("32-bit Float", ...).
    pub bit_depth_display: String,
    /// Fixed buffer size in frames, or 0 when the backend chooses.
    pub buffer_size: u32,
}

impl DeviceSnapshot {
    /// Copies the device fields out of the engine state.
    pub fn from_state(state: &AudioState) -> Self {
        Self {
            device_id: state.device_id.clone(),
            sample_rate: state.device_sample_rate,
            channels: state.channels,
            bit_depth_display: state.bit_depth_display.clone(),
            buffer_size: state.buffer_size,
        }
    }
}

/// Changes in the library database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryEvent {
    /// The library content changed (the scanner flagged new data, or a command
    /// edited it); views should reload.
    Changed,
    /// A folder scan started.
    ScanStarted,
    /// The folder scan finished.
    ScanFinished,
}

/// Changes in the playlists, published by the playlist commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistsEvent {
    /// A playlist was created, renamed, reordered or deleted.
    ListChanged,
    /// The items of one playlist changed.
    ContentChanged {
        /// The playlist whose items changed.
        playlist_id: i64,
    },
}

/// Session lifecycle events.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// The startup restore finished. The UI sends no playback command before
    /// this event (quirk fix 6).
    Restored(RestoredSession),
    /// While playing, the autosave interval elapsed: the UI persists the
    /// playlist position (quirk fix 13).
    AutosaveDue,
}

/// What the startup restore put back.
#[derive(Debug, Clone, PartialEq)]
pub struct RestoredSession {
    /// The active playlist.
    pub playlist_id: i64,
    /// The song loaded in the player, if any.
    pub song_id: Option<i64>,
    /// The restored position, in seconds.
    pub position_secs: f64,
    /// Whether playback was resumed.
    pub is_playing: bool,
    /// Whether shuffle is on for the playlist.
    pub shuffle: bool,
    /// The playlist's repeat mode.
    pub repeat: RepeatMode,
}

/// Identifier of a long operation, unique for the life of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpId(pub u64);

/// What a long operation does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LongOpKind {
    /// A library folder scan.
    Scan,
    /// Importing an M3U/M3U8 file as a playlist.
    ImportPlaylist,
    /// Adding external files and folders to a playlist.
    ExternalBatch,
    /// Exporting a playlist (single file or portable copy).
    ExportPlaylist,
    /// Deleting songs from the library.
    LibraryDelete,
    /// Building the library filter index.
    FilterIndexBuild,
}

/// The state of a long operation, carried by every [`LongOpEvent`].
#[derive(Debug, Clone, PartialEq)]
pub struct LongOpInfo {
    /// The operation.
    pub op_id: OpId,
    /// What it does.
    pub kind: LongOpKind,
    /// Progress from 0.0 to 1.0, or `None` while indeterminate.
    pub progress: Option<f32>,
    /// Why it failed (`Failed` only).
    pub error: Option<String>,
    /// Whether the failure is a cancellation (`Failed` only). A UI shows an
    /// error toast for a failure, not for a cancellation.
    pub cancelled: bool,
}

/// Progress and outcome of a long operation.
#[derive(Debug, Clone, PartialEq)]
pub enum LongOpEvent {
    /// The operation reported progress.
    Progress(LongOpInfo),
    /// The operation succeeded; its result is ready for
    /// [`LongOperations::take_result`](super::long_ops::LongOperations::take_result).
    Completed(LongOpInfo),
    /// The operation failed or was cancelled; its error is ready for
    /// [`LongOperations::take_result`](super::long_ops::LongOperations::take_result).
    Failed(LongOpInfo),
}

/// Memory maintenance requests, published by the playback monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceEvent {
    /// A memory purge was triggered. The facade has already queued the backend
    /// part (string interner, preload buffer, idle stream buffers, returning
    /// memory to the OS); the UI trims its own caches (covers, view models).
    PurgeRequested(PurgeReason),
}

/// Why the playback monitor requested a memory purge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeReason {
    /// System RAM use is over the hard cap (75 %).
    RamHardCap,
    /// The periodic 60 s purge.
    Timer,
    /// The player's own RAM is over 500 MB; at most once every 10 s.
    RamLimit,
    /// A library scan finished 40 s ago. Iced also drops 32 covers from its
    /// cache on this one.
    ScanFinished,
}

/// The callback type subscribers register.
pub type EventCallback = Arc<dyn Fn(Event) + Send + Sync + 'static>;

struct Subscriber {
    id: u64,
    origin: Option<Origin>,
    callback: EventCallback,
    active: Arc<AtomicBool>,
}

enum HubMessage {
    Event(Event),
    Stop,
}

struct HubShared {
    subscribers: RwLock<Vec<Subscriber>>,
    next_id: AtomicU64,
    stopped: AtomicBool,
    tx: crossbeam::channel::Sender<HubMessage>,
}

impl HubShared {
    fn remove(&self, id: u64) {
        self.subscribers.write().retain(|s| s.id != id);
    }

    fn dispatch(&self, event: Event) {
        let origin = event.origin();
        let targets: Vec<(EventCallback, Arc<AtomicBool>)> = self
            .subscribers
            .read()
            .iter()
            .filter(|s| s.origin.is_none_or(|o| o == origin))
            .map(|s| (Arc::clone(&s.callback), Arc::clone(&s.active)))
            .collect();
        for (callback, active) in targets {
            if !active.load(Ordering::Acquire) {
                continue;
            }
            let event = event.clone();
            if std::panic::catch_unwind(AssertUnwindSafe(|| callback(event))).is_err() {
                tracing::error!(?origin, "an event subscriber panicked");
            }
        }
    }
}

/// The event hub. Clones share one registry and one dispatch thread.
#[derive(Clone)]
pub struct EventHub {
    shared: Arc<HubShared>,
    thread: Arc<WorkerThread>,
}

impl std::fmt::Debug for EventHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHub")
            .field("subscribers", &self.shared.subscribers.read().len())
            .field("stopped", &self.shared.stopped.load(Ordering::Acquire))
            .finish()
    }
}

/// Keeps a subscription alive; dropping it unsubscribes.
///
/// After the drop no new event is delivered to the callback, except one the
/// dispatch thread had already started delivering.
#[must_use = "dropping the token unsubscribes at once"]
pub struct SubscriptionToken {
    hub: Weak<HubShared>,
    id: u64,
    active: Arc<AtomicBool>,
}

impl std::fmt::Debug for SubscriptionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionToken")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for SubscriptionToken {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        if let Some(hub) = self.hub.upgrade() {
            hub.remove(self.id);
        }
    }
}

impl EventHub {
    /// Starts the hub and its dispatch thread.
    pub fn new() -> Result<Self, FrontendError> {
        let (tx, rx) = crossbeam::channel::unbounded::<HubMessage>();
        let shared = Arc::new(HubShared {
            subscribers: RwLock::new(Vec::new()),
            next_id: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
            tx,
        });
        // The thread holds only a weak reference, so dropping the last hub
        // clone lets the registry go even if the thread lingers.
        let weak = Arc::downgrade(&shared);
        let thread = WorkerThread::spawn("audoxidy-events", move || {
            while let Ok(HubMessage::Event(event)) = rx.recv() {
                match weak.upgrade() {
                    Some(shared) => shared.dispatch(event),
                    None => break,
                }
            }
        })?;
        Ok(Self {
            shared,
            thread: Arc::new(thread),
        })
    }

    /// Subscribes `callback` to the events of `origin`.
    ///
    /// The callback runs on the hub's dispatch thread. Keep the returned token
    /// for as long as the subscription should last.
    pub fn subscribe(&self, origin: Origin, callback: EventCallback) -> SubscriptionToken {
        self.add_subscriber(Some(origin), callback)
    }

    /// Subscribes `callback` to every event, whatever its origin (for logging
    /// and tests).
    pub fn subscribe_all(&self, callback: EventCallback) -> SubscriptionToken {
        self.add_subscriber(None, callback)
    }

    fn add_subscriber(&self, origin: Option<Origin>, callback: EventCallback) -> SubscriptionToken {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let active = Arc::new(AtomicBool::new(true));
        self.shared.subscribers.write().push(Subscriber {
            id,
            origin,
            callback,
            active: Arc::clone(&active),
        });
        SubscriptionToken {
            hub: Arc::downgrade(&self.shared),
            id,
            active,
        }
    }

    /// Publishes `event` to the subscribers of its origin.
    ///
    /// Never blocks: the event is queued for the dispatch thread. After
    /// [`EventHub::shutdown`] it is dropped.
    pub fn publish(&self, event: Event) {
        if self.shared.stopped.load(Ordering::Acquire) {
            return;
        }
        let _ = self.shared.tx.send(HubMessage::Event(event));
    }

    /// The number of live subscriptions.
    pub fn subscriber_count(&self) -> usize {
        self.shared.subscribers.read().len()
    }

    /// Stops the dispatch thread once the events already published are
    /// delivered, waiting at most `timeout`. Returns `true` when the thread
    /// has stopped. Later calls are no-ops.
    pub fn shutdown(&self, timeout: Duration) -> bool {
        if !self.shared.stopped.swap(true, Ordering::AcqRel) {
            let _ = self.shared.tx.send(HubMessage::Stop);
        }
        self.thread.join_within(timeout)
    }

    /// Whether the dispatch thread has been joined.
    pub fn is_stopped(&self) -> bool {
        self.thread.is_joined()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::mpsc;

    fn library(event: LibraryEvent) -> Event {
        Event::Library(event)
    }

    fn collector(hub: &EventHub, origin: Origin) -> (SubscriptionToken, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let tx = parking_lot::Mutex::new(tx);
        let token = hub.subscribe(
            origin,
            Arc::new(move |event| {
                let _ = tx.lock().send(event);
            }),
        );
        (token, rx)
    }

    #[test]
    fn a_subscriber_receives_events_until_its_token_is_dropped() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let (token, rx) = collector(&hub, Origin::Library);
            assert_eq!(hub.subscriber_count(), 1);
            hub.publish(library(LibraryEvent::Changed));
            assert!(matches!(
                rx.recv_timeout(TEST_TIMEOUT).unwrap(),
                Event::Library(LibraryEvent::Changed)
            ));

            drop(token);
            assert_eq!(hub.subscriber_count(), 0);
            hub.publish(library(LibraryEvent::ScanStarted));
            assert!(hub.shutdown(TEST_TIMEOUT));
            assert!(
                rx.try_recv().is_err(),
                "nothing is delivered after the drop"
            );
        });
    }

    #[test]
    fn only_events_of_the_subscribed_origin_are_delivered() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let (_library, library_rx) = collector(&hub, Origin::Library);
            let (_session, session_rx) = collector(&hub, Origin::Session);
            hub.publish(Event::Session(SessionEvent::AutosaveDue));
            hub.publish(library(LibraryEvent::ScanFinished));
            assert!(hub.shutdown(TEST_TIMEOUT));

            let library_events: Vec<Event> = library_rx.try_iter().collect();
            let session_events: Vec<Event> = session_rx.try_iter().collect();
            assert_eq!(library_events.len(), 1);
            assert!(matches!(
                library_events[0],
                Event::Library(LibraryEvent::ScanFinished)
            ));
            assert_eq!(session_events.len(), 1);
            assert!(matches!(
                session_events[0],
                Event::Session(SessionEvent::AutosaveDue)
            ));
        });
    }

    #[test]
    fn events_of_one_publisher_arrive_in_publication_order() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let (_token, rx) = collector(&hub, Origin::Playlists);
            let publisher = hub.clone();
            std::thread::spawn(move || {
                for id in 0..500 {
                    publisher.publish(Event::Playlists(PlaylistsEvent::ContentChanged {
                        playlist_id: id,
                    }));
                }
            })
            .join()
            .unwrap();
            assert!(hub.shutdown(TEST_TIMEOUT));
            let ids: Vec<i64> = rx
                .try_iter()
                .filter_map(|event| match event {
                    Event::Playlists(PlaylistsEvent::ContentChanged { playlist_id }) => {
                        Some(playlist_id)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(ids, (0..500).collect::<Vec<_>>());
        });
    }

    #[test]
    fn a_slow_subscriber_does_not_block_publishing() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let release_rx = parking_lot::Mutex::new(release_rx);
            let _slow = hub.subscribe(
                Origin::Library,
                Arc::new(move |_| {
                    let _ = release_rx.lock().recv_timeout(TEST_TIMEOUT);
                }),
            );
            let (_fast, fast_rx) = collector(&hub, Origin::Library);

            let started = std::time::Instant::now();
            for _ in 0..1000 {
                hub.publish(library(LibraryEvent::Changed));
            }
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "publishing waited for the blocked subscriber"
            );
            assert!(
                fast_rx.try_recv().is_err(),
                "the subscriber is still blocked"
            );

            for _ in 0..1000 {
                let _ = release_tx.send(());
            }
            assert!(hub.shutdown(TEST_TIMEOUT));
            assert_eq!(fast_rx.try_iter().count(), 1000);
        });
    }

    #[test]
    fn a_panicking_subscriber_does_not_stop_the_dispatch() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let _bad = hub.subscribe(Origin::Library, Arc::new(|_| panic!("subscriber bug")));
            let (_good, rx) = collector(&hub, Origin::Library);
            hub.publish(library(LibraryEvent::Changed));
            hub.publish(library(LibraryEvent::Changed));
            assert!(hub.shutdown(TEST_TIMEOUT));
            assert_eq!(rx.try_iter().count(), 2);
        });
    }

    #[test]
    fn shutdown_is_idempotent_and_drops_later_events() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let (_token, rx) = collector(&hub, Origin::Library);
            assert!(hub.shutdown(TEST_TIMEOUT));
            assert!(hub.is_stopped());
            assert!(hub.shutdown(TEST_TIMEOUT));
            hub.publish(library(LibraryEvent::Changed));
            assert!(rx.try_recv().is_err());
        });
    }

    #[test]
    fn the_types_meet_the_event_source_contract() {
        fn origin<T: Clone + Eq + std::hash::Hash + 'static>() {}
        fn event<T: Clone + Send + 'static>() {}
        fn token<T: Send + Sync + 'static>() {}
        fn callback(_: EventCallback) {}
        origin::<Origin>();
        event::<Event>();
        token::<SubscriptionToken>();
        callback(Arc::new(|_: Event| {}));
    }

    #[test]
    fn a_subscriber_can_drop_its_own_token_from_its_callback() {
        with_timeout(TEST_TIMEOUT, || {
            let hub = EventHub::new().unwrap();
            let slot: Arc<parking_lot::Mutex<Option<SubscriptionToken>>> =
                Arc::new(parking_lot::Mutex::new(None));
            let calls = Arc::new(AtomicU64::new(0));
            let (own_slot, own_calls) = (Arc::clone(&slot), Arc::clone(&calls));
            let token = hub.subscribe(
                Origin::Library,
                Arc::new(move |_| {
                    own_calls.fetch_add(1, Ordering::Relaxed);
                    drop(own_slot.lock().take());
                }),
            );
            *slot.lock() = Some(token);
            hub.publish(library(LibraryEvent::Changed));
            hub.publish(library(LibraryEvent::Changed));
            assert!(hub.shutdown(TEST_TIMEOUT));
            assert_eq!(calls.load(Ordering::Relaxed), 1);
            assert_eq!(hub.subscriber_count(), 0);
        });
    }

    #[test]
    fn every_event_reports_its_origin() {
        let info = LongOpInfo {
            op_id: OpId(1),
            kind: LongOpKind::Scan,
            progress: None,
            error: None,
            cancelled: false,
        };
        let cases = [
            (
                Event::Playback(PlaybackEvent::NoticeChanged(None)),
                Origin::Playback,
            ),
            (library(LibraryEvent::Changed), Origin::Library),
            (
                Event::Playlists(PlaylistsEvent::ListChanged),
                Origin::Playlists,
            ),
            (Event::Session(SessionEvent::AutosaveDue), Origin::Session),
            (
                Event::ExternalControl(ExternalControlEvent::Next),
                Origin::ExternalControl,
            ),
            (
                Event::LongOperation(LongOpEvent::Progress(info)),
                Origin::LongOperation,
            ),
            (
                Event::Maintenance(MaintenanceEvent::PurgeRequested(PurgeReason::Timer)),
                Origin::Maintenance,
            ),
        ];
        for (event, origin) in cases {
            assert_eq!(event.origin(), origin);
        }
    }
}
