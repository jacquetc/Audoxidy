use std::time::Duration;

use super::*;
use crate::frontend::AppContext;
use crate::frontend::test_fixtures::LibraryFixture;
use crate::teksilo_ui::test_support::{VIEWPORT, layout, run_until, tree_with_async};

const TIMEOUT: Duration = Duration::from_secs(10);

fn mounted() -> (
    WidgetTree,
    AsyncRuntimeHandle,
    AppHandles,
    Regions,
    WidgetId,
) {
    let ctx = AppContext::in_memory().unwrap();
    LibraryFixture::demo()
        .seed_on(&ctx.db)
        .recv_blocking()
        .unwrap();
    let handles = AppHandles::new(ctx.clone());
    let (mut tree, runtime) = tree_with_async(&ctx);
    let app = App::new(handles.clone(), None);
    let regions = app.regions();
    let app_id = tree.add(app);
    layout(&mut tree);
    let regions = regions.get().unwrap();
    (tree, runtime, handles, regions, app_id)
}

fn rect(tree: &WidgetTree, id: WidgetId) -> (f32, f32, f32, f32) {
    let b = tree.bounds(id);
    (b.x, b.y, b.width, b.height)
}

#[test]
fn the_regions_have_the_iced_geometry() {
    let (tree, _runtime, handles, r, _) = mounted();
    let (w, h) = VIEWPORT;
    assert_eq!(rect(&tree, r.player), (0.0, 0.0, 400.0, 400.0));
    assert_eq!(rect(&tree, r.playlist), (0.0, 400.0, 400.0, h - 400.0));
    assert_eq!(rect(&tree, r.title_bar), (400.0, 0.0, w - 400.0, 40.0));
    assert_eq!(rect(&tree, r.filters), (400.0, 40.0, 202.0, h - 40.0));
    assert_eq!(rect(&tree, r.library), (602.0, 40.0, w - 602.0, h - 40.0));
    assert!(handles.ctx.shutdown());
}

#[test]
fn the_panels_are_mounted_in_their_regions() {
    let (tree, _runtime, handles, r, app) = mounted();
    use crate::teksilo_ui::test_support::first_of_type;
    assert!(first_of_type(&tree, r.filters, "FiltersPanel").is_some());
    assert!(first_of_type(&tree, r.player, "PlayerPanel").is_some());
    assert!(first_of_type(&tree, r.library, "LibraryPanel").is_some());
    assert!(first_of_type(&tree, r.library, "MeterSlot").is_some());
    assert!(first_of_type(&tree, r.playlist, "PlaylistPanel").is_some());
    assert!(first_of_type(&tree, app, "AudioCenterPanel").is_some());
    assert!(handles.ctx.shutdown());
}

#[test]
fn building_the_root_restores_the_session_once() {
    let (mut tree, runtime, handles, _, _) = mounted();
    let status = handles.session.status();
    assert!(run_until(&runtime, TIMEOUT, || status.get().is_settled()));
    assert!(matches!(status.get(), RestoreStatus::Restored(_)));
    // A relayout does not start a second restore (the facade would refuse it).
    layout(&mut tree);
    runtime.tick();
    assert!(matches!(status.get(), RestoreStatus::Restored(_)));
    assert!(handles.ctx.shutdown());
}

// ---- what spans the panels: Ctrl+A, the reveal, the purge ----------------

use teksilo::core::{Key, Modifiers, WidgetEvent};

use crate::frontend::Event;
use crate::frontend::events::{MaintenanceEvent, PurgeReason};
use crate::teksilo_ui::mock_data::screenshot_library;
use crate::teksilo_ui::test_support::{EventPump, find_by_name, key_down_with, tree_with_pump};

/// The whole app over the mock library, restored, with the playlist's rows,
/// the library and the filter tree loaded.
struct Live {
    tree: WidgetTree,
    runtime: AsyncRuntimeHandle,
    pump: EventPump,
    regions: Regions,
    views: AppViewModels,
}

impl Live {
    fn new() -> Self {
        let ctx = AppContext::in_memory().unwrap();
        screenshot_library()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        let handles = AppHandles::new(ctx.clone());
        let (mut tree, runtime, pump) = tree_with_pump(&ctx);
        let app = App::new(handles.clone(), None);
        let (regions, views) = (app.regions(), app.view_models());
        tree.add(app);
        layout(&mut tree);
        let views = views.borrow().clone().unwrap();
        let live = Self {
            tree,
            runtime,
            pump,
            regions: regions.get().unwrap(),
            views,
        };
        let (status, rows) = (handles.session.status(), live.views.playlist.rows());
        let songs = live.views.services.library_songs.snapshot();
        let filters = live.views.filters.clone();
        live.wait(|| {
            status.get().is_settled()
                && !rows.get().is_empty()
                && !songs.get().songs.is_empty()
                && !filters.visible_items().is_empty()
        });
        let mut live = live;
        layout(&mut live.tree);
        live
    }

    /// Runs the executor and delivers the facade's events until `done`.
    fn wait(&self, done: impl Fn() -> bool) {
        let delivered = run_until(&self.runtime, TIMEOUT, || {
            self.pump.deliver(&self.tree);
            done()
        });
        assert!(delivered, "timed out");
    }

