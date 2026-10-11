//! Threading primitives shared by the facade's workers.
//!
//! The facade owns several background threads (the database worker, the
//! engine worker, the event dispatcher, the playback monitor, the long
//! operations). They share three needs, met here once:
//!
//! - **The UI-thread guard.** A UI calls [`mark_ui_thread`] on its event-loop
//!   thread. From then on, in debug builds, every blocking wait of the facade
//!   (for example [`Reply::recv_blocking`](super::reply::Reply::recv_blocking))
//!   panics when it runs on that thread. This turns "no database or stream call
//!   blocks the UI" (quirk fix 5) into something a test can check.
//! - **An ordered job queue** that a worker drains in FIFO order, and that
//!   lets the submitter coalesce a job that has not started yet.
//! - **A joinable worker thread** whose join is bounded, so that shutting the
//!   facade down (or dropping it) never hangs on a stuck thread.

#![warn(missing_docs)]

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use super::error::FrontendError;

thread_local! {
    static IS_UI_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// Registers the calling thread as the UI thread.
///
/// The UI calls this once, on its event-loop thread, before it starts using
/// the facade. In debug builds every blocking wait of the facade then panics
/// if it is called on this thread; release builds do not check. Submitting work
/// (for example [`DbWorker::read`](super::db_worker::DbWorker::read)) and
/// awaiting a reply with `async fn recv` are always allowed on the UI thread.
pub fn mark_ui_thread() {
    IS_UI_THREAD.with(|flag| flag.set(true));
}

/// Whether the calling thread was registered with [`mark_ui_thread`].
pub fn is_ui_thread() -> bool {
    IS_UI_THREAD.with(Cell::get)
}

/// Panics in debug builds when a blocking wait runs on the UI thread.
///
/// `what` names the blocking call in the panic message.
pub(crate) fn debug_assert_not_ui_thread(what: &str) {
    debug_assert!(
        !is_ui_thread(),
        "{what} blocks and must not be called on the UI thread"
    );
}

/// A FIFO job queue drained by one worker thread.
///
/// Submitters push at the back; the worker pops from the front. While a job is
/// still queued (not started), a submitter can rewrite the pending jobs under
/// the queue lock with [`JobQueue::modify`], which is how coalescing works:
/// once the worker has popped a job, it is no longer visible to `modify`, so a
/// started job is never replaced.
pub(crate) struct JobQueue<J> {
    state: Mutex<QueueState<J>>,
    ready: Condvar,
}

struct QueueState<J> {
    jobs: VecDeque<J>,
    closed: bool,
}

impl<J> JobQueue<J> {
    /// An empty, open queue.
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                jobs: VecDeque::new(),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    /// Runs `f` on the pending jobs under the queue lock and wakes the worker.
    ///
    /// Returns `None`, without calling `f`, once the queue is closed.
    pub(crate) fn modify<R>(&self, f: impl FnOnce(&mut VecDeque<J>) -> R) -> Option<R> {
        let mut state = self.state.lock();
        if state.closed {
            return None;
        }
        let result = f(&mut state.jobs);
        drop(state);
        self.ready.notify_one();
        Some(result)
    }

    /// Appends a job; gives it back when the queue is closed.
    pub(crate) fn push(&self, job: J) -> Result<(), J> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(job);
        }
        state.jobs.push_back(job);
        drop(state);
        self.ready.notify_one();
        Ok(())
    }

    /// Takes the next job, blocking while the queue is empty.
    ///
    /// Returns `None` once the queue is closed and every pending job has been
    /// taken: closing drains the queue, it does not discard it.
    pub(crate) fn pop(&self) -> Option<J> {
        let mut state = self.state.lock();
        loop {
            if let Some(job) = state.jobs.pop_front() {
                return Some(job);
            }
            if state.closed {
                return None;
            }
            self.ready.wait(&mut state);
        }
    }

    /// Refuses new jobs and lets the worker finish the pending ones.
    pub(crate) fn close(&self) {
        self.state.lock().closed = true;
        self.ready.notify_all();
    }

    /// Whether [`JobQueue::close`] was called.
    pub(crate) fn is_closed(&self) -> bool {
        self.state.lock().closed
    }

    /// The number of jobs waiting to start.
    pub(crate) fn len(&self) -> usize {
        self.state.lock().jobs.len()
    }
}

/// A named background thread whose join is bounded.
pub(crate) struct WorkerThread {
    name: &'static str,
    handle: Mutex<Option<JoinHandle<()>>>,
    joined: AtomicBool,
}

