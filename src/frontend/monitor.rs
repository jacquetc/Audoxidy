//! The playback monitor: the thread that turns engine and system state into
//! events, and runs the periodic housekeeping Iced does in its `Tick`.
//!
//! The decision logic is the pure function [`poll_once`]: given the monitor's
//! memory of the last poll ([`MonitorState`]), the time, and what the thread
//! sampled ([`MonitorInputs`]), it returns the events to publish and the side
//! effects to run ([`MonitorAction`]). The thread ([`PlaybackMonitor`]) only
//! samples, calls `poll_once`, and executes the result, so every rule below is
//! tested without threads or clocks.
//!
//! What a poll does:
//! - **Diffs.** It compares the engine state with the previous poll and
//!   publishes only what changed (`Playback` events). It polls every 100 ms
//!   while playing and every 500 ms otherwise.
//! - **End of track.** It publishes `EndOfTrack` when `eof_reached` rises, and
//!   never clears the flag: the UI calls `PlayerPort::take_eof`, which returns
//!   `true` once per end of track.
//! - **Media controls.** It updates the system media controls only when
//!   something they show changed (play state, track, duration, the position to
//!   the second). Iced updates them on every message.
//! - **External control.** It forwards the media-control channel as
//!   `ExternalControl` events, as soon as an event arrives.
//! - **Scanner.** It turns the scanner's `db_dirty` and `is_scanning` flags into
//!   `Library` events.
//! - **Autosave.** While playing, it publishes `Session::AutosaveDue` every
//!   `behavior.auto_save_interval_secs` (quirk fix 13), whatever the UI does.
//! - **Housekeeping** (Iced's `Tick`, every 500 ms while playing or scanning,
//!   every 5 s otherwise): it logs the RAM, underrun and latency metrics, and
//!   runs the memory-purge triggers: the system RAM hard cap, the 60 s timer,
//!   the player RAM limit with its 10 s cooldown, and the purge 40 s after a
//!   scan ends. The timer and RAM-limit purges wait while a scan runs or a
//!   preload is in its safety window. Each purge publishes
//!   `Maintenance::PurgeRequested` for the UI's caches and queues the backend
//!   part on the engine worker.

#![warn(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use super::engine_worker::EngineWorker;
use super::error::FrontendError;
use super::events::{
    Event, EventHub, LibraryEvent, MaintenanceEvent, PlaybackEvent, PlaybackSnapshot, PurgeReason,
    SessionEvent,
};
use super::ports::{MediaControlsPort, PlayerPort};
use super::preload::{PreloadSnapshot, PreloadState};
use super::threading::WorkerThread;
use crate::audio::engine::{AudioState, LATENCY_PEAK_US, UNDERRUNS};
use crate::integrations::control::ExternalControlEvent;
use crate::utils::config::AppConfig;
use crate::utils::memory_manager::{MemoryManager, process_ram_over_limit, ram_purge_decision};

/// Poll interval while playing.
pub const POLL_WHILE_PLAYING: Duration = Duration::from_millis(100);
/// Poll interval while paused or stopped.
pub const POLL_WHILE_IDLE: Duration = Duration::from_millis(500);
/// Housekeeping interval while playing or scanning (Iced's `Tick`).
pub const HOUSEKEEPING_WHILE_BUSY: Duration = Duration::from_millis(500);
/// Housekeeping interval otherwise (Iced's idle `Tick`).
pub const HOUSEKEEPING_WHILE_IDLE: Duration = Duration::from_secs(5);
/// The periodic purge interval, as `MemoryManager::should_run_global_purge`.
pub const TIMER_PURGE_INTERVAL_SECS: u64 = 60;
/// How long after a scan ends its purge runs.
pub const SCAN_PURGE_DELAY_SECS: u64 = 40;

/// How long dropping the monitor waits for its thread.
const DROP_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// What the housekeeping reads from the process and the system.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemorySample {
    /// System RAM use is over the 75 % hard cap.
    pub system_over_hard_cap: bool,
    /// The player's own RAM is over its 500 MB limit.
    pub app_over_limit: bool,
    /// The player's resident memory, in bytes.
    pub process_ram_bytes: u64,
    /// Output underruns since the previous sample.
    pub underruns: u64,
    /// The engine's peak latency, in microseconds.
    pub latency_peak_us: u64,
}