    /// Ctrl+A (⌘A on macOS) where the focus is.
    fn select_all(&mut self) {
        self.tree.dispatch_event(WidgetEvent::KeyDown {
            key: Key::A,
            modifiers: Modifiers::COMMAND,
            text: None,
        });
        layout(&mut self.tree);
    }

    /// Ctrl+A after focusing `id`.
    fn select_all_in(&mut self, id: WidgetId) {
        key_down_with(&mut self.tree, id, Key::A, Modifiers::COMMAND);
    }

    fn playlist_selected(&self) -> usize {
        self.views.playlist.selection().get().selected.len()
    }

    fn library_selected(&self) -> usize {
        self.views.library.selected_songs().len()
    }

    fn library_songs(&self) -> usize {
        self.views
            .services
            .library_songs
            .snapshot()
            .get()
            .songs
            .len()
    }

    /// The first node under `root` whose type name contains `part`.
    fn find_type(&self, root: WidgetId, part: &str) -> WidgetId {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self
                .tree
                .widget_type_name(id)
                .is_some_and(|name| name.contains(part))
            {
                return id;
            }
            let mut children = self.tree.children(id);
            children.reverse();
            stack.extend(children);
        }
        panic!("no {part} under {root:?}");
    }

    fn finish(self) {
        assert!(self.views.services.ctx.shutdown());
    }
}

#[test]
fn ctrl_a_with_nothing_focused_selects_the_whole_library() {
    // Iced's `AppFocus` starts on the library (`app.rs:1111`).
    let mut live = Live::new();
    live.select_all();
    assert_eq!(live.library_selected(), live.library_songs());
    assert_eq!(live.playlist_selected(), 0);
    live.finish();
}

#[test]
fn ctrl_a_in_the_playlist_selects_every_row() {
    let mut live = Live::new();
    let list = live.views.playlist_list.get().unwrap();
    live.select_all_in(list);
    assert_eq!(
        live.playlist_selected(),
        live.views.playlist.rows().get().len()
    );
    assert_eq!(live.library_selected(), 0);
    live.finish();
}

#[test]
fn ctrl_a_in_the_library_selects_the_library_and_follows_the_focus() {
    let mut live = Live::new();
    let grid = live.find_type(live.regions.library, "GridView");
    live.select_all_in(grid);
    assert_eq!(live.library_selected(), live.library_songs());
    assert_eq!(live.playlist_selected(), 0);
    let list = live.views.playlist_list.get().unwrap();
    live.select_all_in(list);
    assert!(
        live.playlist_selected() > 0,
        "the focus moved to the playlist"
    );
    live.finish();
}

#[test]
fn ctrl_a_in_a_text_field_is_the_fields_own() {
    let mut live = Live::new();
    // The search field's inner field is the node a click focuses.
    let field = live.find_type(live.regions.library, "TextInputField");
    live.select_all_in(field);
    assert_eq!(live.library_selected(), 0);
    assert_eq!(live.playlist_selected(), 0);
    live.finish();
}

#[test]
fn ctrl_a_outside_the_two_panels_selects_nothing() {
    let mut live = Live::new();
    let selector = find_by_name(&live.tree, live.regions.filters, "Filter type")[0];
    live.select_all_in(selector);
    assert_eq!(live.library_selected(), 0);
    assert_eq!(live.playlist_selected(), 0);
    live.finish();
}

#[test]
fn a_reveal_that_clears_the_filters_clears_the_filter_columns_letter_and_node() {
    // `app.rs:6108-6110`: the reveal clears the library's search and filters,
    // and the column's letter and selected node with them.
    let live = Live::new();
    let filters = live.views.filters.clone();
    let rock = filters
        .tree_source()
        .rows()
        .into_iter()
        .find(|row| row.label == "Rock")
        .unwrap()
        .id;
    let outside = live
        .views
        .services
        .library_songs
        .snapshot()
        .get()
        .songs
        .iter()
        .find(|song| song.genre.as_deref() != Some("Rock"))
        .unwrap()
        .full_file_path
        .to_string();
    filters.select_subfilter(Some("R"));
    filters.select_node(&rock);
    let songs = live.views.services.library_songs.snapshot();
    live.wait(|| {
        let songs = songs.get();
        !songs.songs.is_empty()
            && songs
                .songs
                .iter()
                .all(|song| song.genre.as_deref() == Some("Rock"))
    });
    live.views.services.links.reveal_in_library.send(outside);
    assert_eq!(filters.subfilter().get(), None);
    assert_eq!(filters.selected_node().get(), None);
    assert!(filters.selection().selected_keys().is_empty());
    live.finish();
}

#[test]
fn a_memory_purge_drops_iceds_16_oldest_covers() {
    let live = Live::new();
    let covers = live.views.services.covers.clone();
    // Paths need no file: a failed decode is an entry too.
    let dir = std::env::temp_dir().join("audoxidy-app-purge");
    for index in 0..40 {
        let _ = covers.request(&format!("{}/{index}.png", dir.display()));
    }
    live.wait(|| covers.in_flight() == 0);
    let before = covers.len();
    let songs = live.library_songs();
    live.views
        .services
        .ctx
        .hub
        .publish(Event::Maintenance(MaintenanceEvent::PurgeRequested(
            PurgeReason::Timer,
        )));
    live.wait(|| covers.len() == before - 16);
    // Quirk fix 62: Iced also cuts an unfocused large library; never here.
    assert_eq!(live.library_songs(), songs);
    live.finish();
}