impl WorkerThread {
    /// Spawns `body` on a new thread called `name`.
    pub(crate) fn spawn(
        name: &'static str,
        body: impl FnOnce() + Send + 'static,
    ) -> Result<Self, FrontendError> {
        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(body)?;
        Ok(Self {
            name,
            handle: Mutex::new(Some(handle)),
            joined: AtomicBool::new(false),
        })
    }

    /// Joins the thread, waiting at most `timeout` for it to finish.
    ///
    /// Returns `true` when the thread has finished (now or on an earlier call).
    /// On timeout the thread keeps running and `false` is returned; a later
    /// call can try again. A thread never joins itself: called from the worker
    /// thread, this returns `false` at once.
    pub(crate) fn join_within(&self, timeout: Duration) -> bool {
        let mut slot = self.handle.lock();
        let Some(handle) = slot.take() else {
            return self.joined.load(Ordering::Acquire);
        };
        if handle.thread().id() == std::thread::current().id() {
            *slot = Some(handle);
            return false;
        }
        let deadline = Instant::now() + timeout;
        while !handle.is_finished() {
            if Instant::now() >= deadline {
                tracing::warn!(
                    thread = self.name,
                    "the thread did not stop within {timeout:?}; it is left running"
                );
                *slot = Some(handle);
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if handle.join().is_err() {
            tracing::error!(thread = self.name, "the thread panicked");
        }
        self.joined.store(true, Ordering::Release);
        true
    }

    /// Whether the thread has been joined.
    pub(crate) fn is_joined(&self) -> bool {
        self.joined.load(Ordering::Acquire)
    }
}

/// Test helpers: bounded waits, so that no thread-based test can hang.
#[cfg(test)]
pub(crate) mod test_support {
    use std::future::Future;
    use std::panic::AssertUnwindSafe;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use std::time::Duration;

    /// The default bound for a thread-based test.
    ///
    /// It catches a deadlock, not a slow machine: a test waits for this long
    /// only when something is stuck. 60 s rather than 10 s, because the whole
    /// suite runs many contexts' threads at once and, on a loaded machine
    /// (parallel builds), a correct 2 s test could take more than 10 s.
    pub(crate) const TEST_TIMEOUT: Duration = Duration::from_secs(60);

    /// Runs `f` on a fresh thread and returns its value, or panics with
    /// "timed out" after `timeout`. A panic inside `f` is re-raised here, so
    /// `#[should_panic(expected = ...)]` sees the original message.
    pub(crate) fn with_timeout<T: Send + 'static>(
        timeout: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(f));
            let _ = tx.send(outcome);
        });
        match rx.recv_timeout(timeout) {
            Ok(Ok(value)) => {
                let _ = handle.join();
                value
            }
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
            Err(_) => panic!("test timed out after {timeout:?}"),
        }
    }

    struct ThreadWaker(std::thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    /// A minimal executor with no reactor: it parks the thread until the
    /// future's waker is called. A UI executor that only wakes on channel
    /// replies works the same way.
    pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::park();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{TEST_TIMEOUT, with_timeout};
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_thread_is_not_the_ui_thread_until_it_is_marked() {
        with_timeout(TEST_TIMEOUT, || {
            assert!(!is_ui_thread());
            mark_ui_thread();
            assert!(is_ui_thread());
            let other = std::thread::spawn(is_ui_thread).join().unwrap();
            assert!(!other, "the mark is per thread");
        });
    }

    #[test]
    fn the_queue_drains_pending_jobs_in_order_after_it_is_closed() {
        let queue = JobQueue::new();
        queue.push(1).unwrap();
        queue.push(2).unwrap();
        queue.close();
        assert_eq!(queue.push(3), Err(3));
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn a_worker_thread_joins_within_the_timeout_and_reports_a_stuck_one() {
        with_timeout(TEST_TIMEOUT, || {
            let quick = WorkerThread::spawn("test-quick", || {}).unwrap();
            assert!(quick.join_within(Duration::from_secs(5)));
            assert!(quick.is_joined());
            assert!(
                quick.join_within(Duration::ZERO),
                "a second join is a no-op"
            );

            let release = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&release);
            let stuck = WorkerThread::spawn("test-stuck", move || {
                while !flag.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
            .unwrap();
            assert!(!stuck.join_within(Duration::from_millis(20)));
            release.store(true, Ordering::Release);
            assert!(stuck.join_within(Duration::from_secs(5)));
        });
    }
}
