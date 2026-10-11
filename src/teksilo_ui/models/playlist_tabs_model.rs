//! `PlaylistTabsModel`: the playlist tabs above the playlist.
//!
//! A `ListModel<PlaylistTab>` keyed by playlist id, from
//! `commands::playlist::list_playlists` (tab order), reloaded on
//! [`PlaylistsEvent::ListChanged`] (a playlist was created, renamed,
//! reordered or deleted). Reconcile by id keeps a tab's identity, so a rename
//! updates the tab in place and a deletion removes just that tab.
//!
//! Each tab knows whether it is a system playlist ("Audoxidy" and "Oxidy
//! Drift", which the database creates): a system playlist cannot be deleted
//! (the menu entry "Eliminar lista" is disabled on it, quirk fix 16). Renaming
//! one makes a renamed copy instead, as in Iced
//! (`commands::playlist::RenameOutcome::Copied`).

use std::rc::Rc;

use teksilo::data::ListModel;
use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use super::coalesced_reload::{ReloadCoalescer, reload_on_events};
use super::load::{finish_tracked, prepare_runtime};
use super::reconcile::reconcile_unique;
use crate::db::database::PlaylistData;
use crate::frontend::commands::playlist;
use crate::frontend::events::PlaylistsEvent;
use crate::frontend::{AppContext, Event, Latest, Origin};

/// One playlist tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistTab {
    /// The playlist's database id; the tab's identity.
    pub id: i64,
    /// The playlist's name.
    pub name: String,
    /// Whether it is a system playlist.
    pub is_system: bool,
}

impl PlaylistTab {
    /// Whether the playlist can be deleted: not a system playlist (quirk fix
    /// 16).
    pub fn can_delete(&self) -> bool {
        !self.is_system
    }
}

impl From<PlaylistData> for PlaylistTab {
    fn from(data: PlaylistData) -> Self {
        Self {
            id: data.id,
            name: data.name,
            is_system: data.is_system,
        }
    }
}

/// Whether `event` changes the list of playlists.
pub fn is_list_event(event: &Event) -> bool {
    matches!(event, Event::Playlists(PlaylistsEvent::ListChanged))
}

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    list: ListModel<PlaylistTab>,
    latest: Latest,
    version: Signal<u64>,
}

/// The playlist tabs. UI thread only; clones share one list.
#[derive(Clone)]
pub struct PlaylistTabsModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for PlaylistTabsModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistTabsModel")
            .field("tabs", &self.inner.list.len())
            .finish_non_exhaustive()
    }
}

