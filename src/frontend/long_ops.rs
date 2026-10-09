//! Long operations: work that takes seconds, runs on its own thread, reports
//! progress, and can be cancelled.
//!
//! Scans, M3U imports, external-file batches, exports, library deletions and
//! the filter-index build take too long for a worker queue: they would hold up
//! every database job or stream call behind them. Each runs on a dedicated
//! thread instead, identified by an [`OpId`]. The UI follows it through
//! `LongOperation` events and collects its typed result with
//! [`LongOperations::take_result`].

#![warn(missing_docs)]

use std::any::Any;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::error::FrontendError;
use super::events::{Event, EventHub, LongOpEvent, LongOpInfo, LongOpKind, OpId};

/// The worker name used in [`FrontendError`] values from this module.
pub const LONG_OPS: &str = "long operations";

/// Where a long operation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpStatus {
    /// Its closure is still running.
    Running,
    /// It succeeded; the result waits for [`LongOperations::take_result`].
    Completed,
    /// It failed; the error waits for [`LongOperations::take_result`].
    Failed,
    /// It stopped after [`LongOperations::cancel`]; the
    /// [`FrontendError::Cancelled`] waits for [`LongOperations::take_result`].
    Cancelled,
}

/// What a running operation's closure can use: its identity, its cancellation
/// flag, and a way to report progress.
pub struct OpContext {
    op_id: OpId,
    kind: LongOpKind,
    cancel: Arc<AtomicBool>,
    hub: EventHub,
}

impl OpContext {
    /// The operation's id.
    pub fn op_id(&self) -> OpId {
        self.op_id
    }

    /// What the operation does.
    pub fn kind(&self) -> LongOpKind {
        self.kind
    }

    /// Whether [`LongOperations::cancel`] was called for this operation.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    /// `Err(FrontendError::Cancelled)` once the operation is cancelled, so a
    /// loop can stop with `ctx.check_cancelled()?`.
    pub fn check_cancelled(&self) -> Result<(), FrontendError> {
        if self.is_cancelled() {
            Err(FrontendError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Publishes a `Progress` event; `fraction` is clamped to 0.0..=1.0.
    pub fn report_progress(&self, fraction: f32) {
        let progress = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        self.hub
            .publish(Event::LongOperation(LongOpEvent::Progress(LongOpInfo {
                op_id: self.op_id,
                kind: self.kind,
                progress: Some(progress),
                error: None,
                cancelled: false,
            })));
    }
}

struct OpEntry {
    kind: LongOpKind,
    status: OpStatus,
    cancel: Arc<AtomicBool>,
    result: Option<Box<dyn Any + Send>>,
    thread: Option<JoinHandle<()>>,
}

struct OpsShared {
    hub: EventHub,
    next_id: AtomicU64,
    closed: AtomicBool,
    ops: Mutex<HashMap<OpId, OpEntry>>,
    /// Threads of operations whose result was taken, kept so that shutdown
    /// can join them.
    retired: Mutex<Vec<JoinHandle<()>>>,
}

impl OpsShared {
    /// Stores the outcome, then publishes it: a subscriber can take the
    /// result from its callback.
    fn finish<T: Send + 'static>(
        &self,
        op_id: OpId,
        kind: LongOpKind,
        outcome: Result<T, FrontendError>,
    ) {
        let (status, event) = match &outcome {
            Ok(_) => (
                OpStatus::Completed,
                LongOpEvent::Completed(LongOpInfo {
                    op_id,
                    kind,
                    progress: Some(1.0),
                    error: None,
                    cancelled: false,
                }),
            ),
            Err(error) => {
                let cancelled = matches!(error, FrontendError::Cancelled);
                (
                    if cancelled {
                        OpStatus::Cancelled
                    } else {
                        OpStatus::Failed
                    },
                    LongOpEvent::Failed(LongOpInfo {
                        op_id,
                        kind,
                        progress: None,
                        error: Some(error.to_string()),
                        cancelled,
                    }),
                )
            }
        };
        if let Some(entry) = self.ops.lock().get_mut(&op_id) {
            entry.status = status;
            entry.result = Some(Box::new(outcome));
        }
        self.hub.publish(Event::LongOperation(event));
    }
}

/// Starts and tracks long operations. Clones share the registry.
#[derive(Clone)]
pub struct LongOperations {
    shared: Arc<OpsShared>,
}

impl std::fmt::Debug for LongOperations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LongOperations")
            .field("operations", &self.shared.ops.lock().len())
            .finish()
    }
}