/// Reads a [`MemorySample`]. Injected so tests never read real memory.
pub trait MemoryProbe: Send + Sync {
    /// Takes a sample. May consume counters (the underrun count is reset).
    fn sample(&self) -> MemorySample;
}

/// The real [`MemoryProbe`]: `MemoryManager` and the engine's counters.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemMemoryProbe;

impl MemoryProbe for SystemMemoryProbe {
    fn sample(&self) -> MemorySample {
        let process_ram_bytes = MemoryManager::current_process_ram_bytes();
        MemorySample {
            system_over_hard_cap: MemoryManager::is_ram_over_hard_cap(),
            app_over_limit: process_ram_over_limit(process_ram_bytes / (1024 * 1024)),
            process_ram_bytes,
            underruns: UNDERRUNS.swap(0, Ordering::Relaxed),
            latency_peak_us: LATENCY_PEAK_US.load(Ordering::Relaxed),
        }
    }
}

/// The monitor's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorConfig {
    /// How often to publish `AutosaveDue` while playing; `None` disables it.
    pub autosave_interval: Option<Duration>,
}

impl MonitorConfig {
    /// Reads `behavior.auto_save_interval_secs` (0 disables the autosave).
    pub fn from_app_config(config: &AppConfig) -> Self {
        let secs = config.behavior.auto_save_interval_secs;
        Self {
            autosave_interval: (secs > 0).then(|| Duration::from_secs(secs)),
        }
    }
}

/// What the thread sampled for one poll.
#[derive(Clone)]
pub struct MonitorInputs {
    /// The engine state.
    pub audio: AudioState,
    /// The wall clock, in UNIX seconds (the purge timers use it, as Iced).
    pub unix_secs: u64,
    /// Media-control events received since the previous poll, oldest first.
    pub external: Vec<ExternalControlEvent>,
    /// The scanner's `db_dirty` flag, already taken (swapped to `false`).
    pub library_dirty: bool,
    /// The scanner's `is_scanning` flag.
    pub is_scanning: bool,
    /// The preload bookkeeping.
    pub preload: PreloadSnapshot,
    /// A memory sample when housekeeping is due (see
    /// [`MonitorState::housekeeping_due`]); housekeeping runs only when this is
    /// `Some`.
    pub memory: Option<MemorySample>,
}

/// A side effect requested by [`poll_once`], run by the monitor thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorAction {
    /// Push the sampled state to the system media controls.
    UpdateMediaControls,
    /// Queue the backend part of the memory purge on the engine worker.
    BackendPurge,
    /// Log the Tick metrics from this sample.
    LogMetrics(MemorySample),
}

/// The result of one poll.
#[derive(Debug, Clone)]
pub struct PollOutput {
    /// Events to publish, in order.
    pub events: Vec<Event>,
    /// Side effects to run, in order.
    pub actions: Vec<MonitorAction>,
    /// When to poll next.
    pub next_poll_in: Duration,
}

/// What the media controls show; they are updated when it changes.
#[derive(Debug, Clone, PartialEq)]
struct MediaKey {
    is_playing: bool,
    path: String,
    title: String,
    artist: String,
    album: String,
    cover_path: Option<String>,
    duration_bits: u64,
    whole_second: i64,
}

impl MediaKey {
    fn of(snapshot: &PlaybackSnapshot) -> Self {
        Self {
            is_playing: snapshot.is_playing,
            path: snapshot.path.clone(),
            title: snapshot.title.clone(),
            artist: snapshot.artist.clone(),
            album: snapshot.album.clone(),
            cover_path: snapshot.cover_path.clone(),
            duration_bits: snapshot.duration_secs.to_bits(),
            whole_second: snapshot.position_secs.floor() as i64,
        }
    }
}

/// What the monitor remembers between polls.
#[derive(Debug, Clone)]
pub struct MonitorState {
    config: MonitorConfig,
    last: Option<PlaybackSnapshot>,
    media: Option<MediaKey>,
    was_scanning: bool,
    autosave_anchor: Option<Instant>,
    last_housekeeping: Option<Instant>,
    last_timer_purge_secs: u64,
    last_ram_purge_secs: u64,
    scan_finished_at: Option<u64>,
}

