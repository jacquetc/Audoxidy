//! The database worker: the only path from the facade to SQLite.
//!
//! The Iced UI queries the database inside its `update`, on the UI thread
//! (quirk fix 5). Here every access is a job on one worker thread:
//!
//! - **One FIFO queue.** Reads and writes share it, so jobs run in submission
//!   order: a read submitted after a write sees it.
//! - **One connection, locked per job.** The worker holds the
//!   `Arc<std::sync::Mutex<Database>>` that `AudioManager::set_database` and the
//!   in-memory `Scanner` share, and locks it around each job, so code that takes
//!   that `Arc` and locks it itself (the scanner, `Scanner::import_m3u`) runs
//!   between two jobs, never inside one.
//! - **Replies.** Every job returns a [`Reply`], awaited on the UI's executor.
//! - **Coalesced writes.** [`DbWorker::write_coalesced`] replaces a pending job
//!   with the same key (last write wins), for settings, playlist persistence and
//!   EQ bands, which a slider can produce dozens of times a second.
//! - **The UI-thread guard.** Blocking waits on a reply panic, in debug builds,
//!   on the thread registered with
//!   [`mark_ui_thread`](super::threading::mark_ui_thread).

#![warn(missing_docs)]

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use super::error::FrontendError;
use super::reply::{Reply, ReplySender, reply_channel};
use super::threading::{JobQueue, WorkerThread};
use crate::db::Database;

/// The worker name used in [`FrontendError::WorkerStopped`].
pub const DB_WORKER: &str = "database";

/// How long dropping the last [`DbWorker`] handle waits for the thread.
const DROP_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// The shared connection type, as `AudioManager` and `Scanner` take it.
pub type SharedDatabase = Arc<std::sync::Mutex<Database>>;

type DbJobFn = Box<dyn FnOnce(&mut Database) + Send>;
type SupersedeFn = Box<dyn FnOnce() + Send>;

struct DbJob {
    key: Option<String>,
    run: DbJobFn,
    supersede: Option<SupersedeFn>,
}

struct DbShared {
    queue: JobQueue<DbJob>,
    connection: SharedDatabase,
}

struct DbWorkerInner {
    shared: Arc<DbShared>,
    thread: WorkerThread,
}

impl Drop for DbWorkerInner {
    fn drop(&mut self) {
        self.shared.queue.close();
        self.thread.join_within(DROP_JOIN_TIMEOUT);
    }
}

/// Handle to the database worker. Clones share the thread; the thread stops
/// when [`DbWorker::shutdown`] is called or the last clone is dropped, after
/// running the jobs already queued.
#[derive(Clone)]
pub struct DbWorker {
    inner: Arc<DbWorkerInner>,
}

impl std::fmt::Debug for DbWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbWorker")
            .field("pending_jobs", &self.pending_jobs())
            .field("stopped", &self.is_stopped())
            .finish()
    }
}

impl DbWorker {
    /// Starts the worker thread on `connection`.
    pub fn new(connection: SharedDatabase) -> Result<Self, FrontendError> {
        let shared = Arc::new(DbShared {
            queue: JobQueue::new(),
            connection,
        });
        let worker_shared = Arc::clone(&shared);
        let thread = WorkerThread::spawn("audoxidy-db", move || run_worker(&worker_shared))?;
        Ok(Self {
            inner: Arc::new(DbWorkerInner { shared, thread }),
        })
    }

    /// The shared connection, for the backend APIs that take it and lock it
    /// themselves: `AudioManager::set_database`, `Scanner::new`,
    /// `Scanner::import_m3u` and `Scanner::process_external_batch`.
    ///
    /// Call those from a long operation, never from the UI thread. Everything
    /// else goes through [`DbWorker::read`] and [`DbWorker::write`].
    pub fn connection(&self) -> SharedDatabase {
        Arc::clone(&self.inner.shared.connection)
    }

    /// Queues a read-only job. It runs after every job submitted before it.
    pub fn read<T, F>(&self, f: F) -> Reply<T>
    where
        T: Send + 'static,
        F: FnOnce(&Database) -> Result<T, FrontendError> + Send + 'static,
    {
        self.submit(None, move |db: &mut Database| f(db))
    }

    /// Queues a job that may modify the database. It runs after every job
    /// submitted before it.
    pub fn write<T, F>(&self, f: F) -> Reply<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, FrontendError> + Send + 'static,
    {
        self.submit(None, f)
    }

    /// Queues a write that replaces any pending, not yet started job with the
    /// same `key` (last write wins).
    ///
    /// The new job takes the replaced job's place in the queue, so a job
    /// submitted after the replaced one still sees a value at least as new as
    /// the replaced one. The replaced job never runs and its reply resolves to
    /// [`FrontendError::Superseded`]. A job that has already started is never
    /// replaced: the new one is queued after it.
    pub fn write_coalesced<T, F>(&self, key: impl Into<String>, f: F) -> Reply<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, FrontendError> + Send + 'static,
    {
        self.submit(Some(key.into()), f)
    }

