//! `PlaylistContentModel`: keeps the rows of the **active** playlist current.
//!
//! The rows live in the [`PlaybackCursor`]
//! that [`SessionState`] holds (the playlist view-model moves it as the user
//! plays); this model watches the facade for changes to them. On
//! [`PlaylistsEvent::ContentChanged`] of the active playlist it asks
//! `commands::playlist::reload_content` for the rows and applies the answer to
//! the cursor with `PlaylistContent::apply`, then bumps a version signal the
//! playlist view watches.
//!
//! Only the rows are replaced. The playing track is never reloaded, re-seeked
//! or given a new play state, and repeat, shuffle and the shuffle session stay
//! (quirk fix 12; Iced switches to the playlist again, which does all three).
//! An answer for a playlist that is no longer the cursor's is dropped by
//! `apply`, and a newer reload supersedes an older one.
//!
//! Whoever else changes the cursor's rows (a switch of playlist, a search)
//! calls [`notify_changed`](PlaylistContentModel::notify_changed) so the view
//! rebuilds through the same signal.
//!
//! A switch of playlist is announced with
//! [`switch_started`](PlaylistContentModel::switch_started): a content change
//! of the playlist being loaded arrives while the cursor still shows the old
//! one, so it is remembered, and
//! [`switch_finished`](PlaylistContentModel::switch_finished) tells the
//! view-model to reload the rows it just applied, which may predate it.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use super::coalesced_reload::{ReloadCoalescer, reload_on_events};
use super::load::{finish_tracked, prepare_runtime};
use crate::db::database::PlaylistFolderGroup;
use crate::frontend::commands::playback::PlaybackCursor;
use crate::frontend::commands::playlist;
use crate::frontend::events::PlaylistsEvent;
use crate::frontend::{AppContext, Event, Latest, Origin};
use crate::teksilo_ui::app::session::SessionState;

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    session: SessionState,
    latest: Latest,
    version: Signal<u64>,
    switching: Cell<Option<i64>>,
    missed: Cell<bool>,
}

/// The active playlist's rows, kept current. UI thread only; clones share one
/// state.
#[derive(Clone)]
pub struct PlaylistContentModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for PlaylistContentModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistContentModel")
            .field("version", &self.inner.version.get())
            .finish_non_exhaustive()
    }
}