impl MonitorState {
    /// A monitor that has not polled yet. The 60 s purge timer starts at
    /// `unix_secs`, as Iced's starts at `MemoryManager::init`.
    pub fn new(config: MonitorConfig, unix_secs: u64) -> Self {
        Self {
            config,
            last: None,
            media: None,
            was_scanning: false,
            autosave_anchor: None,
            last_housekeeping: None,
            last_timer_purge_secs: unix_secs,
            last_ram_purge_secs: 0,
            scan_finished_at: None,
        }
    }

    /// Whether the thread should take a memory sample for the poll at `now`:
    /// every [`HOUSEKEEPING_WHILE_BUSY`] while playing or scanning (`busy`),
    /// every [`HOUSEKEEPING_WHILE_IDLE`] otherwise, and on the first poll.
    pub fn housekeeping_due(&self, now: Instant, busy: bool) -> bool {
        let interval = if busy {
            HOUSEKEEPING_WHILE_BUSY
        } else {
            HOUSEKEEPING_WHILE_IDLE
        };
        self.last_housekeeping
            .is_none_or(|last| now.saturating_duration_since(last) >= interval)
    }
}

/// Decides what one poll publishes and does; see the module documentation.
pub fn poll_once(state: &mut MonitorState, now: Instant, inputs: &MonitorInputs) -> PollOutput {
    let mut events = Vec::new();
    let mut actions = Vec::new();

    let snapshot = PlaybackSnapshot::from_state(&inputs.audio);
    diff_playback(state.last.as_ref(), &snapshot, &mut events);

    let media = MediaKey::of(&snapshot);
    if state.media.as_ref() != Some(&media) {
        actions.push(MonitorAction::UpdateMediaControls);
        state.media = Some(media);
    }

    events.extend(inputs.external.iter().copied().map(Event::ExternalControl));

    if inputs.is_scanning && !state.was_scanning {
        events.push(Event::Library(LibraryEvent::ScanStarted));
    }
    if inputs.library_dirty {
        events.push(Event::Library(LibraryEvent::Changed));
    }
    if state.was_scanning && !inputs.is_scanning {
        events.push(Event::Library(LibraryEvent::ScanFinished));
        state.scan_finished_at = Some(inputs.unix_secs);
    }
    state.was_scanning = inputs.is_scanning;

    if snapshot.is_playing {
        match (state.autosave_anchor, state.config.autosave_interval) {
            (None, _) => state.autosave_anchor = Some(now),
            (Some(anchor), Some(interval)) if now.saturating_duration_since(anchor) >= interval => {
                events.push(Event::Session(SessionEvent::AutosaveDue));
                state.autosave_anchor = Some(now);
            }
            _ => {}
        }
    } else {
        state.autosave_anchor = None;
    }

    if let Some(sample) = &inputs.memory {
        state.last_housekeeping = Some(now);
        actions.push(MonitorAction::LogMetrics(sample.clone()));
        if let Some(reason) = purge_trigger(state, inputs, sample) {
            events.push(Event::Maintenance(MaintenanceEvent::PurgeRequested(reason)));
            actions.push(MonitorAction::BackendPurge);
        }
    }

    let next_poll_in = if snapshot.is_playing {
        POLL_WHILE_PLAYING
    } else {
        POLL_WHILE_IDLE
    };
    state.last = Some(snapshot);
    PollOutput {
        events,
        actions,
        next_poll_in,
    }
}