    /// The number of jobs waiting to start.
    pub fn pending_jobs(&self) -> usize {
        self.inner.shared.queue.len()
    }

    /// Refuses new jobs, lets the worker finish the queued ones, and waits at
    /// most `timeout` for its thread. Returns `true` when the thread has
    /// stopped. Later calls only wait again.
    ///
    /// A job submitted afterwards resolves to
    /// [`FrontendError::WorkerStopped`].
    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shared.queue.close();
        self.inner.thread.join_within(timeout)
    }

    /// Whether the worker thread has been joined.
    pub fn is_stopped(&self) -> bool {
        self.inner.thread.is_joined()
    }

    /// Whether the worker refuses new jobs (shutdown has started).
    pub fn is_closed(&self) -> bool {
        self.inner.shared.queue.is_closed()
    }

    fn submit<T, F>(&self, key: Option<String>, f: F) -> Reply<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Database) -> Result<T, FrontendError> + Send + 'static,
    {
        let (tx, reply) = reply_channel(DB_WORKER);
        // A closed queue drops the job, and with it the sender: the reply then
        // resolves to `WorkerStopped`.
        let Some(key) = key else {
            let _ = self.inner.shared.queue.push(DbJob {
                key: None,
                run: wrap_job(f, tx),
                supersede: None,
            });
            return reply;
        };
        let superseded = tx.clone();
        let job = DbJob {
            key: Some(key),
            run: wrap_job(f, tx),
            supersede: Some(Box::new(move || {
                superseded.send(Err(FrontendError::Superseded));
            })),
        };
        let _ = self.inner.shared.queue.modify(move |jobs| {
            let pending = jobs
                .iter()
                .position(|j| j.key.is_some() && j.key == job.key);
            match pending {
                Some(index) => {
                    let replaced = std::mem::replace(&mut jobs[index], job);
                    if let Some(supersede) = replaced.supersede {
                        supersede();
                    }
                }
                None => jobs.push_back(job),
            }
        });
        reply
    }
}

/// Runs `f` and sends its result; a panic becomes
/// [`FrontendError::JobPanicked`] and the worker keeps running.
fn wrap_job<T, F>(f: F, tx: ReplySender<T>) -> DbJobFn
where
    T: Send + 'static,
    F: FnOnce(&mut Database) -> Result<T, FrontendError> + Send + 'static,
{
    Box::new(move |db: &mut Database| {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| f(db)))
            .unwrap_or_else(|_| Err(FrontendError::JobPanicked(DB_WORKER)));
        tx.send(result);
    })
}

