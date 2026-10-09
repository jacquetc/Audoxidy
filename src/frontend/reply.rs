//! One-shot replies from the facade's workers, and stale-reply detection.
//!
//! Every worker request returns a [`Reply`]: a one-shot channel that a UI
//! awaits on its own executor (`async fn recv`, which needs no reactor; the
//! worker's send wakes it), and that tests and other workers can wait on
//! synchronously. Blocking waits assert, in debug builds, that they do not run
//! on the UI thread (see [`mark_ui_thread`](super::threading::mark_ui_thread)).
//!
//! [`Latest`] solves the "older result arrives last" race: a view-model that
//! fires a new library search before the previous one answered tracks both
//! replies, and only the newest is delivered. Iced has this race today
//! (`src/gui/app.rs:6863`).

#![warn(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::error::FrontendError;
use super::threading::debug_assert_not_ui_thread;

/// The sending half of a [`Reply`], held by the worker that runs the job.
pub(crate) struct ReplySender<T> {
    tx: async_channel::Sender<Result<T, FrontendError>>,
}

impl<T> Clone for ReplySender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<T> ReplySender<T> {
    /// Delivers the result. A receiver that was dropped is ignored, and so is
    /// a second result (the reply already holds one).
    pub(crate) fn send(self, result: Result<T, FrontendError>) {
        let _ = self.tx.try_send(result);
    }
}

/// Creates a connected sender and [`Reply`] for a job on `worker`.
pub(crate) fn reply_channel<T>(worker: &'static str) -> (ReplySender<T>, Reply<T>) {
    let (tx, rx) = async_channel::bounded(1);
    (ReplySender { tx }, Reply { rx, worker })
}

/// The pending result of a job submitted to a worker.
///
/// A reply yields its value once. If the job can no longer run (the worker
/// stopped, or the job was dropped), the reply resolves to
/// [`FrontendError::WorkerStopped`] instead of hanging.
#[must_use = "a reply does nothing unless it is received or dropped on purpose"]
pub struct Reply<T> {
    rx: async_channel::Receiver<Result<T, FrontendError>>,
    worker: &'static str,
}

impl<T> std::fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reply")
            .field("worker", &self.worker)
            .field("ready", &!self.rx.is_empty())
            .finish()
    }
}

impl<T> Reply<T> {
    /// A reply that is already resolved, for a request that can answer
    /// without running a job.
    pub fn ready(result: Result<T, FrontendError>) -> Self {
        let (tx, reply) = reply_channel("immediate");
        tx.send(result);
        reply
    }

    /// The name of the worker that answers this reply.
    pub fn worker(&self) -> &'static str {
        self.worker
    }

    /// Waits for the result without blocking a thread. This is what a UI
    /// awaits on its own executor; the worker's send wakes the task.
    pub async fn recv(self) -> Result<T, FrontendError> {
        match self.rx.recv().await {
            Ok(result) => result,
            Err(_) => Err(FrontendError::WorkerStopped(self.worker)),
        }
    }

    /// Blocks the calling thread until the result arrives.
    ///
    /// For tests and worker threads only: in debug builds this panics on the
    /// thread registered with
    /// [`mark_ui_thread`](super::threading::mark_ui_thread).
    pub fn recv_blocking(self) -> Result<T, FrontendError> {
        debug_assert_not_ui_thread("Reply::recv_blocking");
        match self.rx.recv_blocking() {
            Ok(result) => result,
            Err(_) => Err(FrontendError::WorkerStopped(self.worker)),
        }
    }

    /// Blocks for at most `timeout`; `None` if the result has not arrived.
    ///
    /// The reply stays usable after a timeout. Like
    /// [`Reply::recv_blocking`], it panics on the UI thread in debug builds.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Result<T, FrontendError>> {
        debug_assert_not_ui_thread("Reply::recv_timeout");
        let deadline = Instant::now() + timeout;
        let mut pause = Duration::from_micros(50);
        loop {
            if let Some(result) = self.try_recv() {
                return Some(result);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            std::thread::sleep(pause.min(deadline - now));
            pause = (pause * 2).min(Duration::from_millis(5));
        }
    }

    /// Takes the result if it has arrived, without waiting. Allowed on the UI
    /// thread.
    pub fn try_recv(&self) -> Option<Result<T, FrontendError>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(async_channel::TryRecvError::Empty) => None,
            Err(async_channel::TryRecvError::Closed) => {
                Some(Err(FrontendError::WorkerStopped(self.worker)))
            }
        }
    }
}

