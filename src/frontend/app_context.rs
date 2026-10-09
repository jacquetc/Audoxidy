//! The application context: everything a UI needs from the backend, wired once.
//!
//! [`AppContext`] owns the database worker, the scanner, the audio engine
//! (behind [`PlayerPort`]), the media controls, the sound server, the event
//! hub, the long operations, the engine worker, the preload bookkeeping and the
//! configuration, plus the playback monitor thread. It reproduces the wiring
//! of the Iced UI's `AudoxidyApp::new` without any UI toolkit: the database
//! connection shared with `AudioManager::set_database`, a second connection for
//! the scanner, and the media controls feeding the external-control channel.
//! Restoring the session and loading the library are commands, not part of
//! this wiring.
//!
//! # What the UI initialises before [`AppContext::production`]
//!
//! - The configuration (`utils::config::load_config`) and logging
//!   (`utils::observability::init_logging`).
//! - `MemoryManager::init()`, as Iced does at startup. The monitor keeps its
//!   own purge timers, so the facade itself does not depend on it.
//! - Low-resource detection (`utils::LOW_RESOURCE_MODE`): set it first, since
//!   the audio engine sizes its ring buffer from it.
//! - The working directory: the database is `library.db` and the covers are
//!   under `cache/covers/`, both relative to it, as in Iced.
//! - Optionally the deadlock watchdog (`deadlock-detection` feature).
//!
//! `production` opens the database and the audio device synchronously, so call
//! it before the UI's event loop starts. Then call
//! [`mark_ui_thread`](super::threading::mark_ui_thread) on the UI thread.
//!
//! Tests and the mocks build use `AppContext::in_memory` and
//! `AppContext::in_memory_with_fakes` instead, defined next to the fakes
//! (`frontend::fakes`): one in-memory database, no audio device.

#![warn(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;

use super::adapters::{
    AudioManagerPlayer, NoopMediaControls, SystemAudioAdapter, SystemMediaControlsAdapter,
};
use super::db_worker::{DbWorker, SharedDatabase};
use super::engine_worker::EngineWorker;
use super::error::FrontendError;
use super::events::EventHub;
use super::long_ops::LongOperations;
use super::monitor::{MemoryProbe, MonitorConfig, MonitorDeps, PlaybackMonitor, SystemMemoryProbe};
use super::ports::{MediaControlsPort, PlayerPort, SystemAudioPort};
use super::preload::PreloadState;
use crate::audio::AudioManager;
use crate::db::Database;
use crate::db::scanner::Scanner;
use crate::integrations::control::{ExternalControlEvent, external_control_channel};
use crate::integrations::media_controls::SystemMediaControls;
use crate::utils::config::AppConfig;

/// How long [`AppContext::shutdown`] waits for each thread.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// How long dropping the last context waits for each thread.
const DROP_TIMEOUT: Duration = Duration::from_secs(2);

/// The backend and the facade's threads; see the module documentation.
///
/// Cloning is cheap (every field is shared). The threads stop on
/// [`AppContext::shutdown`], or when the last clone is dropped.
#[derive(Clone)]
pub struct AppContext {
    /// The database worker: the only path to the database.
    pub db: DbWorker,
    /// The library scanner (its own connection in production, the shared one
    /// in memory).
    pub scanner: Arc<Scanner>,
    /// The audio engine.
    pub player: Arc<dyn PlayerPort>,
    /// The system media controls (a no-op when they are unavailable).
    pub media: Arc<dyn MediaControlsPort>,
    /// The system sound server.
    pub system_audio: Arc<dyn SystemAudioPort>,
    /// The event hub.
    pub hub: EventHub,
    /// The long operations.
    pub ops: LongOperations,
    /// The engine worker, for the slow engine and sound-server calls.
    pub engine: EngineWorker,
    /// The preload bookkeeping, written by the playback commands and read by
    /// the monitor.
    pub preload: Arc<PreloadState>,
    /// The configuration the context was built with.
    pub config: Arc<AppConfig>,
    /// Whether the startup restore was claimed (quirk fix 6); shared by every
    /// clone.
    startup_restore: Arc<AtomicBool>,
    lifecycle: Arc<Lifecycle>,
}

