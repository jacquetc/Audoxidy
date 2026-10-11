//! `SingleActivePlaylist`: which playlist is active, and how it plays.
//!
//! Signals for the active playlist's id, name, system flag, shuffle and
//! repeat. They are filled when the startup restore settles
//! ([`SessionState::status`] becomes [`RestoreStatus::Restored`]: the restore
//! names the playlist, its shuffle and its repeat; the name and the system
//! flag come from `commands::playlist::list_playlists`), set by
//! [`set`](SingleActivePlaylist::set) when the playlist view-model switches to
//! another playlist, and refreshed on [`PlaylistsEvent::ListChanged`] so a
//! rename shows (coalesced). Shuffle and repeat are plain signals: the
//! view-model writes them after it toggled them.
//!
//! The system flag drives quirk fix 16: the "Eliminar lista" entry is
//! disabled on a system playlist.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use teksilo::core::signal::ObserverHandle;
use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use crate::frontend::commands::playlist;
use crate::frontend::events::{PlaylistsEvent, RestoredSession};
use crate::frontend::{AppContext, Event, Latest, Origin};
use crate::logic::playlist_nav::RepeatMode;
use crate::teksilo_ui::app::session::{RestoreStatus, SessionState};
use crate::teksilo_ui::models::coalesced_reload::{ReloadCoalescer, reload_on_events};
use crate::teksilo_ui::models::load::{finish_tracked, prepare_runtime};

/// Whether `event` may have renamed or removed the active playlist.
pub fn is_list_event(event: &Event) -> bool {
    matches!(event, Event::Playlists(PlaylistsEvent::ListChanged))
}

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    id: Signal<Option<i64>>,
    name: Signal<String>,
    is_system: Signal<bool>,
    shuffle: Signal<bool>,
    repeat: Signal<RepeatMode>,
    latest: Latest,
    /// Keeps the subscription to the restore's status alive.
    observer: RefCell<Option<ObserverHandle>>,
}

/// The active playlist. UI thread only; clones share one state.
#[derive(Clone)]
pub struct SingleActivePlaylist {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for SingleActivePlaylist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleActivePlaylist")
            .field("id", &self.inner.id.get())
            .field("name", &self.inner.name.get())
            .finish_non_exhaustive()
    }
}

