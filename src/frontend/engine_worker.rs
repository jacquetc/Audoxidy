//! The engine worker: one ordered thread for the slow engine and system calls.
//!
//! Rebuilding the output stream sleeps for tens to hundreds of milliseconds,
//! enumerating devices talks to the audio host, and the sound-server calls
//! start `systemctl`, `pw-metadata` or `pactl`. The Iced UI makes these calls
//! on its UI thread (quirk fix 5). Here they are jobs on one thread, run in
//! submission order, so that "apply the PipeWire clock, then rebuild the
//! stream" keeps its order without the UI waiting for either.
//!
//! Buffer purges get special treatment. Iced purges the stream buffers when
//! playback pauses and from the memory purge when nothing plays. Here
//! [`EngineWorker::request_purge`] coalesces repeated requests into one job,
//! and that job checks `is_playing()` right before it runs: if playback
//! resumed meanwhile (a quick pause-then-play), it does nothing instead of
//! rebuilding a live stream.

#![warn(missing_docs)]

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use super::error::FrontendError;
use super::ports::{PlayerPort, SystemAudioPort};
use super::reply::{Reply, ReplySender, reply_channel};
use super::threading::{JobQueue, WorkerThread};
use crate::audio::engine::{AudioDeviceInfo, AudioSettings};
use crate::utils::interner::clear_interner;
use crate::utils::memory_manager::MemoryManager;

/// The worker name used in [`FrontendError::WorkerStopped`].
pub const ENGINE_WORKER: &str = "engine";

/// How long the PipeWire graph is given to renegotiate after its clock is
/// forced, before the stream is rebuilt (as in Iced).
pub const PIPEWIRE_SETTLE: Duration = Duration::from_millis(250);

/// How long dropping the last [`EngineWorker`] handle waits for the thread.
const DROP_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// What a buffer purge did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurgeOutcome {
    /// The stream buffers were purged and the stream rebuilt.
    Purged,
    /// Playback was running when the purge was about to start, so it was
    /// skipped.
    SkippedWhilePlaying,
    /// The engine refused the purge (for example, no active output).
    Failed(String),
}

/// The sound server that took the clock settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundServer {
    /// PipeWire: the clock was forced live, then persisted.
    PipeWire,
    /// PulseAudio: the clock was persisted for the next service start.
    PulseAudio,
}

/// What an engine job can use.
pub struct EngineContext {
    /// The audio engine.
    pub player: Arc<dyn PlayerPort>,
    /// The system sound server.
    pub system_audio: Arc<dyn SystemAudioPort>,
}

impl EngineContext {
    /// Applies the sound-server clock as Iced does before it builds a stream:
    /// with PipeWire, force the clock live, wait [`PIPEWIRE_SETTLE`], then
    /// persist it; with PulseAudio, only persist it. `None` means the
    /// server's default.
    ///
    /// This is the one implementation of that sequence: Apply, the startup
    /// restore and [`EngineWorker::apply_system_clock`] all run it, as a job
    /// on the engine worker, since it starts subprocesses and sleeps.
    pub fn apply_system_clock(&self, rate: Option<u32>, quantum: Option<u32>) -> SoundServer {
        let system = &self.system_audio;
        if system.is_pipewire_active() {
            system.apply_pipewire_clock(rate, quantum);
            std::thread::sleep(PIPEWIRE_SETTLE);
            system.persist_pipewire_conf(rate, quantum);
            SoundServer::PipeWire
        } else {
            system.persist_pulse_conf(rate, quantum);
            SoundServer::PulseAudio
        }
    }
}

type EngineTask = Box<dyn FnOnce(&EngineContext) + Send>;

enum EngineJob {
    Purge {
        waiters: Vec<ReplySender<PurgeOutcome>>,
    },
    Task(EngineTask),
}

struct EngineShared {
    queue: JobQueue<EngineJob>,
    context: EngineContext,
}

struct EngineWorkerInner {
    shared: Arc<EngineShared>,
    thread: WorkerThread,
}

impl Drop for EngineWorkerInner {
    fn drop(&mut self) {
        self.shared.queue.close();
        self.thread.join_within(DROP_JOIN_TIMEOUT);
    }
}

/// Handle to the engine worker. Clones share the thread; it stops when
/// [`EngineWorker::shutdown`] is called or the last clone is dropped, after
/// running the jobs already queued.
#[derive(Clone)]
pub struct EngineWorker {
    inner: Arc<EngineWorkerInner>,
}