impl std::fmt::Debug for AppContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppContext")
            .field("db", &self.db)
            .field("engine", &self.engine)
            .field("hub", &self.hub)
            .field("ops", &self.ops)
            .finish_non_exhaustive()
    }
}

/// The pieces a context is assembled from (the real ones in
/// [`AppContext::production`], the fakes in `AppContext::in_memory`).
pub(super) struct Parts {
    pub(super) database: SharedDatabase,
    pub(super) scanner: Arc<Scanner>,
    pub(super) player: Arc<dyn PlayerPort>,
    pub(super) media: Arc<dyn MediaControlsPort>,
    pub(super) system_audio: Arc<dyn SystemAudioPort>,
    pub(super) external: crossbeam::channel::Receiver<ExternalControlEvent>,
    pub(super) probe: Arc<dyn MemoryProbe>,
    pub(super) config: AppConfig,
}

/// Stops every thread once: on `shutdown`, or when the last context clone is
/// dropped.
struct Lifecycle {
    monitor: PlaybackMonitor,
    ops: LongOperations,
    engine: EngineWorker,
    db: DbWorker,
    hub: EventHub,
    started: AtomicBool,
    outcome: Mutex<Option<bool>>,
}

impl Lifecycle {
    fn shutdown(&self, timeout: Duration) -> bool {
        // No lock is held while joining: a hub callback that asks
        // `is_shut_down` (or calls `shutdown`) must not stall the join.
        if self.started.swap(true, Ordering::AcqRel) {
            return self.outcome.lock().unwrap_or(false);
        }
        // The monitor first, so it queues nothing more; the hub last, so the
        // events the others publish while stopping are still delivered.
        let monitor = self.monitor.shutdown(timeout);
        let ops = self.ops.shutdown(timeout);
        let engine = self.engine.shutdown(timeout);
        let db = self.db.shutdown(timeout);
        let hub = self.hub.shutdown(timeout);
        let all_joined = monitor && ops && engine && db && hub;
        if !all_joined {
            tracing::warn!(
                monitor,
                ops,
                engine,
                db,
                hub,
                "some frontend threads did not stop in time"
            );
        }
        *self.outcome.lock() = Some(all_joined);
        all_joined
    }
}

impl Drop for Lifecycle {
    fn drop(&mut self) {
        self.shutdown(DROP_TIMEOUT);
    }
}

impl AppContext {
    /// Wires the real backend and starts the threads; see the module
    /// documentation for what must be initialised first.
    ///
    /// If the system media controls cannot be created, the context uses a
    /// no-op stand-in and logs a warning (the Iced UI aborts instead).
    pub fn production(config: &AppConfig) -> Result<Self, FrontendError> {
        let database: SharedDatabase = Arc::new(std::sync::Mutex::new(Database::new()?));
        // The scanner writes in long transactions: as in Iced, it gets its own
        // connection to the same file.
        let scanner = Arc::new(Scanner::new(Arc::new(std::sync::Mutex::new(
            Database::new()?,
        ))));
        let manager = Arc::new(AudioManager::new()?);
        manager.set_database(Arc::clone(&database));

        let (external_tx, external_rx) = external_control_channel();
        let media: Arc<dyn MediaControlsPort> = match SystemMediaControls::new(external_tx) {
            Ok(controls) => Arc::new(SystemMediaControlsAdapter::new(controls)),
            Err(e) => {
                tracing::warn!("system media controls unavailable, continuing without them: {e}");
                Arc::new(NoopMediaControls)
            }
        };

        Self::assemble(Parts {
            database,
            scanner,
            player: Arc::new(AudioManagerPlayer::new(manager)),
            media,
            system_audio: Arc::new(SystemAudioAdapter),
            external: external_rx,
            probe: Arc::new(SystemMemoryProbe),
            config: config.clone(),
        })
    }

