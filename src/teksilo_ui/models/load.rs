//! Awaiting the facade's replies on the main-thread executor.
//!
//! Every facade command returns a [`Reply`]: a one-shot future that the
//! worker's answer wakes. A model awaits it in a task spawned on the
//! executor `install_async` registers, and applies the answer when the task
//! resumes, on the UI thread, so the model's signals and lists are only ever
//! touched there. The task never blocks: `recv_blocking` panics on the UI
//! thread in debug builds (quirk fix 5).
//!
//! A failed reply is logged and reported to the caller as `None`; a
//! [`TrackedReply`] that a newer request made stale is dropped silently, since
//! the newer request is the one that will answer.

use std::future::Future;

use teksilo::prelude::{AsyncRuntimeHandle, BuildContext};

use crate::frontend::{FrontendError, Reply, TrackedReply};

/// Logs a failed load. A stopped worker is the normal end of the process, so
/// it is not a warning.
fn log_failure(what: &str, error: &FrontendError) {
    if matches!(error, FrontendError::WorkerStopped(_)) {
        tracing::debug!("{what} was not loaded: {error}");
    } else {
        tracing::warn!("{what} could not be loaded: {error}");
    }
}

/// Awaits `reply` on `runtime` and calls `finish` with the answer
/// (`Some(value)`), or with `None` when the load failed.
pub fn finish_reply<T: 'static>(
    runtime: &AsyncRuntimeHandle,
    what: &'static str,
    reply: Reply<T>,
    finish: impl FnOnce(Option<T>) + 'static,
) {
    spawn(runtime, async move {
        match reply.recv().await {
            Ok(value) => finish(Some(value)),
            Err(error) => {
                log_failure(what, &error);
                finish(None);
            }
        }
    });
}

/// Awaits `reply` on `runtime` and calls `finish` with the answer
/// (`Some(value)`), or with `None` when the load failed. `finish` is not
/// called when a newer request made the reply stale.
pub fn finish_tracked<T: 'static>(
    runtime: &AsyncRuntimeHandle,
    what: &'static str,
    reply: TrackedReply<T>,
    finish: impl FnOnce(Option<T>) + 'static,
) {
    spawn(runtime, async move {
        match reply.recv().await {
            Some(Ok(value)) => finish(Some(value)),
            Some(Err(error)) => {
                log_failure(what, &error);
                finish(None);
            }
            None => {}
        }
    });
}

/// Runs `future` on the executor, detached.
fn spawn(runtime: &AsyncRuntimeHandle, future: impl Future<Output = ()> + 'static) {
    runtime.spawn_local(future).detach();
}

/// Lets a worker thread's answer wake the event loop.
///
/// The reply wakes the executor from the worker's thread; with the poster set,
/// that wake also wakes the event loop, so an answer that lands while the
/// window is idle is still applied. Setting it again is harmless.
pub fn prepare_runtime(ctx: &BuildContext, runtime: &AsyncRuntimeHandle) {
    if let Some(poster) = ctx.poster() {
        runtime.set_poster(poster.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::frontend::Latest;
    use crate::teksilo_ui::models::test_env::TIMEOUT;
    use crate::teksilo_ui::test_support::run_until;

    #[test]
    fn a_reply_is_applied_on_the_executor() {
        let runtime = AsyncRuntimeHandle::new();
        let got = Rc::new(RefCell::new(None));
        let sink = got.clone();
        finish_reply(&runtime, "a number", Reply::ready(Ok(7)), move |value| {
            *sink.borrow_mut() = Some(value);
        });
        assert!(
            got.borrow().is_none(),
            "nothing runs before the executor ticks"
        );
        assert!(run_until(&runtime, TIMEOUT, || got.borrow().is_some()));
        assert_eq!(*got.borrow(), Some(Some(7)));
    }

    #[test]
    fn a_failed_reply_finishes_with_none() {
        let runtime = AsyncRuntimeHandle::new();
        let got = Rc::new(RefCell::new(None));
        let sink = got.clone();
        let failed = Reply::<u32>::ready(Err(FrontendError::InvalidInput("no".into())));
        finish_reply(&runtime, "a number", failed, move |value| {
            *sink.borrow_mut() = Some(value);
        });
        assert!(run_until(&runtime, TIMEOUT, || got.borrow().is_some()));
        assert_eq!(*got.borrow(), Some(None));
    }

    #[test]
    fn a_stale_tracked_reply_is_dropped() {
        let runtime = AsyncRuntimeHandle::new();
        let latest = Latest::new();
        let older = latest.track(Reply::ready(Ok("older")));
        let newer = latest.track(Reply::ready(Ok("newer")));
        let got = Rc::new(RefCell::new(Vec::new()));
        for reply in [older, newer] {
            let sink = got.clone();
            finish_tracked(&runtime, "a word", reply, move |value| {
                sink.borrow_mut().push(value);
            });
        }
        assert!(run_until(&runtime, TIMEOUT, || !got.borrow().is_empty()));
        runtime.tick();
        assert_eq!(*got.borrow(), vec![Some("newer")]);
    }
}