impl PlaylistTabsModel {
    /// An empty model. Nothing is loaded until [`reload`](Self::reload).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                list: ListModel::new(),
                latest: Latest::new(),
                version: Signal::new(0),
            }),
        }
    }

    /// Subscribes the model to [`PlaylistsEvent::ListChanged`]; the reload
    /// runs once per frame however many arrive. Call it from a long-lived
    /// widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let model = self.clone();
        reload_on_events(ctx, [Origin::Playlists], is_list_event, move || {
            model.reload();
        })
    }

    /// The tabs, in tab order.
    pub fn list(&self) -> ListModel<PlaylistTab> {
        self.inner.list.clone()
    }

    /// A counter that bumps each time the list was reloaded (even to the same
    /// tabs).
    pub fn version(&self) -> Signal<u64> {
        self.inner.version.clone()
    }

    /// The tab of playlist `id`.
    pub fn tab(&self, id: i64) -> Option<PlaylistTab> {
        let list = &self.inner.list;
        (0..list.len()).find_map(|index| {
            list.with_item(index, |tab| (tab.id == id).then(|| tab.clone()))
                .flatten()
        })
    }

    /// Queries the playlists again.
    pub fn reload(&self) {
        let inner = &self.inner;
        let reply = inner.latest.track(playlist::list_playlists(&inner.ctx));
        let model = self.clone();
        finish_tracked(&inner.runtime, "the playlists", reply, move |playlists| {
            let Some(playlists) = playlists else { return };
            model.replace(playlists.into_iter().map(PlaylistTab::from).collect());
        });
    }

    /// Replaces the tabs with `tabs` by keyed reconcile.
    fn replace(&self, tabs: Vec<PlaylistTab>) {
        reconcile_unique(&self.inner.list, tabs, |tab| tab.id);
        let version = &self.inner.version;
        version.set(version.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use teksilo::data::DataChange;

    use super::*;
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::run_until;

    fn tabs(model: &PlaylistTabsModel) -> Vec<PlaylistTab> {
        let list = model.list();
        (0..list.len())
            .filter_map(|i| list.with_item(i, Clone::clone))
            .collect()
    }

    fn load(model: &PlaylistTabsModel, runtime: &AsyncRuntimeHandle, version: u64) {
        model.reload();
        assert!(run_until(runtime, TIMEOUT, || model.version().get() >= version));
    }

    #[test]
    fn the_tabs_are_the_playlists_in_tab_order_with_their_system_flag() {
        let (ctx, _) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let model = PlaylistTabsModel::new(ctx.clone(), runtime.clone());
        load(&model, &runtime, 1);
        let tabs = tabs(&model);
        let names: Vec<&str> = tabs.iter().map(|tab| tab.name.as_str()).collect();
        assert_eq!(names, vec!["Audoxidy", "Oxidy Drift", "David Bowie"]);
        assert_eq!(
            tabs.iter().map(|tab| tab.is_system).collect::<Vec<_>>(),
            vec![true, true, false]
        );
    }

    #[test]
    fn a_system_playlist_cannot_be_deleted() {
        let system = PlaylistTab {
            id: 1,
            name: "Audoxidy".into(),
            is_system: true,
        };
        let user = PlaylistTab {
            id: 3,
            name: "Mine".into(),
            is_system: false,
        };
        assert!(!system.can_delete());
        assert!(user.can_delete());
    }

    #[test]
    fn a_rename_updates_the_tab_in_place_and_a_creation_appends_one() {
        let (ctx, seeded) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let model = PlaylistTabsModel::new(ctx.clone(), runtime.clone());
        load(&model, &runtime, 1);
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = log.clone();
        let _handle = model.list().observe_changes(move |change: &DataChange| {
            sink.borrow_mut().push(format!("{change:?}"));
        });

        load(&model, &runtime, 2);
        assert!(log.borrow().is_empty(), "an unchanged reload is silent");

        let bowie = seeded.playlist("David Bowie").unwrap().id;
        playlist::rename_playlist(&ctx, bowie, "Bowie")
            .recv_blocking()
            .unwrap();
        load(&model, &runtime, 3);
        assert_eq!(model.tab(bowie).unwrap().name, "Bowie");
        assert_eq!(log.borrow().len(), 1);
        assert!(log.borrow()[0].starts_with("ItemUpdated"));

        playlist::create_playlist(&ctx, "Mixtape", Vec::new())
            .recv_blocking()
            .unwrap();
        load(&model, &runtime, 4);
        assert_eq!(tabs(&model).len(), 4);
        assert!(log.borrow()[1].starts_with("ItemsInserted"));
    }

    #[test]
    fn a_deletion_removes_only_that_tab() {
        let (ctx, seeded) = test_env::screenshot();
        let runtime = AsyncRuntimeHandle::new();
        let model = PlaylistTabsModel::new(ctx.clone(), runtime.clone());
        load(&model, &runtime, 1);
        let bowie = seeded.playlist("David Bowie").unwrap().id;
        playlist::delete_playlist(&ctx, bowie)
            .recv_blocking()
            .unwrap();
        load(&model, &runtime, 2);
        assert!(model.tab(bowie).is_none());
        assert_eq!(tabs(&model).len(), 2);
    }

    #[test]
    fn only_a_change_of_the_list_reloads_the_tabs() {
        assert!(is_list_event(&Event::Playlists(
            PlaylistsEvent::ListChanged
        )));
        assert!(!is_list_event(&Event::Playlists(
            PlaylistsEvent::ContentChanged { playlist_id: 1 }
        )));
    }
}