    pub(super) fn assemble(parts: Parts) -> Result<Self, FrontendError> {
        let hub = EventHub::new()?;
        let db = DbWorker::new(parts.database)?;
        let engine = EngineWorker::new(Arc::clone(&parts.player), Arc::clone(&parts.system_audio))?;
        let ops = LongOperations::new(hub.clone());
        let preload = Arc::new(PreloadState::new());
        let monitor = PlaybackMonitor::spawn(MonitorDeps {
            player: Arc::clone(&parts.player),
            media: Arc::clone(&parts.media),
            hub: hub.clone(),
            engine: engine.clone(),
            preload: Arc::clone(&preload),
            db_dirty: Arc::clone(&parts.scanner.db_dirty),
            is_scanning: Arc::clone(&parts.scanner.is_scanning),
            external: parts.external,
            probe: parts.probe,
            config: MonitorConfig::from_app_config(&parts.config),
        })?;
        let lifecycle = Arc::new(Lifecycle {
            monitor,
            ops: ops.clone(),
            engine: engine.clone(),
            db: db.clone(),
            hub: hub.clone(),
            started: AtomicBool::new(false),
            outcome: Mutex::new(None),
        });
        Ok(Self {
            db,
            scanner: parts.scanner,
            player: parts.player,
            media: parts.media,
            system_audio: parts.system_audio,
            hub,
            ops,
            engine,
            preload,
            config: Arc::new(parts.config),
            startup_restore: Arc::new(AtomicBool::new(false)),
            lifecycle,
        })
    }

    /// Claims the startup restore: `true` on the first call on this context
    /// or any of its clones, `false` on every later one.
    ///
    /// The settings restore (`commands::engine_settings::restore_persisted`)
    /// and the session restore (`session::restore`, which includes the
    /// settings restore) both claim it, so whichever runs first is the only
    /// startup restore of the context (quirk fix 6).
    pub(crate) fn claim_startup_restore(&self) -> bool {
        !self.startup_restore.swap(true, Ordering::AcqRel)
    }

    /// Stops every thread, in order: the monitor, the long operations
    /// (cancelled), the engine worker and the database worker (each after its
    /// queued jobs), and the event hub (after its queued events). Each is
    /// given [`SHUTDOWN_TIMEOUT`]. Returns `true` when every thread was joined.
    ///
    /// Idempotent: later calls return at once, with the first call's result
    /// (`false` while the first call is still running).
    /// Requests made afterwards resolve to
    /// [`FrontendError::WorkerStopped`]. It does not stop playback; the
    /// session's stop command does that before calling this.
    pub fn shutdown(&self) -> bool {
        self.lifecycle.shutdown(SHUTDOWN_TIMEOUT)
    }