impl SingleActivePlaylist {
    /// A single with no active playlist, that fills itself when `session`'s
    /// restore settles (at once if it already did).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle, session: &SessionState) -> Self {
        let inner = Rc::new(Inner {
            ctx,
            runtime,
            id: Signal::new(None),
            name: Signal::new(String::new()),
            is_system: Signal::new(false),
            shuffle: Signal::new(false),
            repeat: Signal::new(RepeatMode::Off),
            latest: Latest::new(),
            observer: RefCell::new(None),
        });
        let single = Self { inner };
        let weak: Weak<Inner> = Rc::downgrade(&single.inner);
        let status = session.status();
        let handle = status.observe(move |status| {
            if let (RestoreStatus::Restored(restored), Some(inner)) = (status, weak.upgrade()) {
                Self { inner }.init_from_restore(restored);
            }
        });
        *single.inner.observer.borrow_mut() = Some(handle);
        if let RestoreStatus::Restored(restored) = status.get() {
            single.init_from_restore(&restored);
        }
        single
    }

    /// Subscribes the single to the playlist list's changes; the refresh runs
    /// once per frame however many arrive. Call it from a long-lived widget's
    /// `build`.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let single = self.clone();
        reload_on_events(ctx, [Origin::Playlists], is_list_event, move || {
            single.refresh();
        })
    }

    /// The active playlist's id; `None` until the restore settled.
    pub fn id(&self) -> Signal<Option<i64>> {
        self.inner.id.clone()
    }

    /// The active playlist's name.
    pub fn name(&self) -> Signal<String> {
        self.inner.name.clone()
    }

    /// Whether the active playlist is a system playlist.
    pub fn is_system(&self) -> Signal<bool> {
        self.inner.is_system.clone()
    }

    /// Whether shuffle is on for the active playlist.
    pub fn shuffle(&self) -> Signal<bool> {
        self.inner.shuffle.clone()
    }

    /// The active playlist's repeat mode.
    pub fn repeat(&self) -> Signal<RepeatMode> {
        self.inner.repeat.clone()
    }

    /// Sets the active playlist, after the view-model switched to it.
    pub fn set(&self, id: i64, name: &str, is_system: bool, shuffle: bool, repeat: RepeatMode) {
        let inner = &self.inner;
        inner.id.set_if_changed(Some(id));
        inner.name.set_if_changed(name.to_string());
        inner.is_system.set_if_changed(is_system);
        inner.shuffle.set_if_changed(shuffle);
        inner.repeat.set_if_changed(repeat);
    }

    /// Takes the playlist, shuffle and repeat the restore put back, then reads
    /// the name and the system flag.
    fn init_from_restore(&self, restored: &RestoredSession) {
        let inner = &self.inner;
        inner.id.set_if_changed(Some(restored.playlist_id));
        inner.shuffle.set_if_changed(restored.shuffle);
        inner.repeat.set_if_changed(restored.repeat);
        self.refresh();
    }

    /// Reads the active playlist's name and system flag again (a rename, or
    /// the first read after the restore). A playlist that is no longer listed
    /// leaves the signals as they are: the view-model switches away from it.
    pub fn refresh(&self) {
        let inner = &self.inner;
        let Some(active) = inner.id.get() else {
            return;
        };
        let reply = inner.latest.track(playlist::list_playlists(&inner.ctx));
        let single = self.clone();
        finish_tracked(&inner.runtime, "the playlists", reply, move |playlists| {
            let found = playlists
                .into_iter()
                .flatten()
                .find(|playlist| playlist.id == active);
            if let Some(found) = found {
                single.inner.name.set_if_changed(found.name);
                single.inner.is_system.set_if_changed(found.is_system);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::{run_until, tree_with_async};

    fn restored() -> (AppContext, AsyncRuntimeHandle, SessionState, i64) {
        let (ctx, seeded) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let session = SessionState::new();
        session.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || session
            .status()
            .get()
            .is_settled()));
        let bowie = seeded.playlist("David Bowie").unwrap().id;
        (ctx, runtime, session, bowie)
    }

    #[test]
    fn nothing_is_active_before_the_restore() {
        let (ctx, _) = test_env::screenshot();
        let single =
            SingleActivePlaylist::new(ctx, AsyncRuntimeHandle::new(), &SessionState::new());
        assert_eq!(single.id().get(), None);
        assert_eq!(single.name().get(), "");
    }

    #[test]
    fn the_restore_fills_the_playlist_its_name_and_its_play_modes() {
        let (ctx, _) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let session = SessionState::new();
        let single = SingleActivePlaylist::new(ctx.clone(), runtime.clone(), &session);
        session.start_restore(&ctx, &runtime);
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "David Bowie"));
        assert!(single.id().get().is_some());
        assert!(!single.is_system().get());
        assert!(!single.shuffle().get());
        assert_eq!(single.repeat().get(), RepeatMode::Off);
    }

    #[test]
    fn a_single_built_after_the_restore_fills_itself_at_once() {
        let (ctx, runtime, session, bowie) = restored();
        let single = SingleActivePlaylist::new(ctx, runtime.clone(), &session);
        assert_eq!(single.id().get(), Some(bowie));
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "David Bowie"));
    }

    #[test]
    fn a_switch_sets_every_signal() {
        let (ctx, runtime, session, _) = restored();
        let single = SingleActivePlaylist::new(ctx, runtime, &session);
        single.set(1, "Audoxidy", true, true, RepeatMode::All);
        assert_eq!(single.id().get(), Some(1));
        assert_eq!(single.name().get(), "Audoxidy");
        assert!(single.is_system().get());
        assert!(single.shuffle().get());
        assert_eq!(single.repeat().get(), RepeatMode::All);
    }

    #[test]
    fn a_rename_shows_after_a_refresh() {
        let (ctx, runtime, session, bowie) = restored();
        let single = SingleActivePlaylist::new(ctx.clone(), runtime.clone(), &session);
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "David Bowie"));
        playlist::rename_playlist(&ctx, bowie, "Bowie")
            .recv_blocking()
            .unwrap();
        single.refresh();
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "Bowie"));
        assert_eq!(single.id().get(), Some(bowie));
    }

    #[test]
    fn a_playlist_that_is_gone_leaves_the_signals_alone() {
        let (ctx, runtime, session, bowie) = restored();
        let single = SingleActivePlaylist::new(ctx.clone(), runtime.clone(), &session);
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "David Bowie"));
        playlist::delete_playlist(&ctx, bowie)
            .recv_blocking()
            .unwrap();
        single.refresh();
        let changed = run_until(&runtime, std::time::Duration::from_millis(300), || {
            single.name().get() != "David Bowie"
        });
        assert!(!changed);
    }

    #[test]
    fn only_a_change_of_the_list_refreshes() {
        assert!(is_list_event(&Event::Playlists(
            PlaylistsEvent::ListChanged
        )));
        assert!(!is_list_event(&Event::Playlists(
            PlaylistsEvent::ContentChanged { playlist_id: 1 }
        )));
    }

    #[test]
    fn a_burst_of_list_events_refreshes_once_per_frame() {
        let (ctx, runtime, session, bowie) = restored();
        let (mut tree, _) = tree_with_async(&ctx);
        let single = SingleActivePlaylist::new(ctx.clone(), runtime.clone(), &session);
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "David Bowie"));
        let slot = Rc::new(RefCell::new(None));
        {
            let (single, slot) = (single.clone(), slot.clone());
            test_env::mount(&mut tree, move |build| {
                *slot.borrow_mut() = Some(single.wire(build));
            });
        }
        playlist::rename_playlist(&ctx, bowie, "Bowie")
            .recv_blocking()
            .unwrap();
        let coalescer = slot.borrow().clone().unwrap();
        let wake = test_env::wake();
        for _ in 0..10 {
            coalescer.mark(&wake);
        }
        tree.frame_tick().set(16.0);
        assert!(run_until(&runtime, TIMEOUT, || single.name().get() == "Bowie"));
    }
}