impl LongOperations {
    /// A registry that publishes on `hub`.
    pub fn new(hub: EventHub) -> Self {
        Self {
            shared: Arc::new(OpsShared {
                hub,
                next_id: AtomicU64::new(1),
                closed: AtomicBool::new(false),
                ops: Mutex::new(HashMap::new()),
                retired: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Runs `f` on a new thread and returns the operation's id at once.
    ///
    /// When `f` returns, its result is stored for
    /// [`LongOperations::take_result`] and a `Completed` or `Failed` event is
    /// published (`Failed` with `cancelled: true` when `f` returned
    /// [`FrontendError::Cancelled`]). A panic in `f` becomes a failure. After
    /// [`LongOperations::shutdown`], the operation fails at once with
    /// [`FrontendError::WorkerStopped`].
    pub fn start<T, F>(&self, kind: LongOpKind, f: F) -> OpId
    where
        T: Send + 'static,
        F: FnOnce(&OpContext) -> Result<T, FrontendError> + Send + 'static,
    {
        self.join_retired_threads();
        let op_id = OpId(self.shared.next_id.fetch_add(1, Ordering::Relaxed));
        let cancel = Arc::new(AtomicBool::new(false));
        self.shared.ops.lock().insert(
            op_id,
            OpEntry {
                kind,
                status: OpStatus::Running,
                cancel: Arc::clone(&cancel),
                result: None,
                thread: None,
            },
        );
        if self.shared.closed.load(Ordering::Acquire) {
            self.shared
                .finish::<T>(op_id, kind, Err(FrontendError::WorkerStopped(LONG_OPS)));
            return op_id;
        }

        let shared = Arc::clone(&self.shared);
        let context = OpContext {
            op_id,
            kind,
            cancel,
            hub: self.shared.hub.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name(format!("audoxidy-op-{}", op_id.0))
            .spawn(move || {
                let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| f(&context)))
                    .unwrap_or_else(|_| Err(FrontendError::JobPanicked(LONG_OPS)));
                shared.finish(op_id, kind, outcome);
            });
        match spawned {
            Ok(handle) => {
                if let Some(entry) = self.shared.ops.lock().get_mut(&op_id) {
                    entry.thread = Some(handle);
                } else {
                    self.shared.retired.lock().push(handle);
                }
            }
            Err(e) => self.shared.finish::<T>(op_id, kind, Err(e.into())),
        }
        op_id
    }

    /// Asks the operation to stop. Its closure sees the request through
    /// [`OpContext::is_cancelled`]; it is not interrupted. Returns `true` when
    /// the operation was still running.
    pub fn cancel(&self, op_id: OpId) -> bool {
        match self.shared.ops.lock().get(&op_id) {
            Some(entry) if entry.status == OpStatus::Running => {
                entry.cancel.store(true, Ordering::Release);
                true
            }
            _ => false,
        }
    }

    /// Where the operation stands; `None` for an unknown id or once its result
    /// was taken.
    pub fn status(&self, op_id: OpId) -> Option<OpStatus> {
        self.shared.ops.lock().get(&op_id).map(|entry| entry.status)
    }

    /// What the operation does; `None` for an unknown id or once its result
    /// was taken.
    pub fn kind(&self, op_id: OpId) -> Option<LongOpKind> {
        self.shared.ops.lock().get(&op_id).map(|entry| entry.kind)
    }

    /// The operations still running.
    pub fn running(&self) -> Vec<OpId> {
        let mut ids: Vec<OpId> = self
            .shared
            .ops
            .lock()
            .iter()
            .filter(|(_, entry)| entry.status == OpStatus::Running)
            .map(|(id, _)| *id)
            .collect();
        ids.sort();
        ids
    }

    /// Takes the finished operation's result and forgets the operation.
    ///
    /// `None` while it runs, for an unknown id, or once the result was taken.
    /// If `T` is not the type the closure returned, the result stays stored
    /// and this returns [`FrontendError::InvalidInput`].
    pub fn take_result<T: Send + 'static>(&self, op_id: OpId) -> Option<Result<T, FrontendError>> {
        let mut ops = self.shared.ops.lock();
        let entry = ops.get_mut(&op_id)?;
        if entry.status == OpStatus::Running {
            return None;
        }
        let stored = entry.result.take()?;
        match stored.downcast::<Result<T, FrontendError>>() {
            Ok(result) => {
                if let Some(handle) = ops.remove(&op_id).and_then(|entry| entry.thread) {
                    self.shared.retired.lock().push(handle);
                }
                Some(*result)
            }
            Err(stored) => {
                entry.result = Some(stored);
                Some(Err(FrontendError::InvalidInput(format!(
                    "the result of operation {} is not a {}",
                    op_id.0,
                    std::any::type_name::<T>()
                ))))
            }
        }
    }

    /// Refuses new operations, cancels the running ones, and waits at most
    /// `timeout` for every operation thread to end. Returns `true` when every
    /// thread was joined; an operation that ignores cancellation is left
    /// running after the timeout. Later calls only wait again.
    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.shared.closed.store(true, Ordering::Release);
        let mut handles: Vec<JoinHandle<()>> = {
            let mut ops = self.shared.ops.lock();
            ops.values_mut()
                .filter_map(|entry| {
                    entry.cancel.store(true, Ordering::Release);
                    entry.thread.take()
                })
                .collect()
        };
        handles.append(&mut self.shared.retired.lock());

        let deadline = Instant::now() + timeout;
        let mut all_joined = true;
        for handle in handles {
            if handle.thread().id() == std::thread::current().id() {
                all_joined = false;
                continue;
            }
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                tracing::warn!("a long operation ignored cancellation; it is left running");
                all_joined = false;
                self.shared.retired.lock().push(handle);
            }
        }
        all_joined
    }