/// Publishes the playback changes between `previous` and `current`.
fn diff_playback(
    previous: Option<&PlaybackSnapshot>,
    current: &PlaybackSnapshot,
    events: &mut Vec<Event>,
) {
    let changed = |differs: &dyn Fn(&PlaybackSnapshot) -> bool| previous.is_none_or(differs);
    let shared = Arc::new(current.clone());

    if changed(&|p| {
        p.path != current.path
            || p.title != current.title
            || p.artist != current.artist
            || p.album != current.album
            || p.cover_path != current.cover_path
    }) {
        events.push(Event::Playback(PlaybackEvent::TrackChanged(Arc::clone(
            &shared,
        ))));
    }
    if changed(&|p| {
        p.is_playing != current.is_playing
            || p.volume.to_bits() != current.volume.to_bits()
            || p.source_sample_rate != current.source_sample_rate
            || p.replay_gain_track != current.replay_gain_track
            || p.replay_gain_album != current.replay_gain_album
    }) {
        events.push(Event::Playback(PlaybackEvent::StateChanged(Arc::clone(
            &shared,
        ))));
    }
    if changed(&|p| {
        p.position_secs.to_bits() != current.position_secs.to_bits()
            || p.duration_secs.to_bits() != current.duration_secs.to_bits()
    }) {
        events.push(Event::Playback(PlaybackEvent::PositionChanged {
            position_secs: current.position_secs,
            duration_secs: current.duration_secs,
        }));
    }
    if changed(&|p| p.device != current.device) {
        events.push(Event::Playback(PlaybackEvent::DeviceChanged(
            current.device.clone(),
        )));
    }
    if changed(&|p| p.audio_notice != current.audio_notice) {
        events.push(Event::Playback(PlaybackEvent::NoticeChanged(
            current.audio_notice.clone(),
        )));
    }
    // The flag rises, or a new end of track was flagged on another track before
    // the monitor saw the previous one taken.
    if current.eof_reached && previous.is_none_or(|p| !p.eof_reached || p.path != current.path) {
        events.push(Event::Playback(PlaybackEvent::EndOfTrack(shared)));
    }
}

/// Iced's purge triggers, in its order: the hard cap first, then the timer
/// and the RAM limit (both held back during a scan or the preload window),
/// then the post-scan purge. At most one purge per housekeeping pass.
fn purge_trigger(
    state: &mut MonitorState,
    inputs: &MonitorInputs,
    sample: &MemorySample,
) -> Option<PurgeReason> {
    let now = inputs.unix_secs;
    if sample.system_over_hard_cap {
        return Some(PurgeReason::RamHardCap);
    }

    if !inputs.is_scanning && !inputs.preload.blocks_gc(now) {
        let timer_due =
            now.saturating_sub(state.last_timer_purge_secs) >= TIMER_PURGE_INTERVAL_SECS;
        if timer_due {
            state.last_timer_purge_secs = now;
        }
        // Iced also tests the system hard cap here, but it returned above.
        let ram_due =
            match ram_purge_decision(sample.app_over_limit, now, state.last_ram_purge_secs) {
                Some(accepted_at) => {
                    state.last_ram_purge_secs = accepted_at;
                    true
                }
                None => false,
            };
        if timer_due {
            return Some(PurgeReason::Timer);
        }
        if ram_due {
            return Some(PurgeReason::RamLimit);
        }
    }

    if let Some(finished_at) = state.scan_finished_at
        && now.saturating_sub(finished_at) >= SCAN_PURGE_DELAY_SECS
    {
        state.scan_finished_at = None;
        state.last_timer_purge_secs = now;
        return Some(PurgeReason::ScanFinished);
    }
    None
}

/// The current wall clock in UNIX seconds (0 if the clock is before 1970).
pub fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Logs the Tick metrics as Iced does (`target: "audoxidy::metrics"`).
fn log_metrics(sample: &MemorySample) {
    tracing::info!(
        target: "audoxidy::metrics",
        metric = "ram_usage_bytes",
        ram_usage_bytes = sample.process_ram_bytes
    );
    if sample.underruns > 0 {
        tracing::warn!(
            target: "audoxidy::metrics",
            metric = "underrun",
            count = sample.underruns
        );
    }
    if sample.latency_peak_us > 0 {
        tracing::info!(
            target: "audoxidy::metrics",
            metric = "audio_latency",
            latency_us = sample.latency_peak_us
        );
    }
}