impl std::fmt::Debug for EngineWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineWorker")
            .field("pending_jobs", &self.pending_jobs())
            .field("stopped", &self.is_stopped())
            .finish()
    }
}

impl EngineWorker {
    /// Starts the worker thread.
    pub fn new(
        player: Arc<dyn PlayerPort>,
        system_audio: Arc<dyn SystemAudioPort>,
    ) -> Result<Self, FrontendError> {
        let shared = Arc::new(EngineShared {
            queue: JobQueue::new(),
            context: EngineContext {
                player,
                system_audio,
            },
        });
        let worker_shared = Arc::clone(&shared);
        let thread = WorkerThread::spawn("audoxidy-engine", move || run_worker(&worker_shared))?;
        Ok(Self {
            inner: Arc::new(EngineWorkerInner { shared, thread }),
        })
    }

    /// Queues an arbitrary job, for sequences the other methods do not cover.
    /// It runs after every job submitted before it.
    pub fn run<T, F>(&self, f: F) -> Reply<T>
    where
        T: Send + 'static,
        F: FnOnce(&EngineContext) -> Result<T, FrontendError> + Send + 'static,
    {
        let (tx, reply) = reply_channel(ENGINE_WORKER);
        let task: EngineTask = Box::new(move |context: &EngineContext| {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| f(context)))
                .unwrap_or_else(|_| Err(FrontendError::JobPanicked(ENGINE_WORKER)));
            tx.send(result);
        });
        let _ = self.inner.shared.queue.push(EngineJob::Task(task));
        reply
    }

    /// Requests a purge of the stream buffers.
    ///
    /// If a purge is already queued and not started, this request joins it:
    /// both replies get its outcome. Right before it runs, the purge checks
    /// whether the engine is playing, and skips if so.
    pub fn request_purge(&self) -> Reply<PurgeOutcome> {
        let (tx, reply) = reply_channel(ENGINE_WORKER);
        let _ = self.inner.shared.queue.modify(move |jobs| {
            enqueue_purge(jobs, tx);
        });
        reply
    }

    /// Runs the backend part of the global memory purge, in Iced's order:
    /// clear the string interner, free the preload's memory, purge the stream
    /// buffers if nothing plays (a coalesced [`EngineWorker::request_purge`]),
    /// then return free memory to the OS. The reply is the buffer purge's
    /// outcome. The UI's own caches (covers, view models) are not touched.
    pub fn run_backend_purge(&self) -> Reply<PurgeOutcome> {
        let (tx, reply) = reply_channel(ENGINE_WORKER);
        let _ = self.inner.shared.queue.modify(move |jobs| {
            jobs.push_back(EngineJob::Task(Box::new(|context: &EngineContext| {
                clear_interner();
                if let Err(e) = context.player.purge_preload() {
                    tracing::debug!("the memory purge could not free the preload: {e}");
                }
            })));
            enqueue_purge(jobs, tx);
            jobs.push_back(EngineJob::Task(Box::new(|_: &EngineContext| {
                MemoryManager::force_free_to_os();
            })));
        });
        reply
    }

    /// Applies a device configuration and rebuilds the stream.
    pub fn apply_audio_settings(&self, settings: AudioSettings) -> Reply<()> {
        self.run(move |context| Ok(context.player.apply_audio_settings(settings)?))
    }

    /// Applies the sound-server clock as a job, with
    /// [`EngineContext::apply_system_clock`].
    pub fn apply_system_clock(
        &self,
        rate: Option<u32>,
        quantum: Option<u32>,
    ) -> Reply<SoundServer> {
        self.run(move |context| Ok(context.apply_system_clock(rate, quantum)))
    }

    /// Restarts PipeWire, its PulseAudio bridge and WirePlumber.
    pub fn restart_audio_services(&self) -> Reply<()> {
        self.run(|context| {
            context.system_audio.restart_audio_services();
            Ok(())
        })
    }

    /// The sound server's `(status, latency)` description.
    pub fn system_sound_status(&self) -> Reply<(String, String)> {
        self.run(|context| Ok(context.system_audio.system_sound_status()))
    }

    /// The output devices.
    pub fn devices(&self) -> Reply<Vec<AudioDeviceInfo>> {
        self.run(|context| Ok(context.player.devices()))
    }

    /// The audio hosts.
    pub fn available_hosts(&self) -> Reply<Vec<String>> {
        self.run(|context| Ok(context.player.available_hosts()))
    }

    /// The number of jobs waiting to start.
    pub fn pending_jobs(&self) -> usize {
        self.inner.shared.queue.len()
    }

    /// Refuses new jobs, lets the worker finish the queued ones, and waits at
    /// most `timeout` for its thread. Returns `true` when the thread has
    /// stopped. Later calls only wait again.
    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shared.queue.close();
        self.inner.thread.join_within(timeout)
    }

    /// Whether the worker thread has been joined.
    pub fn is_stopped(&self) -> bool {
        self.inner.thread.is_joined()
    }

    /// Whether the worker refuses new jobs.
    pub fn is_closed(&self) -> bool {
        self.inner.shared.queue.is_closed()
    }
}