    /// Whether [`AppContext::shutdown`] has started.
    pub fn is_shut_down(&self) -> bool {
        self.lifecycle.started.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::events::{Event, Origin, PlaybackEvent};
    use crate::frontend::ports::{ReplayGains, TrackRequest};
    use crate::frontend::test_fixtures::LibraryFixture;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::mpsc;

    fn assert_send_sync<T: Send + Sync + 'static>() {}

    #[test]
    fn the_context_and_its_handles_can_be_shared_across_threads() {
        assert_send_sync::<AppContext>();
        assert_send_sync::<DbWorker>();
        assert_send_sync::<EngineWorker>();
        assert_send_sync::<EventHub>();
        assert_send_sync::<LongOperations>();
        assert_send_sync::<PreloadState>();
    }

    #[test]
    fn an_in_memory_context_builds_and_shuts_down_every_thread_in_time() {
        with_timeout(TEST_TIMEOUT, || {
            let context = AppContext::in_memory().unwrap();
            let started = std::time::Instant::now();
            assert!(context.shutdown());
            assert!(started.elapsed() < TEST_TIMEOUT);
            assert!(context.is_shut_down());
            assert!(context.db.is_stopped());
            assert!(context.engine.is_stopped());
            assert!(context.hub.is_stopped());
            assert!(context.lifecycle.monitor.is_stopped());
        });
    }

    #[test]
    fn a_second_shutdown_is_a_no_op() {
        with_timeout(TEST_TIMEOUT, || {
            let context = AppContext::in_memory().unwrap();
            assert!(context.shutdown());
            let started = std::time::Instant::now();
            assert!(context.clone().shutdown());
            assert!(started.elapsed() < Duration::from_millis(100));
            assert!(matches!(
                context.db.read(|_| Ok(())).recv_blocking(),
                Err(FrontendError::WorkerStopped(_))
            ));
        });
    }

    #[test]
    fn dropping_the_last_context_stops_every_thread() {
        with_timeout(TEST_TIMEOUT, || {
            let context = AppContext::in_memory().unwrap();
            let (db, engine, hub) = (
                context.db.clone(),
                context.engine.clone(),
                context.hub.clone(),
            );
            let clone = context.clone();
            drop(context);
            assert!(!db.is_stopped(), "a clone is still alive");
            drop(clone);
            assert!(db.is_stopped());
            assert!(engine.is_stopped());
            assert!(hub.is_stopped());
        });
    }

    #[test]
    fn the_startup_restore_is_claimed_once_across_clones() {
        with_timeout(TEST_TIMEOUT, || {
            let context = AppContext::in_memory().unwrap();
            let clone = context.clone();
            assert!(clone.claim_startup_restore());
            assert!(!context.claim_startup_restore());
            assert!(!clone.claim_startup_restore());
            let other = AppContext::in_memory().unwrap();
            assert!(other.claim_startup_restore(), "each context has its own");
            assert!(context.shutdown());
            assert!(other.shutdown());
        });
    }

    #[test]
    fn the_scanner_shares_the_in_memory_connection() {
        with_timeout(TEST_TIMEOUT, || {
            let context = AppContext::in_memory().unwrap();
            let connection = context.db.connection();
            // The worker, the scanner and this handle.
            assert_eq!(Arc::strong_count(&connection), 3);
            assert!(context.shutdown());
        });
    }

    #[test]
    fn the_in_memory_context_is_wired_end_to_end() {
        with_timeout(TEST_TIMEOUT, || {
            let (context, fakes) = AppContext::in_memory_with_fakes().unwrap();
            let seeded = LibraryFixture::demo()
                .seed_on(&context.db)
                .recv_blocking()
                .unwrap();
            assert_eq!(seeded.songs.len(), 11);

            let (tx, rx) = mpsc::channel();
            let tx = Mutex::new(tx);
            let _token = context.hub.subscribe(
                Origin::Playback,
                Arc::new(move |event| {
                    let _ = tx.lock().send(event);
                }),
            );
            let (ext_tx, ext_rx) = mpsc::channel();
            let ext_tx = Mutex::new(ext_tx);
            let _external = context.hub.subscribe(
                Origin::ExternalControl,
                Arc::new(move |event| {
                    let _ = ext_tx.lock().send(event);
                }),
            );

            let song = &seeded.songs[0];
            context
                .player
                .load_file(
                    &TrackRequest {
                        path: song.path.clone(),
                        title: song.title.clone(),
                        ..TrackRequest::default()
                    },
                    ReplayGains::default(),
                )
                .unwrap();
            context.player.play();
            let deadline = std::time::Instant::now() + TEST_TIMEOUT;
            loop {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if let Event::Playback(PlaybackEvent::TrackChanged(snapshot)) =
                    rx.recv_timeout(left).unwrap()
                    && snapshot.title == song.title
                {
                    break;
                }
            }

            fakes.external_control.send(ExternalControlEvent::Pause);
            assert!(matches!(
                ext_rx.recv_timeout(TEST_TIMEOUT).unwrap(),
                Event::ExternalControl(ExternalControlEvent::Pause)
            ));
            assert!(fakes.media.update_count() >= 1);
            assert!(context.shutdown());
        });
    }
}