impl PlaylistContentModel {
    /// A model over the cursor of `session`. There is nothing to load until
    /// the restore has given the session a cursor.
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle, session: SessionState) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                session,
                latest: Latest::new(),
                version: Signal::new(0),
                switching: Cell::new(None),
                missed: Cell::new(false),
            }),
        }
    }

    /// Whether `event` is a change to the rows of the active playlist. A
    /// change to the playlist a switch is loading is remembered for
    /// [`switch_finished`](Self::switch_finished).
    pub fn is_content_event(&self, event: &Event) -> bool {
        let Event::Playlists(PlaylistsEvent::ContentChanged { playlist_id }) = event else {
            return false;
        };
        if self.inner.switching.get() == Some(*playlist_id) {
            self.inner.missed.set(true);
        }
        self.playlist_id() == Some(*playlist_id)
    }

    /// A switch to `playlist_id` started: its content changes are remembered
    /// until [`switch_finished`](Self::switch_finished). A newer switch
    /// replaces an older one.
    pub fn switch_started(&self, playlist_id: i64) {
        self.inner.switching.set(Some(playlist_id));
        self.inner.missed.set(false);
    }

    /// The switch to `playlist_id` was applied. Returns whether the
    /// playlist's content changed while it loaded, so the rows applied may be
    /// older than the change and must be reloaded (the change's event was not
    /// for the cursor's playlist when it arrived).
    pub fn switch_finished(&self, playlist_id: i64) -> bool {
        if self.inner.switching.get() != Some(playlist_id) {
            return false;
        }
        self.inner.switching.set(None);
        self.inner.missed.replace(false)
    }

    /// Subscribes the model to the content changes of the active playlist; the
    /// reload runs once per frame however many arrive. Call it from a
    /// long-lived widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let (filter, reload) = (self.clone(), self.clone());
        reload_on_events(
            ctx,
            [Origin::Playlists],
            move |event| filter.is_content_event(event),
            move || reload.reload(),
        )
    }

    /// A counter that bumps whenever the rows changed: after a reload was
    /// applied, and on [`notify_changed`](Self::notify_changed). The playlist
    /// view rebuilds on it.
    pub fn version(&self) -> Signal<u64> {
        self.inner.version.clone()
    }

    /// The active playlist's id; `None` until the restore succeeded.
    pub fn playlist_id(&self) -> Option<i64> {
        self.inner.session.read_cursor(PlaybackCursor::playlist_id)
    }

    /// Runs `f` on the rows on screen (the search result when a search is
    /// applied); `None` until the restore succeeded.
    pub fn with_groups<R>(&self, f: impl FnOnce(&[PlaylistFolderGroup]) -> R) -> Option<R> {
        self.inner
            .session
            .read_cursor(|cursor| f(cursor.active_groups()))
    }

    /// Runs `f` on the cursor; `None` until the restore succeeded.
    pub fn with_cursor<R>(&self, f: impl FnOnce(&mut PlaybackCursor) -> R) -> Option<R> {
        self.inner.session.with_cursor(f)
    }

    /// Tells the playlist view that the cursor's rows changed by other means
    /// (a switch of playlist, a search).
    pub fn notify_changed(&self) {
        let version = &self.inner.version;
        version.set(version.get().wrapping_add(1));
    }

    /// Reloads the rows of the active playlist and applies them to the cursor.
    /// Does nothing before the restore succeeded.
    pub fn reload(&self) {
        let inner = &self.inner;
        let Some(reply) = inner
            .session
            .with_cursor(|cursor| playlist::reload_content(&inner.ctx, cursor))
        else {
            return;
        };
        let model = self.clone();
        finish_tracked(
            &inner.runtime,
            "the playlist content",
            inner.latest.track(reply),
            move |content| {
                let Some(content) = content else { return };
                let applied = model
                    .inner
                    .session
                    .with_cursor(|cursor| content.apply(cursor))
                    .unwrap_or(false);
                if applied {
                    model.notify_changed();
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::commands::playlist as commands;
    use crate::frontend::test_fixtures::SeededLibrary;
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::{run_until, tree_with_async};

    /// A restored session over the screenshot library: the active playlist is
    /// "David Bowie".
    fn restored() -> (
        AppContext,
        SeededLibrary,
        AsyncRuntimeHandle,
        SessionState,
        PlaylistContentModel,
    ) {
        let (ctx, seeded) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let session = SessionState::new();
        session.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || session
            .status()
            .get()
            .is_settled()));
        let model = PlaylistContentModel::new(ctx.clone(), runtime.clone(), session.clone());
        (ctx, seeded, runtime, session, model)
    }

    fn row_count(model: &PlaylistContentModel) -> usize {
        model
            .with_groups(|groups| groups.iter().map(|g| g.songs.len()).sum())
            .unwrap()
    }

    #[test]
    fn there_is_nothing_to_read_or_reload_before_the_restore() {
        let (ctx, _) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let model = PlaylistContentModel::new(ctx, runtime.clone(), SessionState::new());
        assert_eq!(model.playlist_id(), None);
        assert!(model.with_groups(|groups| groups.len()).is_none());
        model.reload();
        runtime.tick();
        assert_eq!(model.version().get(), 0);
    }

    #[test]
    fn the_active_playlist_is_the_restored_one() {
        let (_ctx, seeded, _runtime, _session, model) = restored();
        assert_eq!(
            model.playlist_id(),
            Some(seeded.playlist("David Bowie").unwrap().id)
        );
        assert_eq!(model.with_groups(|groups| groups.len()), Some(2));
    }

    #[test]
    fn added_songs_appear_without_touching_the_playing_track() {
        let (ctx, seeded, runtime, session, model) = restored();
        let bowie = seeded.playlist("David Bowie").unwrap();
        let playing_before = session.with_cursor(|c| c.playing_song_id()).unwrap();
        let before = row_count(&model);

        let extra: Vec<i64> = seeded
            .songs
            .iter()
            .map(|song| song.id)
            .filter(|id| !bowie.song_ids.contains(id))
            .take(3)
            .collect();
        commands::add_songs(&ctx, bowie.id, extra.clone())
            .recv_blocking()
            .unwrap();
        model.reload();
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        assert_eq!(row_count(&model), before + extra.len());
        assert_eq!(
            session.with_cursor(|c| c.playing_song_id()).unwrap(),
            playing_before,
            "quirk fix 12: the playing track is untouched"
        );
    }

    #[test]
    fn only_a_content_change_of_the_active_playlist_is_wanted() {
        let (_ctx, seeded, _runtime, _session, model) = restored();
        let active = seeded.playlist("David Bowie").unwrap().id;
        let content =
            |playlist_id| Event::Playlists(PlaylistsEvent::ContentChanged { playlist_id });
        assert!(model.is_content_event(&content(active)));
        assert!(!model.is_content_event(&content(active + 100)));
        assert!(!model.is_content_event(&Event::Playlists(PlaylistsEvent::ListChanged)));
    }

    #[test]
    fn a_content_change_of_the_playlist_a_switch_loads_is_remembered() {
        let (_ctx, seeded, _runtime, _session, model) = restored();
        let active = seeded.playlist("David Bowie").unwrap().id;
        let other = seeded.playlist("Audoxidy").unwrap().id;
        let content =
            |playlist_id| Event::Playlists(PlaylistsEvent::ContentChanged { playlist_id });
        assert!(!model.switch_finished(other), "no switch");
        model.switch_started(other);
        assert!(!model.is_content_event(&content(other)), "not shown yet");
        assert!(model.is_content_event(&content(active)));
        assert!(model.switch_finished(other), "its rows may be stale");
        assert!(!model.switch_finished(other), "once");
        // A switch with no change meanwhile, and one overtaken by another.
        model.switch_started(other);
        assert!(!model.switch_finished(other));
        model.switch_started(other);
        model.switch_started(active);
        assert!(!model.is_content_event(&content(other)));
        assert!(!model.switch_finished(other));
        assert!(!model.switch_finished(active));
    }

    #[test]
    fn an_answer_for_a_playlist_that_is_no_longer_active_is_dropped() {
        let (_ctx, _seeded, runtime, session, model) = restored();
        model.reload();
        session.with_cursor(|cursor| cursor.playlist_id = 999);
        let applied = run_until(&runtime, std::time::Duration::from_millis(300), || {
            model.version().get() > 0
        });
        assert!(!applied, "the answer belongs to another playlist");
    }

    #[test]
    fn a_newer_reload_supersedes_an_older_one() {
        let (_ctx, _seeded, runtime, _session, model) = restored();
        model.reload();
        model.reload();
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        runtime.tick();
        assert_eq!(model.version().get(), 1);
    }

    #[test]
    fn notifying_a_change_bumps_the_version() {
        let (_ctx, _seeded, _runtime, _session, model) = restored();
        model.notify_changed();
        assert_eq!(model.version().get(), 1);
    }

    #[test]
    fn a_burst_of_content_events_reloads_once_per_frame() {
        let (ctx, _seeded, runtime, _session, model) = restored();
        let (mut tree, _runtime) = tree_with_async(&ctx);
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        {
            let (model, slot) = (model.clone(), slot.clone());
            test_env::mount(&mut tree, move |build| {
                *slot.borrow_mut() = Some(model.wire(build));
            });
        }
        let coalescer = slot.borrow().clone().unwrap();
        let wake = test_env::wake();
        for _ in 0..30 {
            coalescer.mark(&wake);
        }
        tree.frame_tick().set(16.0);
        tree.frame_tick().set(16.0);
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        runtime.tick();
        assert_eq!(model.version().get(), 1);
    }
}