    /// Joins the threads of retired operations that have already ended.
    fn join_retired_threads(&self) {
        let mut retired = self.shared.retired.lock();
        let (finished, running): (Vec<_>, Vec<_>) =
            retired.drain(..).partition(JoinHandle::is_finished);
        *retired = running;
        drop(retired);
        for handle in finished {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::events::Origin;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::mpsc;

    struct Rig {
        hub: EventHub,
        ops: LongOperations,
        events: mpsc::Receiver<LongOpEvent>,
        _token: crate::frontend::events::SubscriptionToken,
    }

    fn rig() -> Rig {
        let hub = EventHub::new().unwrap();
        let (tx, events) = mpsc::channel();
        let tx = Mutex::new(tx);
        let token = hub.subscribe(
            Origin::LongOperation,
            Arc::new(move |event| {
                if let Event::LongOperation(event) = event {
                    let _ = tx.lock().send(event);
                }
            }),
        );
        Rig {
            ops: LongOperations::new(hub.clone()),
            hub,
            events,
            _token: token,
        }
    }

    /// Collects events until the operation's outcome event.
    fn events_until_outcome(rig: &Rig) -> Vec<LongOpEvent> {
        let mut events = Vec::new();
        loop {
            let event = rig.events.recv_timeout(TEST_TIMEOUT).unwrap();
            let done = !matches!(event, LongOpEvent::Progress(_));
            events.push(event);
            if done {
                return events;
            }
        }
    }

    #[test]
    fn a_completed_operation_reports_progress_and_its_typed_result() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let op = rig.ops.start(LongOpKind::Scan, |ctx| {
                ctx.report_progress(0.25);
                ctx.report_progress(7.0);
                Ok(42_u32)
            });
            let events = events_until_outcome(&rig);
            let progress: Vec<Option<f32>> = events
                .iter()
                .filter_map(|event| match event {
                    LongOpEvent::Progress(info) => Some(info.progress),
                    _ => None,
                })
                .collect();
            assert_eq!(progress, vec![Some(0.25), Some(1.0)]);
            assert!(matches!(
                events.last(),
                Some(LongOpEvent::Completed(info)) if info.op_id == op && info.kind == LongOpKind::Scan
            ));
            assert_eq!(rig.ops.status(op), Some(OpStatus::Completed));
            assert_eq!(rig.ops.take_result::<u32>(op).unwrap().unwrap(), 42);
            assert!(rig.ops.take_result::<u32>(op).is_none(), "taken once");
            assert_eq!(rig.ops.status(op), None);
            assert!(rig.ops.shutdown(TEST_TIMEOUT));
            assert!(rig.hub.shutdown(TEST_TIMEOUT));
        });
    }

    #[test]
    fn a_failed_operation_reports_its_error() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let op = rig.ops.start::<u32, _>(LongOpKind::ExportPlaylist, |_| {
                Err(FrontendError::InvalidInput("no such folder".into()))
            });
            let events = events_until_outcome(&rig);
            match events.last() {
                Some(LongOpEvent::Failed(info)) => {
                    assert_eq!(info.op_id, op);
                    assert!(!info.cancelled);
                    assert_eq!(info.error.as_deref(), Some("invalid input: no such folder"));
                }
                other => panic!("expected a failure, got {other:?}"),
            }
            assert_eq!(rig.ops.status(op), Some(OpStatus::Failed));
            assert!(matches!(
                rig.ops.take_result::<u32>(op),
                Some(Err(FrontendError::InvalidInput(_)))
            ));
        });
    }

    #[test]
    fn a_cancelled_operation_stops_and_reports_the_cancellation() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let (started_tx, started_rx) = mpsc::channel();
            let op = rig
                .ops
                .start::<(), _>(LongOpKind::ImportPlaylist, move |ctx| {
                    let _ = started_tx.send(());
                    loop {
                        ctx.check_cancelled()?;
                        ctx.report_progress(0.5);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                });
            started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
            assert_eq!(rig.ops.running(), vec![op]);
            assert!(rig.ops.cancel(op));

            let events = events_until_outcome(&rig);
            assert!(matches!(
                events.last(),
                Some(LongOpEvent::Failed(info)) if info.cancelled && info.op_id == op
            ));
            assert_eq!(rig.ops.status(op), Some(OpStatus::Cancelled));
            assert!(
                !rig.ops.cancel(op),
                "a finished operation cannot be cancelled"
            );
            assert!(matches!(
                rig.ops.take_result::<()>(op),
                Some(Err(FrontendError::Cancelled))
            ));
        });
    }

    #[test]
    fn taking_a_result_with_the_wrong_type_keeps_it() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let op = rig
                .ops
                .start(LongOpKind::FilterIndexBuild, |_| Ok("index".to_string()));
            events_until_outcome(&rig);
            assert!(matches!(
                rig.ops.take_result::<u32>(op),
                Some(Err(FrontendError::InvalidInput(_)))
            ));
            assert_eq!(rig.ops.take_result::<String>(op).unwrap().unwrap(), "index");
        });
    }

    #[test]
    fn a_panicking_operation_fails() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let op = rig
                .ops
                .start::<(), _>(LongOpKind::LibraryDelete, |_| panic!("op bug"));
            let events = events_until_outcome(&rig);
            assert!(matches!(events.last(), Some(LongOpEvent::Failed(_))));
            assert!(matches!(
                rig.ops.take_result::<()>(op),
                Some(Err(FrontendError::JobPanicked(LONG_OPS)))
            ));
        });
    }

    #[test]
    fn shutdown_cancels_running_operations_and_refuses_new_ones() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = rig();
            let (started_tx, started_rx) = mpsc::channel();
            let op = rig.ops.start::<(), _>(LongOpKind::Scan, move |ctx| {
                let _ = started_tx.send(());
                while !ctx.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(FrontendError::Cancelled)
            });
            started_rx.recv_timeout(TEST_TIMEOUT).unwrap();
            assert!(rig.ops.shutdown(TEST_TIMEOUT));
            assert_eq!(rig.ops.status(op), Some(OpStatus::Cancelled));

            let late = rig.ops.start(LongOpKind::Scan, |_| Ok(()));
            assert!(matches!(
                rig.ops.take_result::<()>(late),
                Some(Err(FrontendError::WorkerStopped(LONG_OPS)))
            ));
        });
    }
}
