//! The playback session as the UI holds it: the startup restore, the playback
//! cursor, and the save on close.
//!
//! [`SessionState::start_restore`] runs the facade's restore
//! ([`session::restore`]) once, when the main window is built, and awaits its
//! reply on the main-thread executor. Until the reply lands,
//! [`SessionState::status`] is not settled ([`RestoreStatus::is_settled`]):
//! the view-models send no playback command before it is (quirk fix 6).
//!
//! The restore hands back the [`PlaybackCursor`] (the restored playlist with
//! the playing song). It lives here, shared, because two parties need it: the
//! playlist view-model, which moves it as the user plays, and the window's
//! close hook, which saves its position ([`SessionState::persist_now`], quirk
//! fix 3).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use teksilo::prelude::{AsyncRuntimeHandle, Signal};

use crate::frontend::commands::engine_settings::RestoreReport;
use crate::frontend::commands::playback::PlaybackCursor;
use crate::frontend::events::RestoredSession;
use crate::frontend::session::{self, SessionRestore};
use crate::frontend::{AppContext, FrontendError, Reply};

/// Where the startup restore stands.
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreStatus {
    /// Not started yet.
    Idle,
    /// Started; the reply has not arrived.
    Pending,
    /// Done: what was restored.
    Restored(RestoredSession),
    /// Failed: the error, as text. The UI starts empty, as Iced does when its
    /// database reads fail.
    Failed(String),
}

impl RestoreStatus {
    /// Whether the restore has finished, successfully or not: from then on the
    /// view-models may send playback commands.
    pub fn is_settled(&self) -> bool {
        matches!(self, Self::Restored(_) | Self::Failed(_))
    }
}

struct Inner {
    status: Signal<RestoreStatus>,
    cursor: RefCell<Option<PlaybackCursor>>,
    cursor_version: Signal<u64>,
    settings: RefCell<Option<RestoreReport>>,
    started: Cell<bool>,
}