/// Everything the monitor thread reads and drives.
pub struct MonitorDeps {
    /// The engine it polls.
    pub player: Arc<dyn PlayerPort>,
    /// The media controls it updates.
    pub media: Arc<dyn MediaControlsPort>,
    /// Where it publishes.
    pub hub: EventHub,
    /// Where it queues the backend purge.
    pub engine: EngineWorker,
    /// The preload bookkeeping it reads.
    pub preload: Arc<PreloadState>,
    /// The scanner's "new data" flag, which it takes.
    pub db_dirty: Arc<AtomicBool>,
    /// The scanner's "scanning" flag.
    pub is_scanning: Arc<AtomicBool>,
    /// The media-control channel it forwards.
    pub external: crossbeam::channel::Receiver<ExternalControlEvent>,
    /// Where it reads memory.
    pub probe: Arc<dyn MemoryProbe>,
    /// Its settings.
    pub config: MonitorConfig,
}

/// The monitor thread. It stops on [`PlaybackMonitor::shutdown`] or when
/// dropped.
pub struct PlaybackMonitor {
    stop: Mutex<Option<crossbeam::channel::Sender<()>>>,
    thread: WorkerThread,
}

impl std::fmt::Debug for PlaybackMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybackMonitor")
            .field("stopped", &self.is_stopped())
            .finish()
    }
}

impl PlaybackMonitor {
    /// Starts the monitor thread. Its first poll runs at once.
    pub fn spawn(deps: MonitorDeps) -> Result<Self, FrontendError> {
        let (stop_tx, stop_rx) = crossbeam::channel::bounded::<()>(1);
        let thread = WorkerThread::spawn("audoxidy-monitor", move || run_monitor(deps, &stop_rx))?;
        Ok(Self {
            stop: Mutex::new(Some(stop_tx)),
            thread,
        })
    }

    /// Stops the thread and waits at most `timeout` for it. Returns `true`
    /// when it has stopped. Later calls only wait again.
    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.stop.lock().take();
        self.thread.join_within(timeout)
    }

    /// Whether the thread has been joined.
    pub fn is_stopped(&self) -> bool {
        self.thread.is_joined()
    }
}

impl Drop for PlaybackMonitor {
    fn drop(&mut self) {
        self.shutdown(DROP_JOIN_TIMEOUT);
    }
}

fn run_monitor(deps: MonitorDeps, stop: &crossbeam::channel::Receiver<()>) {
    let mut state = MonitorState::new(deps.config, unix_now_secs());
    let mut external = deps.external.clone();
    let mut received = Vec::new();
    let mut next_poll = Instant::now();
    loop {
        let wait = next_poll.saturating_duration_since(Instant::now());
        let mut disconnected = false;
        crossbeam::channel::select! {
            recv(stop) -> _ => return,
            recv(external) -> message => match message {
                Ok(event) => {
                    received.push(event);
                    received.extend(external.try_iter());
                }
                Err(_) => disconnected = true,
            },
            default(wait) => {}
        }
        if disconnected {
            // No media controls (or they were dropped): stop listening.
            external = crossbeam::channel::never();
        }

        let now = Instant::now();
        let audio = deps.player.snapshot();
        let is_scanning = deps.is_scanning.load(Ordering::Relaxed);
        let memory = state
            .housekeeping_due(now, audio.is_playing || is_scanning)
            .then(|| deps.probe.sample());
        let inputs = MonitorInputs {
            audio,
            unix_secs: unix_now_secs(),
            external: std::mem::take(&mut received),
            library_dirty: deps.db_dirty.swap(false, Ordering::Relaxed),
            is_scanning,
            preload: deps.preload.snapshot(),
            memory,
        };
        let output = poll_once(&mut state, now, &inputs);
        for event in output.events {
            deps.hub.publish(event);
        }
        for action in output.actions {
            match action {
                MonitorAction::UpdateMediaControls => {
                    let media = &deps.media;
                    let audio = &inputs.audio;
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        media.update(audio);
                    }))
                    .is_err()
                    {
                        tracing::warn!("updating the system media controls panicked");
                    }
                }
                MonitorAction::BackendPurge => {
                    tracing::debug!("Audoxidy GC: Purging Memory (Ghost Mode)");
                    let _ = deps.engine.run_backend_purge();
                }
                MonitorAction::LogMetrics(sample) => log_metrics(&sample),
            }
        }
        next_poll = now + output.next_poll_in;
    }
}

#[cfg(test)]
mod tests;