/// Adds `waiter` to the pending purge, or queues a new purge.
fn enqueue_purge(
    jobs: &mut std::collections::VecDeque<EngineJob>,
    waiter: ReplySender<PurgeOutcome>,
) {
    let pending = jobs.iter_mut().find_map(|job| match job {
        EngineJob::Purge { waiters } => Some(waiters),
        EngineJob::Task(_) => None,
    });
    match pending {
        Some(waiters) => waiters.push(waiter),
        None => jobs.push_back(EngineJob::Purge {
            waiters: vec![waiter],
        }),
    }
}

fn run_purge(context: &EngineContext) -> PurgeOutcome {
    if context.player.is_playing() {
        return PurgeOutcome::SkippedWhilePlaying;
    }
    match std::panic::catch_unwind(AssertUnwindSafe(|| context.player.purge_buffers())) {
        Ok(Ok(())) => PurgeOutcome::Purged,
        Ok(Err(e)) => {
            tracing::debug!("the stream buffers could not be purged: {e}");
            PurgeOutcome::Failed(e.to_string())
        }
        Err(_) => PurgeOutcome::Failed("the purge panicked".to_string()),
    }
}

fn run_worker(shared: &EngineShared) {
    while let Some(job) = shared.queue.pop() {
        match job {
            EngineJob::Task(task) => task(&shared.context),
            EngineJob::Purge { waiters } => {
                let outcome = run_purge(&shared.context);
                for waiter in waiters {
                    waiter.send(Ok(outcome.clone()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::fakes::{FakePlayer, FakeSystemAudio, PlayerCall, SystemAudioCall};
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::mpsc;

    struct Rig {
        worker: EngineWorker,
        player: Arc<FakePlayer>,
        system: Arc<FakeSystemAudio>,
    }

    fn rig() -> Rig {
        let player = Arc::new(FakePlayer::new());
        let system = Arc::new(FakeSystemAudio::new());
        let worker = EngineWorker::new(player.clone(), system.clone()).unwrap();
        Rig {
            worker,
            player,
            system,
        }
    }

    /// Blocks the worker until the returned sender is used; returns once the
    /// blocking job has started.
    fn block_worker(worker: &EngineWorker) -> (mpsc::Sender<()>, Reply<()>) {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let blocker = worker.run(move |_| {
            let _ = started_tx.send(());
            let _ = release_rx.recv_timeout(TEST_TIMEOUT);
            Ok(())
        });
        started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        (release_tx, blocker)
    }

    fn purge_calls(player: &FakePlayer) -> usize {
        player
            .calls()
            .iter()
            .filter(|call| **call == PlayerCall::PurgeBuffers)
            .count()
    }

    #[test]
    fn a_purge_requested_while_paused_runs() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let outcome = rig.worker.request_purge().recv_blocking().unwrap();
            assert_eq!(outcome, PurgeOutcome::Purged);
            assert_eq!(purge_calls(&rig.player), 1);
        });
    }

    #[test]
    fn a_purge_followed_by_play_before_it_runs_is_skipped() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let (release, blocker) = block_worker(&rig.worker);
            let purge = rig.worker.request_purge();
            rig.player.play();
            release.send(()).unwrap();
            blocker.recv_blocking().unwrap();

            assert_eq!(
                purge.recv_blocking().unwrap(),
                PurgeOutcome::SkippedWhilePlaying
            );
            assert_eq!(purge_calls(&rig.player), 0);
        });
    }

    #[test]
    fn repeated_purge_requests_coalesce_into_one_purge() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let (release, blocker) = block_worker(&rig.worker);
            let replies: Vec<_> = (0..3).map(|_| rig.worker.request_purge()).collect();
            assert_eq!(rig.worker.pending_jobs(), 1);
            release.send(()).unwrap();
            blocker.recv_blocking().unwrap();

            for reply in replies {
                assert_eq!(reply.recv_blocking().unwrap(), PurgeOutcome::Purged);
            }
            assert_eq!(purge_calls(&rig.player), 1);
        });
    }

    #[test]
    fn engine_jobs_run_in_submission_order() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let (release, blocker) = block_worker(&rig.worker);
            let first = AudioSettings {
                sample_rate: Some(44_100),
                ..AudioSettings::default()
            };
            let second = AudioSettings {
                sample_rate: Some(96_000),
                ..AudioSettings::default()
            };
            let applied_first = rig.worker.apply_audio_settings(first.clone());
            let purge = rig.worker.request_purge();
            let devices = rig.worker.devices();
            let applied_second = rig.worker.apply_audio_settings(second.clone());
            release.send(()).unwrap();
            blocker.recv_blocking().unwrap();
            applied_first.recv_blocking().unwrap();
            assert_eq!(purge.recv_blocking().unwrap(), PurgeOutcome::Purged);
            assert_eq!(devices.recv_blocking().unwrap().len(), 1);
            applied_second.recv_blocking().unwrap();

            use crate::frontend::fakes::RecordedSettings;
            assert_eq!(
                rig.player.calls(),
                vec![
                    PlayerCall::ApplyAudioSettings(RecordedSettings(first)),
                    PlayerCall::PurgeBuffers,
                    PlayerCall::Devices,
                    PlayerCall::ApplyAudioSettings(RecordedSettings(second)),
                ]
            );
        });
    }

    #[test]
    fn the_pipewire_clock_is_forced_then_persisted_after_the_settle_wait() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let started = std::time::Instant::now();
            let server = rig
                .worker
                .apply_system_clock(Some(96_000), Some(1024))
                .recv_blocking()
                .unwrap();
            assert_eq!(server, SoundServer::PipeWire);
            assert!(started.elapsed() >= PIPEWIRE_SETTLE);
            assert_eq!(
                rig.system.calls(),
                vec![
                    SystemAudioCall::IsPipewireActive,
                    SystemAudioCall::ApplyPipewireClock(Some(96_000), Some(1024)),
                    SystemAudioCall::PersistPipewireConf(Some(96_000), Some(1024)),
                ]
            );
        });
    }

    #[test]
    fn with_pulseaudio_the_clock_is_only_persisted() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            rig.system.set_pipewire_active(false);
            let server = rig
                .worker
                .apply_system_clock(None, None)
                .recv_blocking()
                .unwrap();
            assert_eq!(server, SoundServer::PulseAudio);
            assert_eq!(
                rig.system.calls(),
                vec![
                    SystemAudioCall::IsPipewireActive,
                    SystemAudioCall::PersistPulseConf(None, None),
                ]
            );
        });
    }

    #[test]
    fn the_backend_purge_frees_the_preload_and_skips_the_buffers_while_playing() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            rig.player.play();
            let outcome = rig.worker.run_backend_purge().recv_blocking().unwrap();
            assert_eq!(outcome, PurgeOutcome::SkippedWhilePlaying);
            let calls = rig.player.calls();
            assert!(calls.contains(&PlayerCall::PurgePreload));
            assert!(!calls.contains(&PlayerCall::PurgeBuffers));
        });
    }

    #[test]
    fn the_system_status_and_device_lists_come_from_the_ports() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            rig.system.set_status("PulseAudio 44100 Hz", "512");
            let status = rig.worker.system_sound_status().recv_blocking().unwrap();
            assert_eq!(status.0, "PulseAudio 44100 Hz");
            let hosts = rig.worker.available_hosts().recv_blocking().unwrap();
            assert_eq!(hosts, vec!["Fake".to_string()]);
            rig.worker.restart_audio_services().recv_blocking().unwrap();
            assert!(
                rig.system
                    .calls()
                    .contains(&SystemAudioCall::RestartAudioServices)
            );
        });
    }

    #[test]
    fn shutdown_stops_the_thread_and_refuses_new_jobs() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            assert!(rig.worker.shutdown(TEST_TIMEOUT));
            assert!(rig.worker.is_stopped());
            assert!(matches!(
                rig.worker.request_purge().recv_blocking(),
                Err(FrontendError::WorkerStopped(ENGINE_WORKER))
            ));
            assert!(rig.worker.shutdown(TEST_TIMEOUT));
        });
    }
}