/// The session, shared by every clone. UI thread only.
#[derive(Clone)]
pub struct SessionState {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionState")
            .field("status", &self.inner.status.get())
            .field("has_cursor", &self.inner.cursor.borrow().is_some())
            .finish_non_exhaustive()
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionState {
    /// A session that has not been restored.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                status: Signal::new(RestoreStatus::Idle),
                cursor: RefCell::new(None),
                cursor_version: Signal::new(0),
                settings: RefCell::new(None),
                started: Cell::new(false),
            }),
        }
    }

    /// The restore's progress. The gate of quirk fix 6: playback commands wait
    /// for [`RestoreStatus::is_settled`].
    pub fn status(&self) -> Signal<RestoreStatus> {
        self.inner.status.clone()
    }

    /// Starts the startup restore on `ctx` and awaits it on `runtime`. Only
    /// the first call does anything.
    pub fn start_restore(&self, ctx: &AppContext, runtime: &AsyncRuntimeHandle) {
        if self.inner.started.replace(true) {
            return;
        }
        self.inner.status.set(RestoreStatus::Pending);
        let reply = session::restore(ctx);
        let state = self.clone();
        runtime
            .spawn_local(async move { state.apply(reply.recv().await) })
            .detach();
    }

    /// Records the restore's outcome.
    fn apply(&self, outcome: Result<SessionRestore, FrontendError>) {
        match outcome {
            Ok(restore) => {
                *self.inner.cursor.borrow_mut() = Some(restore.cursor);
                *self.inner.settings.borrow_mut() = Some(restore.settings);
                self.bump_cursor_version();
                self.inner
                    .status
                    .set(RestoreStatus::Restored(restore.session));
            }
            Err(error) => {
                tracing::error!("the session could not be restored: {error}");
                self.inner
                    .status
                    .set(RestoreStatus::Failed(error.to_string()));
            }
        }
    }

    /// Runs `f` on the playback cursor; `None` until the restore succeeded.
    ///
    /// `f` may change the cursor, so [`SessionState::cursor_version`] moves
    /// after it runs (outside the borrow, so an observer may read the cursor).
    /// Do not call it again from inside `f`: the cursor is borrowed.
    pub fn with_cursor<R>(&self, f: impl FnOnce(&mut PlaybackCursor) -> R) -> Option<R> {
        let result = self.inner.cursor.borrow_mut().as_mut().map(f);
        if result.is_some() {
            self.bump_cursor_version();
        }
        result
    }

    /// Runs `f` on the playback cursor without changing it; `None` until the
    /// restore succeeded. The version does not move.
    pub fn read_cursor<R>(&self, f: impl FnOnce(&PlaybackCursor) -> R) -> Option<R> {
        self.inner.cursor.borrow().as_ref().map(f)
    }

    /// A counter that moves after each [`SessionState::with_cursor`] and when
    /// the restore installs the cursor: what a view showing the playlist or
    /// the playing song observes, whichever panel moved the cursor.
    pub fn cursor_version(&self) -> Signal<u64> {
        self.inner.cursor_version.clone()
    }

    fn bump_cursor_version(&self) {
        let version = &self.inner.cursor_version;
        version.set(version.get().wrapping_add(1));
    }

    /// What the settings restore applied, taken once (the Audio Center starts
    /// from it).
    pub fn take_settings(&self) -> Option<RestoreReport> {
        self.inner.settings.borrow_mut().take()
    }

    /// Saves the playback position now (quirk fix 3). `None` when there is no
    /// cursor to save (the restore has not succeeded).
    pub fn persist_now(&self, ctx: &AppContext) -> Option<Reply<()>> {
        self.inner
            .cursor
            .borrow()
            .as_ref()
            .map(|cursor| session::persist_now(ctx, cursor))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::frontend::test_fixtures::LibraryFixture;
    use crate::teksilo_ui::test_support::run_until;

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn seeded_context() -> AppContext {
        let ctx = AppContext::in_memory().unwrap();
        LibraryFixture::demo()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        ctx
    }

    #[test]
    fn the_restore_completes_on_the_main_thread_executor() {
        let ctx = seeded_context();
        let runtime = AsyncRuntimeHandle::new();
        let state = SessionState::new();
        assert_eq!(state.status().get(), RestoreStatus::Idle);
        state.start_restore(&ctx, &runtime);
        assert_eq!(state.status().get(), RestoreStatus::Pending);
        assert!(run_until(&runtime, TIMEOUT, || state
            .status()
            .get()
            .is_settled()));
        assert!(matches!(state.status().get(), RestoreStatus::Restored(_)));
        assert!(state.with_cursor(|cursor| cursor.playlist_id()).is_some());
        assert!(state.take_settings().is_some());
        assert!(state.take_settings().is_none());
        assert!(ctx.shutdown());
    }

    #[test]
    fn the_cursor_version_moves_on_restore_and_after_every_change() {
        let ctx = seeded_context();
        let runtime = AsyncRuntimeHandle::new();
        let state = SessionState::new();
        let version = state.cursor_version();
        assert_eq!(version.get(), 0);
        assert!(state.with_cursor(|_| ()).is_none());
        assert_eq!(version.get(), 0, "no cursor, no change");
        state.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || state
            .status()
            .get()
            .is_settled()));
        assert_eq!(version.get(), 1);
        state.with_cursor(|cursor| cursor.set_search("x"));
        assert_eq!(version.get(), 2);
        assert!(state.read_cursor(|cursor| cursor.playlist_id()).is_some());
        assert_eq!(version.get(), 2, "reading does not move it");
        assert!(ctx.shutdown());
    }

    #[test]
    fn the_restore_runs_once() {
        let ctx = seeded_context();
        let runtime = AsyncRuntimeHandle::new();
        let state = SessionState::new();
        state.start_restore(&ctx, &runtime);
        state.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || state
            .status()
            .get()
            .is_settled()));
        assert!(matches!(state.status().get(), RestoreStatus::Restored(_)));
        assert!(ctx.shutdown());
    }

    #[test]
    fn a_failed_restore_settles_with_the_error() {
        let ctx = AppContext::in_memory().unwrap();
        // A second restore on the same context is refused by the facade.
        session::restore(&ctx).recv_blocking().ok();
        let runtime = AsyncRuntimeHandle::new();
        let state = SessionState::new();
        state.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || state
            .status()
            .get()
            .is_settled()));
        assert!(matches!(state.status().get(), RestoreStatus::Failed(_)));
        assert!(state.with_cursor(|_| ()).is_none());
        assert!(state.persist_now(&ctx).is_none());
        assert!(ctx.shutdown());
    }

    #[test]
    fn persisting_after_the_restore_writes_the_position() {
        let ctx = seeded_context();
        let runtime = AsyncRuntimeHandle::new();
        let state = SessionState::new();
        state.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || state
            .status()
            .get()
            .is_settled()));
        let reply = state.persist_now(&ctx).unwrap();
        assert!(reply.recv_blocking().is_ok());
        assert!(ctx.shutdown());
    }
}