fn run_worker(shared: &DbShared) {
    while let Some(job) = shared.queue.pop() {
        let mut db = match shared.connection.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Another holder of the shared connection (the scanner) panicked.
                // SQLite rolled back its open statement; the connection is usable.
                tracing::warn!("the database lock was poisoned by another thread; recovering");
                shared.connection.clear_poison();
                poisoned.into_inner()
            }
        };
        (job.run)(&mut db);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::reply::Latest;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::mpsc;

    fn memory_worker() -> DbWorker {
        let db = Database::new_memory().unwrap();
        DbWorker::new(Arc::new(std::sync::Mutex::new(db))).unwrap()
    }

    /// Blocks the worker until the returned sender is used (or dropped), and
    /// returns once the blocking job has started.
    fn block_worker(worker: &DbWorker) -> (mpsc::Sender<()>, Reply<()>) {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let blocker = worker.write(move |_| {
            let _ = started_tx.send(());
            let _ = release_rx.recv_timeout(TEST_TIMEOUT);
            Ok(())
        });
        started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
        (release_tx, blocker)
    }

    #[test]
    fn jobs_see_the_in_memory_schema() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let names = worker
                .read(|db| {
                    Ok(db
                        .get_all_playlists()?
                        .into_iter()
                        .map(|p| p.name)
                        .collect::<Vec<_>>())
                })
                .recv_blocking()
                .unwrap();
            assert!(names.contains(&"Audoxidy".to_string()));
            assert!(names.contains(&"Oxidy Drift".to_string()));
        });
    }

    #[test]
    fn writes_run_in_submission_order() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let replies: Vec<Reply<()>> = (0..50)
                .map(|i| {
                    worker.write(move |db| {
                        let log = db.get_setting("log").unwrap_or_default();
                        db.set_setting("log", &format!("{log}{i},"))?;
                        Ok(())
                    })
                })
                .collect();
            let log = worker
                .read(|db| Ok(db.get_setting("log")))
                .recv_blocking()
                .unwrap()
                .unwrap();
            let expected: String = (0..50).map(|i| format!("{i},")).collect();
            assert_eq!(log, expected, "the read ran after every earlier write");
            for reply in replies {
                reply.recv_blocking().unwrap();
            }
        });
    }

    #[test]
    fn a_coalesced_write_replaces_a_pending_job_with_the_same_key() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let (release, blocker) = block_worker(&worker);

            let first = worker.write_coalesced("player_volume", |db| {
                db.set_setting("player_volume", "0.1")?;
                Ok(1)
            });
            let other = worker.write_coalesced("eq_enabled", |db| {
                db.set_setting("eq_enabled", "1")?;
                Ok(())
            });
            let second = worker.write_coalesced("player_volume", |db| {
                db.set_setting("player_volume", "0.2")?;
                Ok(2)
            });
            assert_eq!(
                worker.pending_jobs(),
                2,
                "the second write took the first's place"
            );

            release.send(()).unwrap();
            blocker.recv_blocking().unwrap();
            assert!(matches!(
                first.recv_blocking(),
                Err(FrontendError::Superseded)
            ));
            assert_eq!(second.recv_blocking().unwrap(), 2);
            other.recv_blocking().unwrap();
            let volume = worker
                .read(|db| Ok(db.get_setting("player_volume")))
                .recv_blocking()
                .unwrap();
            assert_eq!(volume.as_deref(), Some("0.2"));
        });
    }

    #[test]
    fn a_coalesced_write_never_replaces_a_job_that_already_started() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let first = worker.write_coalesced("player_volume", move |db| {
                let _ = started_tx.send(());
                let _ = release_rx.recv_timeout(TEST_TIMEOUT);
                db.set_setting("player_volume", "0.1")?;
                Ok("first")
            });
            started_rx.recv_timeout(TEST_TIMEOUT).unwrap();

            let second = worker.write_coalesced("player_volume", |db| {
                db.set_setting("player_volume", "0.2")?;
                Ok("second")
            });
            assert_eq!(worker.pending_jobs(), 1);
            release_tx.send(()).unwrap();

            assert_eq!(first.recv_blocking().unwrap(), "first");
            assert_eq!(second.recv_blocking().unwrap(), "second");
            let volume = worker
                .read(|db| Ok(db.get_setting("player_volume")))
                .recv_blocking()
                .unwrap();
            assert_eq!(volume.as_deref(), Some("0.2"));
        });
    }

    #[test]
    fn latest_drops_a_stale_search_reply() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let latest = Latest::new();
            let (release, blocker) = block_worker(&worker);
            let older = latest.track(worker.read(|_| Ok("older search")));
            let newer = latest.track(worker.read(|_| Ok("newer search")));
            release.send(()).unwrap();
            blocker.recv_blocking().unwrap();

            assert_eq!(newer.recv_blocking().unwrap().unwrap(), "newer search");
            assert!(older.recv_blocking().is_none(), "the older reply is stale");
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "must not be called on the UI thread")]
    fn recv_blocking_on_the_marked_ui_thread_panics() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            crate::frontend::threading::mark_ui_thread();
            let _ = worker.read(|_| Ok(())).recv_blocking();
        });
    }

    #[test]
    fn a_panicking_job_reports_the_panic_and_the_worker_keeps_running() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let failed = worker.read::<(), _>(|_| panic!("job bug"));
            assert!(matches!(
                failed.recv_blocking(),
                Err(FrontendError::JobPanicked(DB_WORKER))
            ));
            assert_eq!(worker.read(|_| Ok(4)).recv_blocking().unwrap(), 4);
        });
    }

    #[test]
    fn shutdown_runs_the_queued_jobs_then_refuses_new_ones() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let (release, blocker) = block_worker(&worker);
            let queued = worker.write(|db| {
                db.set_setting("k", "v")?;
                Ok(())
            });
            let stopper = worker.clone();
            let shutdown = std::thread::spawn(move || stopper.shutdown(TEST_TIMEOUT));
            release.send(()).unwrap();
            assert!(shutdown.join().unwrap());
            blocker.recv_blocking().unwrap();
            queued.recv_blocking().unwrap();

            assert!(worker.is_stopped());
            assert!(worker.is_closed());
            assert!(matches!(
                worker.read(|_| Ok(())).recv_blocking(),
                Err(FrontendError::WorkerStopped(DB_WORKER))
            ));
            assert!(
                worker.shutdown(TEST_TIMEOUT),
                "a second shutdown is a no-op"
            );
        });
    }

    #[test]
    fn dropping_the_last_handle_stops_the_thread() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = memory_worker();
            let connection = worker.connection();
            assert_eq!(Arc::strong_count(&connection), 2);
            drop(worker);
            // The thread's share of the connection is gone once it has stopped.
            assert_eq!(Arc::strong_count(&connection), 1);
        });
    }
}