/// A generation counter that tells a stale reply from the latest one.
///
/// Each request takes a [`Ticket`] with [`Latest::issue`] (or is wrapped with
/// [`Latest::track`]); a later `issue` makes every earlier ticket stale. Clones
/// share the counter.
#[derive(Debug, Clone, Default)]
pub struct Latest {
    generation: Arc<AtomicU64>,
}

/// The generation a request was issued with; see [`Latest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

impl Latest {
    /// A counter with no request issued yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new generation and returns its ticket. Every earlier ticket
    /// becomes stale.
    pub fn issue(&self) -> Ticket {
        Ticket(self.generation.fetch_add(1, Ordering::AcqRel) + 1)
    }

    /// Whether `ticket` is still the newest one.
    pub fn is_current(&self, ticket: Ticket) -> bool {
        self.generation.load(Ordering::Acquire) == ticket.0
    }

    /// Makes every ticket issued so far stale, without issuing a new one (for
    /// example when the view that asked is closed).
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Issues a ticket for `reply`; the result is delivered only if no newer
    /// ticket has been issued by the time it arrives.
    pub fn track<T>(&self, reply: Reply<T>) -> TrackedReply<T> {
        TrackedReply {
            ticket: self.issue(),
            latest: self.clone(),
            reply,
        }
    }
}

/// A [`Reply`] that is dropped if it is stale when it arrives; see
/// [`Latest::track`].
#[must_use = "a tracked reply does nothing unless it is received"]
#[derive(Debug)]
pub struct TrackedReply<T> {
    reply: Reply<T>,
    latest: Latest,
    ticket: Ticket,
}

impl<T> TrackedReply<T> {
    /// The ticket this reply was issued with.
    pub fn ticket(&self) -> Ticket {
        self.ticket
    }

    /// Waits for the result; `None` if a newer request was issued meanwhile.
    pub async fn recv(self) -> Option<Result<T, FrontendError>> {
        let result = self.reply.recv().await;
        self.latest.is_current(self.ticket).then_some(result)
    }

    /// Blocking form of [`TrackedReply::recv`]; panics on the UI thread in
    /// debug builds.
    pub fn recv_blocking(self) -> Option<Result<T, FrontendError>> {
        let result = self.reply.recv_blocking();
        self.latest.is_current(self.ticket).then_some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::threading::mark_ui_thread;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, block_on, with_timeout};

    #[test]
    fn a_reply_future_completes_on_an_executor_without_a_reactor() {
        with_timeout(TEST_TIMEOUT, || {
            let (tx, reply) = reply_channel::<u32>("test");
            let sender = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                tx.send(Ok(7));
            });
            assert_eq!(block_on(reply.recv()).unwrap(), 7);
            sender.join().unwrap();
        });
    }

    #[test]
    fn a_dropped_sender_resolves_to_worker_stopped() {
        let (tx, reply) = reply_channel::<u32>("test");
        drop(tx);
        assert!(matches!(
            reply.try_recv(),
            Some(Err(FrontendError::WorkerStopped("test")))
        ));
    }

    #[test]
    fn recv_timeout_returns_none_and_the_reply_stays_usable() {
        with_timeout(TEST_TIMEOUT, || {
            let (tx, reply) = reply_channel::<u32>("test");
            assert!(reply.recv_timeout(Duration::from_millis(10)).is_none());
            tx.send(Ok(3));
            assert_eq!(reply.recv_timeout(TEST_TIMEOUT).unwrap().unwrap(), 3);
        });
    }

    #[test]
    fn try_recv_and_async_recv_are_allowed_on_the_ui_thread() {
        with_timeout(TEST_TIMEOUT, || {
            mark_ui_thread();
            let reply = Reply::ready(Ok(5));
            assert!(reply.try_recv().is_some());
            assert_eq!(block_on(Reply::ready(Ok(6)).recv()).unwrap(), 6);
        });
    }

    #[test]
    fn only_the_latest_tracked_reply_is_delivered() {
        let latest = Latest::new();
        let older = latest.track(Reply::ready(Ok("older")));
        let newer = latest.track(Reply::ready(Ok("newer")));
        assert_ne!(older.ticket(), newer.ticket());
        assert!(older.recv_blocking().is_none());
        assert_eq!(newer.recv_blocking().unwrap().unwrap(), "newer");
    }

    #[test]
    fn invalidate_makes_every_issued_ticket_stale() {
        let latest = Latest::new();
        let ticket = latest.issue();
        assert!(latest.is_current(ticket));
        latest.invalidate();
        assert!(!latest.is_current(ticket));
    }
}
