use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::time::Duration;

use teksilo::core::WidgetTree;
use teksilo::widgets::{ToastInstallOptions, ToastRegistry};

use super::*;
use crate::frontend::events::{LongOpInfo, PlaybackEvent, PlaybackSnapshot};
use crate::frontend::test_fixtures::LibraryFixture;
use crate::logic::library_groups::{ArtistGroup, parse_album_composite_id};
use crate::teksilo_ui::app::services::testing::in_memory_services;
use crate::teksilo_ui::mock_data::BOWIE;
use crate::teksilo_ui::test_support::{
    drain_dialog_titles, press, run_until, tree_with_app_state, tree_with_async,
};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Services over the mock library, loaded, and a view-model over them.
fn loaded() -> (AppServices, LibraryViewModel) {
    let services = in_memory_services();
    services.library_songs.reload();
    let version = services.library_songs.version();
    assert!(run_until(&services.runtime, TIMEOUT, || version.get() >= 1));
    let vm = LibraryViewModel::with_low_resource(&services, false);
    (services, vm)
}

/// Waits until the library model answered its latest request.
fn settle(services: &AppServices) {
    let loading = services.library_songs.loading();
    assert!(run_until(&services.runtime, TIMEOUT, || !loading.get()));
    services.runtime.tick();
}

fn at(ms: u64) -> Instant {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *START.get_or_init(Instant::now) + Duration::from_millis(ms)
}

fn ctrl() -> Modifiers {
    Modifiers::CTRL
}

fn first_artist(vm: &LibraryViewModel) -> String {
    vm.structure().get().artists[0].artist_name.clone()
}

fn bowie(vm: &LibraryViewModel) -> ArtistGroup {
    vm.structure()
        .get()
        .groups
        .iter()
        .find(|g| g.name == BOWIE)
        .cloned()
        .unwrap()
}

/// The composite ids of the album runs of `artist`, in display order.
fn album_ids(vm: &LibraryViewModel, artist: &str) -> Vec<String> {
    let structure = vm.structure().get();
    let va = structure
        .artists
        .iter()
        .find(|a| a.artist_name == artist)
        .unwrap();
    va.albums
        .iter()
        .map(|r| album_composite_id(artist, &r.album_hash))
        .collect()
}

fn song(id: i64) -> LibraryListItem {
    LibraryListItem::Song(id)
}

fn list_mode(vm: &LibraryViewModel) {
    vm.set_view_mode(LibraryViewMode::SimpleList);
}

#[test]
fn the_view_mode_cycles_grid_detailed_thumbnail_simple() {
    let (services, vm) = loaded();
    assert_eq!(vm.view_mode().get(), LibraryViewMode::Grid);
    let mut seen = Vec::new();
    for _ in 0..4 {
        vm.cycle_view_mode();
        seen.push(vm.view_mode().get());
    }
    assert_eq!(
        seen,
        vec![
            LibraryViewMode::DetailedList,
            LibraryViewMode::ThumbnailList,
            LibraryViewMode::SimpleList,
            LibraryViewMode::Grid,
        ]
    );
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::Top)
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn changing_the_view_mode_closes_the_grid_detail_and_resets_the_offset() {
    let (services, vm) = loaded();
    let id = services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    vm.toggle_album(&id);
    assert_eq!(vm.expanded_album().get(), Some(id));
    vm.h_offset().set(40.0);
    vm.set_view_mode(LibraryViewMode::DetailedList);
    assert_eq!(vm.expanded_album().get(), None);
    assert_eq!(vm.h_offset().get(), 0.0);
    assert!(services.ctx.shutdown());
}

#[cfg(not(feature = "teksilo-mocks"))]
#[test]
fn g5_a_view_mode_change_retries_the_covers_that_failed() {
    // Iced's `ChangeLibraryViewMode` clears its negative cover cache
    // (`app.rs:3367`): a cover a scan wrote since shows in the new mode.
    let (services, vm) = loaded();
    let dir = std::env::temp_dir().join(format!("audoxidy-g5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("late.png");
    let path = file.to_string_lossy().into_owned();
    let covers = services.covers.clone();
    let idle =
        |covers: &crate::teksilo_ui::shared::cover_store::CoverStore| covers.in_flight() == 0;
    let cover = covers.request(&path);
    assert!(run_until(&services.runtime, TIMEOUT, || idle(&covers)));
    assert!(cover.get().is_none(), "no file yet");
    image::RgbaImage::from_pixel(10, 10, image::Rgba([1, 2, 3, 255]))
        .save(&file)
        .unwrap();
    let _ = covers.request(&path);
    assert!(run_until(&services.runtime, TIMEOUT, || idle(&covers)));
    assert!(cover.get().is_none(), "the failure is remembered");
    vm.set_view_mode(LibraryViewMode::DetailedList);
    let _ = covers.request(&path);
    assert!(run_until(&services.runtime, TIMEOUT, || cover
        .get()
        .is_some()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_column_widths_start_at_the_iced_defaults_and_store_a_resize() {
    let (services, vm) = loaded();
    assert_eq!(vm.column_widths().get(), default_widths());
    vm.set_column_width(SortColumn::Title, 300);
    assert_eq!(vm.column_widths().get()[&SortColumn::Title], 300);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_sort_reorders_the_rows_and_clearing_it_restores_the_database_order() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let database: Vec<i64> = services
        .library_songs
        .snapshot()
        .get()
        .songs
        .iter()
        .map(|s| s.id)
        .collect();
    let shown = |vm: &LibraryViewModel| -> Vec<i64> {
        vm.structure().get().songs.iter().map(|s| s.id).collect()
    };
    assert_eq!(shown(&vm), database);

    let by_title = SortState::default().after_header_click(SortColumn::Title);
    vm.set_sort(by_title);
    let sorted = shown(&vm);
    assert_ne!(sorted, database);
    let titles: Vec<String> = vm
        .structure()
        .get()
        .songs
        .iter()
        .map(|s| s.title.as_deref().unwrap_or_default().to_lowercase())
        .collect();
    let mut expected = titles.clone();
    expected.sort();
    assert_eq!(titles, expected);

    let descending = by_title.after_header_click(SortColumn::Title);
    let cleared = descending.after_header_click(SortColumn::Title);
    assert_eq!(cleared.column, None);
    vm.set_sort(descending);
    vm.set_sort(cleared);
    assert_eq!(shown(&vm), database, "the database order is back");
    assert!(services.ctx.shutdown());
}

#[test]
fn the_sort_orders_the_grid_too() {
    let (services, vm) = loaded();
    vm.set_sort(SortState::default().after_header_click(SortColumn::Album));
    let list = services.albums.list();
    let titles: Vec<String> = (0..list.len())
        .filter_map(|i| list.with_item(i, |c| c.title.to_lowercase()))
        .collect();
    let mut expected = titles.clone();
    expected.sort();
    assert_eq!(titles, expected);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_search_queries_the_library_and_scrolls_to_the_top() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let before = vm.structure().get().songs.len();
    vm.search().set("Starman".to_string());
    assert_eq!(
        services.library_songs.params().query.as_deref(),
        Some("Starman")
    );
    settle(&services);
    let after = vm.structure().get().songs.len();
    assert!(after > 0 && after < before, "{after} of {before}");
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::Top)
    );
    vm.search().set(String::new());
    settle(&services);
    assert_eq!(vm.structure().get().songs.len(), before);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_click_selects_ctrl_toggles_and_shift_extends_from_the_pivot() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let songs: Vec<i64> = bowie(&vm).songs.iter().map(|s| s.id).collect();
    let structure = vm.structure().get();
    let order: Vec<i64> = structure
        .items()
        .into_iter()
        .filter_map(|it| match it {
            LibraryListItem::Song(id) => Some(id),
            _ => None,
        })
        .filter(|id| songs.contains(id))
        .collect();
    let (a, b, c) = (order[0], order[1], order[3]);

    vm.click(song(a), Modifiers::NONE, at(0));
    assert_eq!(
        vm.selection().get().selected,
        [song(a)].into_iter().collect()
    );
    vm.click(song(c), ctrl(), at(1000));
    assert_eq!(
        vm.selection().get().selected,
        [song(a), song(c)].into_iter().collect()
    );
    vm.click(song(b), Modifiers::NONE, at(2000));
    vm.click(song(c), Modifiers::SHIFT, at(3000));
    let selected = vm.selection().get().selected;
    for id in &order[1..=3] {
        assert!(selected.contains(&song(*id)), "{id}");
    }
    assert!(!selected.contains(&song(a)));
    assert!(services.ctx.shutdown());
}

#[test]
fn a_double_click_on_a_header_toggles_it_and_a_slow_one_only_selects() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let artist = first_artist(&vm);
    let header = LibraryListItem::Artist(artist.clone());
    vm.click(header.clone(), Modifiers::NONE, at(0));
    vm.click(header.clone(), Modifiers::NONE, at(600));
    assert!(
        !vm.is_artist_collapsed(&artist),
        "600 ms apart is two clicks"
    );
    vm.click(header.clone(), Modifiers::NONE, at(900));
    assert!(vm.is_artist_collapsed(&artist), "300 ms apart toggles");
    assert_eq!(
        vm.selection().get().selected,
        [header].into_iter().collect()
    );

    let album = album_ids(&vm, BOWIE)[0].clone();
    let item = LibraryListItem::Album(album.clone());
    vm.click(item.clone(), Modifiers::NONE, at(2000));
    vm.click(item, Modifiers::NONE, at(2200));
    assert!(vm.is_album_collapsed(&album));
    assert!(services.ctx.shutdown());
}

#[test]
fn collapsing_an_artist_hides_its_rows_and_selects_its_header() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let rows_before = vm.structure().get().rows.len();
    let songs = bowie(&vm).songs.len();
    vm.toggle_artist(BOWIE);
    let structure = vm.structure().get();
    assert!(structure.rows.len() < rows_before);
    assert!(!structure.items().iter().any(
        |it| matches!(it, LibraryListItem::Song(id) if bowie(&vm).songs.iter().any(|s| s.id == *id))
    ));
    assert_eq!(
        vm.selection().get().focused,
        Some(LibraryListItem::Artist(BOWIE.into()))
    );
    // The collapsed header stands for every song of the artist.
    assert_eq!(vm.selected_songs().len(), songs);
    vm.toggle_artist(BOWIE);
    assert_eq!(vm.structure().get().rows.len(), rows_before);
    assert!(services.ctx.shutdown());
}

#[test]
fn up_and_down_move_the_selection_and_shift_extends_it() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let items = vm.structure().get().items();
    vm.click(items[3].clone(), Modifiers::NONE, at(0));
    assert!(vm.navigate(LibraryNavDir::Down, Modifiers::NONE));
    assert_eq!(vm.selection().get().focused, Some(items[4].clone()));
    assert!(matches!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::Reveal {
            force_top: false,
            ..
        })
    ));
    vm.navigate(LibraryNavDir::Up, Modifiers::NONE);
    vm.navigate(LibraryNavDir::Up, Modifiers::NONE);
    assert_eq!(vm.selection().get().focused, Some(items[2].clone()));
    vm.navigate(LibraryNavDir::Down, Modifiers::SHIFT);
    vm.navigate(LibraryNavDir::Down, Modifiers::SHIFT);
    let selection = vm.selection().get();
    assert_eq!(selection.focused, Some(items[4].clone()));
    for item in &items[2..=4] {
        assert!(selection.selected.contains(item), "{item:?}");
    }
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_37_left_collapses_every_selected_album_and_keeps_them_all_selected() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let structure = vm.structure().get();
    let va = structure
        .artists
        .iter()
        .find(|a| a.artist_name == BOWIE)
        .unwrap();
    let first = va.albums[0].songs[0].0.id;
    let second = va.albums[1].songs[0].0.id;
    vm.click(song(first), Modifiers::NONE, at(0));
    vm.click(song(second), ctrl(), at(1000));
    assert!(vm.navigate(LibraryNavDir::Left, Modifiers::NONE));
    let albums = album_ids(&vm, BOWIE);
    assert!(vm.is_album_collapsed(&albums[0]) && vm.is_album_collapsed(&albums[1]));
    let selection = vm.selection().get();
    assert_eq!(
        selection.selected,
        [
            LibraryListItem::Album(albums[0].clone()),
            LibraryListItem::Album(albums[1].clone())
        ]
        .into_iter()
        .collect(),
        "both albums stay selected"
    );
    assert_eq!(
        selection.focused,
        Some(LibraryListItem::Album(albums[0].clone()))
    );
    // Right expands both again, from that selection.
    assert!(vm.navigate(LibraryNavDir::Right, Modifiers::NONE));
    assert!(!vm.is_album_collapsed(&albums[0]) && !vm.is_album_collapsed(&albums[1]));
    assert!(!vm.navigate(LibraryNavDir::Right, Modifiers::NONE));
    assert!(services.ctx.shutdown());
}

#[test]
fn select_all_selects_every_row_and_the_grid_selects_every_album() {
    let (services, vm) = loaded();
    list_mode(&vm);
    vm.select_all();
    assert_eq!(
        vm.selection().get().selected.len(),
        vm.structure().get().items().len()
    );
    assert_eq!(
        vm.selected_songs().len(),
        services.library_songs.snapshot().get().songs.len()
    );
    vm.set_view_mode(LibraryViewMode::Grid);
    vm.deselect();
    vm.select_all();
    assert_eq!(vm.grid_selection().count(), services.albums.list().len());
    assert!(services.ctx.shutdown());
}

#[test]
fn the_marquee_selects_the_rows_it_crosses() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let rects = vm.structure().get().item_rects(500.0);
    let rect = ContentRect::new(10.0, rects[2].1.y + 1.0, 50.0, rects[2].1.height * 2.0);
    vm.select_marquee(rect, 500.0, Modifiers::NONE);
    let selected = vm.selection().get().selected;
    assert_eq!(selected.len(), 3);
    for (item, _) in &rects[2..5] {
        assert!(selected.contains(item));
    }
    assert!(services.ctx.shutdown());
}

#[test]
fn the_stats_show_the_selection_then_the_results_then_the_totals() {
    let services = in_memory_services();
    services.library_stats.reload();
    let loaded_totals = services.library_stats.loaded();
    assert!(run_until(&services.runtime, TIMEOUT, || loaded_totals.get()));
    let vm = LibraryViewModel::with_low_resource(&services, false);
    assert_eq!(
        vm.stats().get(),
        services.library_stats.stats().get(),
        "before the first load: the totals"
    );

    services.library_songs.reload();
    settle(&services);
    let all = services.library_songs.snapshot().get().songs.clone();
    assert_eq!(vm.stats().get(), compute_stats(&all), "the results");

    list_mode(&vm);
    let picked = bowie(&vm).songs[0].clone();
    vm.click(song(picked.id), Modifiers::NONE, at(0));
    assert_eq!(vm.stats().get().songs, 1, "the selection");
    assert_eq!(vm.stats().get(), compute_stats(&[picked]));

    vm.deselect();
    vm.search().set("Starman".to_string());
    settle(&services);
    let found = services.library_songs.snapshot().get().songs.clone();
    assert_eq!(
        vm.stats().get(),
        compute_stats(&found),
        "the search results"
    );
    assert!(services.ctx.shutdown());
}

/// Services whose session is restored, so the active playlist is known.
fn restored() -> (AppServices, LibraryViewModel) {
    let (services, vm) = loaded();
    services
        .session
        .start_restore(&services.ctx, &services.runtime);
    let id = services.active_playlist.id();
    assert!(run_until(&services.runtime, TIMEOUT, || id.get().is_some()));
    (services, vm)
}

fn playlist_song_ids(services: &AppServices) -> Vec<i64> {
    let id = services.active_playlist.id().get().unwrap();
    playlist::load_playlist(&services.ctx, id)
        .recv_blocking()
        .unwrap()
        .groups
        .iter()
        .flat_map(|g| g.songs.iter().map(|s| s.song_id))
        .collect()
}

#[test]
fn quirk_2_play_appends_the_library_selection_to_the_active_playlist() {
    let (services, vm) = restored();
    list_mode(&vm);
    let before = playlist_song_ids(&services);
    let album = album_ids(&vm, BOWIE)[3].clone();
    vm.click(LibraryListItem::Album(album), Modifiers::NONE, at(0));
    let wanted: Vec<i64> = vm.selected_songs().iter().map(|s| s.id).collect();
    assert!(!wanted.is_empty());
    assert!(vm.play_selection());
    assert!(run_until(&services.runtime, TIMEOUT, || {
        let now = playlist_song_ids(&services);
        wanted.iter().all(|id| now.contains(id))
    }));
    let after = playlist_song_ids(&services);
    assert_eq!(
        &after[..before.len()],
        &before[..],
        "appended after the rows"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn play_without_a_selection_or_an_active_playlist_does_nothing() {
    let (services, vm) = loaded();
    assert!(!vm.play_selection(), "no selection");
    list_mode(&vm);
    vm.click(song(bowie(&vm).songs[0].id), Modifiers::NONE, at(0));
    assert!(
        !vm.play_selection(),
        "no active playlist before the restore"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_49_delete_takes_only_the_songs_picked_inside_a_selected_album_as_play_does() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let structure = vm.structure().get();
    let va = structure
        .artists
        .iter()
        .find(|a| a.artist_name == BOWIE)
        .unwrap();
    let album = &va.albums[0];
    let id = album_composite_id(BOWIE, &album.album_hash);
    let lone = va.albums[1].songs[0].0.id;
    let picked = album.songs[0].0.id;
    let selected: HashSet<LibraryListItem> =
        [LibraryListItem::Album(id.clone()), song(picked), song(lone)]
            .into_iter()
            .collect();
    let mut expected = vec![picked, lone];
    expected.sort_unstable();
    assert_eq!(
        delete_target_ids(&selected, &structure.data(), &structure.songs),
        expected
    );
    let played: Vec<i64> = selected_songs(&selected, &structure.data(), &structure.songs)
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(played.len(), 2, "Play takes the same songs");
    // With nothing picked inside it, the whole album goes.
    let whole: HashSet<LibraryListItem> = [LibraryListItem::Album(id), song(lone)]
        .into_iter()
        .collect();
    let mut expected: Vec<i64> = album.songs.iter().map(|(s, _)| s.id).collect();
    expected.push(lone);
    expected.sort_unstable();
    assert_eq!(
        delete_target_ids(&whole, &structure.data(), &structure.songs),
        expected
    );
    // A selected artist narrows the same way.
    let artist: HashSet<LibraryListItem> =
        [LibraryListItem::Artist(BOWIE.to_string()), song(picked)]
            .into_iter()
            .collect();
    assert_eq!(
        delete_target_ids(&artist, &structure.data(), &structure.songs),
        vec![picked]
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn delete_leaves_the_selected_songs_the_search_hides_as_iced_and_play_do() {
    // Review #2: Iced deletes among `filtered_songs` only
    // (`src/gui/app.rs:3759`); the selection survives a search (`:2797`).
    let (services, vm) = loaded();
    list_mode(&vm);
    let hidden = bowie(&vm).songs[0].id;
    vm.click(song(hidden), Modifiers::NONE, at(0));
    vm.search().set("Placebo".to_string());
    settle(&services);
    let visible = vm.structure().get().songs[0].id;
    assert!(!vm.structure().get().songs.iter().any(|s| s.id == hidden));
    vm.click(song(visible), ctrl(), at(5_000));
    assert!(vm.selection().get().selected.contains(&song(hidden)));
    let played: Vec<i64> = vm.selected_songs().iter().map(|s| s.id).collect();
    assert_eq!(played, vec![visible]);
    assert_eq!(vm.delete_targets(), vec![visible], "the hidden song stays");
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_49_the_confirmation_states_how_many_songs_go() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let structure = vm.structure().get();
    let va = structure
        .artists
        .iter()
        .find(|a| a.artist_name == BOWIE)
        .unwrap();
    let album = album_composite_id(BOWIE, &va.albums[0].album_hash);
    vm.click(LibraryListItem::Album(album), Modifiers::NONE, at(0));
    vm.click(song(va.albums[0].songs[1].0.id), ctrl(), at(5_000));
    assert_eq!(vm.delete_targets().len(), 1);
    let text =
        |count| crate::teksilo_ui::library::test_util::plain(&delete_text(count).resolve_now());
    assert!(
        text(1).contains("1 song") && !text(1).contains("1 songs"),
        "{}",
        text(1)
    );
    assert!(text(12).contains("12 songs"), "{}", text(12));
    assert!(
        text(1_234).contains("1234 songs"),
        "no grouping: {}",
        text(1_234)
    );
    assert!(services.ctx.shutdown());
}

/// The delete confirmation `request_delete` presented, its content built
/// into `tree`: the layer and the callback Escape and a click outside run.
fn open_confirmation(
    tree: &mut WidgetTree,
) -> (
    WidgetId,
    Option<teksilo::core::overlay::OverlayDismissCallback>,
) {
    use teksilo::core::modal::ModalContent;
    let queued = tree
        .drain_pending_modal_requests()
        .pop()
        .expect("the confirmation was presented");
    assert_eq!(
        queued.request.title.as_deref(),
        Some("Delete from the library?")
    );
    let on_dismiss = queued.request.on_dismiss.clone();
    let layer = match queued.request.content {
        ModalContent::Deferred(build) => build(tree),
        ModalContent::ExistingWidget(id) => id,
    };
    crate::teksilo_ui::test_support::layout(tree);
    let _ = tree.render();
    (layer, on_dismiss)
}

/// Whether the library still holds the song `id`, after a reload.
fn still_there(services: &AppServices, id: i64) -> bool {
    services.library_songs.reload();
    settle(services);
    services
        .library_songs
        .snapshot()
        .get()
        .songs
        .iter()
        .any(|s| s.id == id)
}

#[test]
fn quirk_1_only_the_confirmations_eliminar_deletes() {
    use crate::teksilo_ui::test_support::{find_by_name, tap_at};
    let (services, vm) = loaded();
    list_mode(&vm);
    let (mut tree, _) = tree_with_async(&services.ctx);
    let victim = bowie(&vm).songs[0].clone();
    vm.click(song(victim.id), Modifiers::NONE, at(0));
    let ask = |tree: &mut WidgetTree| {
        let asked = vm.clone();
        press(tree, move |ctx| asked.request_delete(ctx));
        open_confirmation(tree)
    };
    let cancel = tr!(chrome_dialog_cancel()).resolve_now();

    // "Cancelar", with the pointer.
    let (layer, _) = ask(&mut tree);
    let button = find_by_name(&tree, layer, &cancel)[0];
    let center = tree.bounds(button).center();
    tap_at(&mut tree, center);
    assert!(!vm.is_deleting());
    assert!(
        still_there(&services, victim.id),
        "Cancelar deletes nothing"
    );

    // Escape (the framework runs the modal's dismissal).
    let (_, on_dismiss) = ask(&mut tree);
    let dismiss = on_dismiss.expect("a dismissal callback");
    press(&mut tree, move |ctx| {
        dismiss(teksilo::core::overlay::DismissReason::Escape, ctx)
    });
    assert!(!vm.is_deleting());
    assert!(still_there(&services, victim.id), "Escape deletes nothing");

    // "Eliminar", with the pointer.
    let (layer, _) = ask(&mut tree);
    let button = find_by_name(&tree, layer, "Delete")[0];
    let center = tree.bounds(button).center();
    tap_at(&mut tree, center);
    assert!(vm.is_deleting(), "Eliminar starts the delete");
    assert!(run_until(&services.runtime, TIMEOUT, || !still_there(
        &services, victim.id
    )));
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_1_nothing_selected_asks_nothing() {
    let (services, vm) = loaded();
    let (mut tree, _) = tree_with_async(&services.ctx);
    let asked = vm.clone();
    press(&mut tree, move |ctx| asked.request_delete(ctx));
    assert!(drain_dialog_titles(&mut tree).is_empty());
    assert!(services.ctx.shutdown());
}

/// A tree with a toast registry, so `show_toast` lands somewhere a test sees.
fn tree_with_toasts(services: &AppServices) -> (WidgetTree, ToastRegistry) {
    let registry = ToastRegistry::new(ToastInstallOptions {
        archive: None,
        ..ToastInstallOptions::default()
    });
    let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
    state.insert(TypeId::of::<ToastRegistry>(), Box::new(registry.clone()));
    (tree_with_app_state(&services.ctx, state), registry)
}

fn long_op(op: OpId, cancelled: bool) -> LongOpInfo {
    LongOpInfo {
        op_id: op,
        kind: LongOpKind::LibraryDelete,
        progress: None,
        error: Some("disk full".into()),
        cancelled,
    }
}

#[test]
fn quirk_8_a_failed_delete_shows_an_error_toast_and_a_cancelled_one_does_not() {
    let (services, vm) = loaded();
    let (mut tree, registry) = tree_with_toasts(&services);
    let op = vm.delete_songs(Vec::new());
    let failed = Event::LongOperation(LongOpEvent::Failed(long_op(op, false)));
    let handler = vm.clone();
    press(&mut tree, move |ctx| handler.on_long_op(&failed, ctx));
    assert_eq!(registry.live_count(), 1);
    assert!(!vm.is_deleting());

    let other = vm.delete_songs(Vec::new());
    let cancelled = Event::LongOperation(LongOpEvent::Failed(long_op(other, true)));
    let handler = vm.clone();
    press(&mut tree, move |ctx| handler.on_long_op(&cancelled, ctx));
    assert_eq!(registry.live_count(), 1, "a cancellation is not an error");

    // An operation this panel did not start is not its business.
    let foreign = Event::LongOperation(LongOpEvent::Failed(long_op(OpId(9_999), false)));
    let handler = vm.clone();
    press(&mut tree, move |ctx| handler.on_long_op(&foreign, ctx));
    assert_eq!(registry.live_count(), 1);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_completed_delete_clears_the_selection() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let (mut tree, _registry) = tree_with_toasts(&services);
    vm.click(song(bowie(&vm).songs[0].id), Modifiers::NONE, at(0));
    let op = vm.delete_songs(Vec::new());
    let mut info = long_op(op, false);
    info.error = None;
    let done = Event::LongOperation(LongOpEvent::Completed(info));
    let handler = vm.clone();
    press(&mut tree, move |ctx| handler.on_long_op(&done, ctx));
    assert!(vm.selection().get().selected.is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_8_a_folder_that_cannot_be_scanned_shows_an_error_toast() {
    let (services, vm) = loaded();
    let (mut tree, registry) = tree_with_toasts(&services);
    let handler = vm.clone();
    press(&mut tree, move |ctx| handler.scan_folder("  ", ctx));
    assert_eq!(registry.live_count(), 1);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_reveal_by_path_opens_the_collapsed_artist_and_selects_the_song() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let target = bowie(&vm).songs[5].clone();
    vm.toggle_artist(BOWIE);
    assert!(vm.is_artist_collapsed(BOWIE));
    services
        .links
        .reveal_in_library
        .send(target.full_file_path.to_string());
    assert!(!vm.is_artist_collapsed(BOWIE));
    let selection = vm.selection().get();
    assert_eq!(selection.selected, [song(target.id)].into_iter().collect());
    assert_eq!(selection.focused, Some(song(target.id)));
    let (y, height) = vm.structure().get().item_y_range(&song(target.id)).unwrap();
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::Reveal {
            item: song(target.id),
            y,
            height,
            force_top: true
        })
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_reveal_in_detailed_mode_opens_the_collapsed_album() {
    let (services, vm) = loaded();
    vm.set_view_mode(LibraryViewMode::DetailedList);
    let structure = vm.structure().get();
    let va = structure
        .artists
        .iter()
        .find(|a| a.artist_name == BOWIE)
        .unwrap();
    let run = &va.albums[2];
    let target = run.songs[0].0.clone();
    let album = album_composite_id(BOWIE, &run.album_hash);
    vm.toggle_album(&album);
    assert!(vm.is_album_collapsed(&album));
    vm.reveal_path(&target.full_file_path);
    assert!(!vm.is_album_collapsed(&album));
    assert!(vm.selection().get().selected.contains(&song(target.id)));
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_51_a_reveal_in_the_simple_and_thumbnail_modes_opens_the_collapsed_album() {
    for mode in [LibraryViewMode::SimpleList, LibraryViewMode::ThumbnailList] {
        let (services, vm) = loaded();
        vm.set_view_mode(mode);
        let structure = vm.structure().get();
        let va = structure
            .artists
            .iter()
            .find(|a| a.artist_name == BOWIE)
            .unwrap();
        let run = &va.albums[2];
        let target = run.songs[0].0.clone();
        let album = album_composite_id(BOWIE, &run.album_hash);
        vm.toggle_album(&album);
        assert!(vm.is_album_collapsed(&album));
        assert!(!vm.structure().get().items().contains(&song(target.id)));
        vm.reveal_path(&target.full_file_path);
        assert!(!vm.is_album_collapsed(&album), "{mode:?}");
        assert!(
            vm.structure().get().items().contains(&song(target.id)),
            "{mode:?}: the song is shown"
        );
        assert!(vm.selection().get().selected.contains(&song(target.id)));
        assert!(services.ctx.shutdown());
    }
}

#[test]
fn quirk_52_with_nothing_focused_down_and_up_select_the_first_row() {
    for dir in [LibraryNavDir::Down, LibraryNavDir::Up] {
        let (services, vm) = loaded();
        list_mode(&vm);
        assert_eq!(vm.selection().get().focused, None);
        assert!(vm.navigate(dir, Modifiers::NONE));
        let first = vm.structure().get().items()[0].clone();
        let selection = vm.selection().get();
        assert_eq!(selection.selected, [first.clone()].into_iter().collect());
        assert_eq!(selection.focused, Some(first));
        // From there Down moves on as before.
        assert!(vm.navigate(LibraryNavDir::Down, Modifiers::NONE));
        let second = vm.structure().get().items()[1].clone();
        assert_eq!(vm.selection().get().focused, Some(second));
        assert!(services.ctx.shutdown());
    }
}

#[test]
fn a_reveal_that_clears_the_search_and_the_filters_says_what_it_cleared() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let target = bowie(&vm).songs[0].clone();
    vm.search().set("Placebo".to_string());
    services
        .library_songs
        .update_params(|p| p.folder_id = Some(-1));
    settle(&services);
    assert!(vm.reveal_cleared().get().is_none());
    vm.reveal_path(&target.full_file_path);
    let (_, cleared) = vm.reveal_cleared().get().unwrap();
    assert_eq!(cleared.query.as_deref(), Some("Placebo"));
    assert_eq!(cleared.folder_id, Some(-1));
    assert_eq!(
        (cleared.artist, cleared.album, cleared.genre, cleared.year),
        (None, None, None, None)
    );
    let now = services.library_songs.params();
    assert!(now.query.is_none() && !has_any_filter(&now));
    settle(&services);
    assert!(vm.selection().get().selected.contains(&song(target.id)));
    // A reveal of a song the results show clears nothing.
    let sent = vm.reveal_cleared().get().map(|(n, _)| n);
    vm.reveal_path(&bowie(&vm).songs[1].full_file_path);
    assert_eq!(vm.reveal_cleared().get().map(|(n, _)| n), sent);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_reveal_clears_the_search_that_hides_the_song_and_resumes_after_the_reload() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let target = bowie(&vm).songs[0].clone();
    vm.search().set("Placebo".to_string());
    settle(&services);
    assert!(!vm.structure().get().songs.iter().any(|s| s.id == target.id));
    vm.reveal_path(&target.full_file_path);
    assert_eq!(vm.search().get(), "");
    settle(&services);
    assert!(vm.selection().get().selected.contains(&song(target.id)));
    assert!(services.ctx.shutdown());
}

#[test]
fn show_playing_reveals_the_playing_track() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let target = bowie(&vm).songs[2].clone();
    let snapshot = PlaybackSnapshot {
        path: target.full_file_path.to_string(),
        ..PlaybackSnapshot::from_state(&crate::audio::engine::AudioState::default())
    };
    services
        .now_playing
        .on_event(&Event::Playback(PlaybackEvent::TrackChanged(Arc::new(
            snapshot,
        ))));
    vm.show_playing();
    assert_eq!(vm.selection().get().focused, Some(song(target.id)));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_grid_has_two_to_ten_columns_and_five_at_most_in_low_resource_mode() {
    assert_eq!(grid_columns(100.0, false), 2);
    assert_eq!(grid_columns(373.0, false), 2);
    assert_eq!(grid_columns(374.0, false), 2);
    assert_eq!(grid_columns(561.0, false), 3);
    assert_eq!(grid_columns(758.0, false), 4);
    assert_eq!(grid_columns(1870.0, false), 10);
    assert_eq!(grid_columns(5000.0, false), 10);
    assert_eq!(grid_columns(5000.0, true), 5);
    assert_eq!(grid_columns(800.0, true), 4);
    assert_eq!(grid_columns(f32::NAN, false), 2);
}

/// The first grid card and its view-model, in Grid mode.
fn first_card(services: &AppServices) -> String {
    services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap()
}

#[test]
fn toggling_a_tile_opens_its_detail_and_selects_it() {
    let (services, vm) = loaded();
    let id = first_card(&services);
    vm.toggle_album(&id);
    assert_eq!(vm.expanded_album().get(), Some(id.clone()));
    assert!(!vm.expanded_songs().get().is_empty());
    assert_eq!(vm.grid_selection().selected_keys(), vec![id.clone()]);
    assert_eq!(vm.grid_anchor(), Some(id.clone()));
    let songs = vm.expanded_songs().get();
    assert_eq!(vm.stats().get(), compute_stats(&songs));
    vm.toggle_album(&id);
    assert_eq!(vm.expanded_album().get(), None);
    assert!(vm.expanded_songs().get().is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_42_clearing_the_sort_keeps_the_expanded_album_and_restores_its_order() {
    let (services, vm) = loaded();
    let id = first_card(&services);
    vm.toggle_album(&id);
    let canonical: Vec<i64> = vm.expanded_songs().get().iter().map(|s| s.id).collect();
    let by_title = SortState::default().after_header_click(SortColumn::Title);
    let descending = by_title.after_header_click(SortColumn::Title);
    vm.set_sort(by_title);
    vm.set_sort(descending);
    let reversed: Vec<i64> = vm.expanded_songs().get().iter().map(|s| s.id).collect();
    assert_eq!(reversed.len(), canonical.len());
    vm.set_sort(descending.after_header_click(SortColumn::Title));
    assert_eq!(vm.sort().get().column, None);
    let cleared: Vec<i64> = vm.expanded_songs().get().iter().map(|s| s.id).collect();
    assert!(!cleared.is_empty(), "D22: the album is not emptied");
    assert_eq!(cleared, canonical);
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_42_an_album_hidden_by_the_search_loads_from_its_parsed_artist_and_hash() {
    let (services, vm) = loaded();
    let id = first_card(&services);
    let (_, hash) = parse_album_composite_id(&id).unwrap();
    vm.search().set("Placebo".to_string());
    settle(&services);
    let snapshot = services.library_songs.snapshot().get();
    assert_eq!(
        album_songs_in_memory(&id, &snapshot.songs, &snapshot.albums).map(|s| s.len()),
        Some(0),
        "the results hold none of its songs"
    );
    vm.toggle_album(&id);
    let songs = vm.expanded_songs();
    assert!(run_until(&services.runtime, TIMEOUT, || !songs
        .get()
        .is_empty()));
    let ids: HashSet<i64> = songs.get().iter().map(|s| s.id).collect();
    let in_db: HashSet<i64> = library::album_songs_by_composite_id(&services.ctx, &id)
        .recv_blocking()
        .unwrap()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, in_db);
    assert!(!hash.is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_new_search_keeps_the_expanded_songs_it_finds_and_closes_the_album_without_any() {
    let (services, vm) = loaded();
    let list = services.albums.list();
    let starman = (0..list.len())
        .find_map(|i| {
            list.with_item(i, |c| (c.title == "Starman").then(|| c.id.clone()))
                .flatten()
        })
        .unwrap();
    vm.toggle_album(&starman);
    vm.search().set("Starman".to_string());
    settle(&services);
    assert_eq!(vm.expanded_album().get(), Some(starman.clone()));
    vm.search().set("Placebo".to_string());
    settle(&services);
    assert_eq!(vm.expanded_album().get(), None, "Iced closes it");
    assert!(services.ctx.shutdown());
}

/// Services over the demo library (a compilation, "Summer Mix" by Various
/// Artists, whose tracks have two other artists), loaded, a view-model over
/// them, and the compilation's card id.
fn demo() -> (AppServices, LibraryViewModel, String) {
    let services = {
        let ctx = crate::frontend::AppContext::in_memory().unwrap();
        LibraryFixture::demo()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        AppServices::new(
            &crate::teksilo_ui::shell::main_window::AppHandles::new(ctx),
            AsyncRuntimeHandle::new(),
        )
    };
    services.library_songs.reload();
    settle(&services);
    let vm = LibraryViewModel::with_low_resource(&services, false);
    let list = services.albums.list();
    let summer = (0..list.len())
        .find_map(|i| {
            list.with_item(i, |c| (c.title == "Summer Mix").then(|| c.id.clone()))
                .flatten()
        })
        .unwrap();
    (services, vm, summer)
}

#[test]
fn a_compilation_opens_with_the_songs_of_its_album_artist() {
    let (services, vm, summer) = demo();
    vm.toggle_album(&summer);
    assert_eq!(vm.expanded_songs().get().len(), 2);
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_54_a_compilation_hidden_by_the_search_loads_from_its_album_artist() {
    let (services, vm, summer) = demo();
    vm.search().set("Night Signals".to_string());
    settle(&services);
    let snapshot = services.library_songs.snapshot().get();
    assert_eq!(
        album_songs_in_memory(&summer, &snapshot.songs, &snapshot.albums).map(|s| s.len()),
        Some(0),
        "the results hold none of its songs"
    );
    vm.toggle_album(&summer);
    let songs = vm.expanded_songs();
    assert!(
        run_until(&services.runtime, TIMEOUT, || songs.get().len() == 2),
        "the database fallback finds the compilation: {}",
        songs.get().len()
    );
    assert!(
        songs
            .get()
            .iter()
            .all(|s| s.album.as_deref() == Some("Summer Mix"))
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_50_a_grid_reveal_finds_a_compilation_track_s_album() {
    let (services, vm, summer) = demo();
    let snapshot = services.library_songs.snapshot().get();
    let track = snapshot
        .songs
        .iter()
        .find(|s| s.title.as_deref() == Some("Sol y Sombra"))
        .cloned()
        .unwrap();
    assert_ne!(track.artist.as_deref(), Some("Various Artists"));
    vm.reveal_path(&track.full_file_path);
    assert_eq!(vm.expanded_album().get(), Some(summer));
    assert_eq!(
        vm.selection().get().selected,
        [song(track.id)].into_iter().collect()
    );
    assert!(matches!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::GridSong { .. })
    ));
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_53_the_open_album_s_songs_match_the_search_whatever_its_case() {
    let (services, vm) = loaded();
    let id = first_card(&services);
    vm.toggle_album(&id);
    let songs = vm.expanded_songs().get();
    let title = songs[0].title.as_deref().unwrap_or_default().to_string();
    let lower = visible_album_songs(&songs, &title.to_lowercase());
    let upper = visible_album_songs(&songs, &format!("  {}  ", title.to_uppercase()));
    assert!(!lower.is_empty());
    let ids = |found: &[Arc<SongData>]| found.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(ids(&upper), ids(&lower));
    assert_eq!(visible_album_songs(&songs, "   ").len(), songs.len());
    assert!(services.ctx.shutdown());
}

#[test]
fn alt_down_opens_and_alt_up_closes_the_selected_tile_and_up_closes_the_open_one() {
    let (services, vm) = loaded();
    let id = first_card(&services);
    vm.grid_selection().select(id.clone());
    assert_eq!(vm.grid_anchor(), Some(id.clone()));
    let key = |key: Key, modifiers: Modifiers| vm.grid_key(&key, modifiers, false);
    assert_eq!(
        key(Key::ArrowUp, Modifiers::ALT),
        GridKey::Unused,
        "nothing to close: a step up, the grid's"
    );
    assert_eq!(key(Key::ArrowDown, Modifiers::ALT), GridKey::Used);
    assert_eq!(vm.expanded_album().get(), Some(id.clone()));
    assert_eq!(key(Key::ArrowUp, Modifiers::ALT), GridKey::Used);
    assert_eq!(vm.expanded_album().get(), None);
    vm.toggle_album(&id);
    assert_eq!(key(Key::ArrowUp, Modifiers::NONE), GridKey::Used);
    assert_eq!(vm.expanded_album().get(), None);
    assert_eq!(key(Key::ArrowLeft, Modifiers::ALT), GridKey::Unused);
    // Alt+↓ on the open album walks into its songs, as Iced's
    // `handle_key_nav` does once the toggle does not apply.
    vm.toggle_album(&id);
    assert_eq!(key(Key::ArrowDown, Modifiers::ALT), GridKey::EnteredBand);
    assert_eq!(vm.expanded_album().get(), Some(id));
    assert!(services.ctx.shutdown());
}

/// A press on the grid's card `index` with `modifiers`, as the grid reports
/// it before its own selection change ([`LibraryViewModel::grid_press`]).
fn press_card(vm: &LibraryViewModel, index: usize, modifiers: Modifiers) {
    vm.grid_press((0.5, 0.5), modifiers, |spot, _| {
        if spot == GridSpot::Card(index) {
            ContentRect::new(0.0, 0.0, 1.0, 1.0)
        } else {
            ContentRect::new(-10.0, -10.0, 1.0, 1.0)
        }
    });
}

#[test]
fn a_tile_click_replaces_the_detail_songs_and_a_ctrl_toggle_keeps_them() {
    let (services, vm) = loaded();
    let list = services.albums.list();
    let (a, b) = (
        list.with_item(0, |c| c.id.clone()).unwrap(),
        list.with_item(1, |c| c.id.clone()).unwrap(),
    );
    vm.toggle_album(&a);
    let first = vm.expanded_songs().get()[0].id;
    vm.click(song(first), Modifiers::NONE, at(0));
    assert_eq!(
        vm.grid_selection().count(),
        0,
        "a plain song click clears the tiles"
    );
    // The grid applies each click its own way; Iced's applies instead.
    press_card(&vm, 1, Modifiers::NONE);
    vm.grid_selection().toggle(b.clone());
    assert!(
        vm.selection().get().selected.is_empty(),
        "from no tile to one: a plain click"
    );
    vm.click(song(first), ctrl(), at(1000));
    press_card(&vm, 0, ctrl());
    vm.grid_selection().toggle(a.clone());
    assert!(
        vm.selection().get().selected.contains(&song(first)),
        "a Ctrl toggle keeps the songs"
    );
    assert_eq!(vm.selected_items().len(), 3);
    // A plain click on one of two selected tiles leaves one: a replacement.
    press_card(&vm, 1, Modifiers::NONE);
    vm.grid_selection().select(b.clone());
    assert!(vm.selection().get().selected.is_empty());
    assert_eq!(vm.grid_selection().selected_keys(), vec![b]);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_shift_click_on_a_card_ranges_over_the_open_albums_songs_as_iced() {
    // Review #11b: Iced's `handle_click` ranges over the grid's items, cards
    // and the band's songs (`library.rs:913-922`); the grid's own Shift
    // range is over the cards alone, and the size of its change was taken
    // for a Ctrl toggle (adjacent) or a replacement (farther).
    let (services, vm) = loaded();
    vm.set_grid_per_row(2);
    let list = services.albums.list();
    let cards: Vec<String> = (0..list.len())
        .map(|i| list.with_item(i, |c| c.id.clone()).unwrap())
        .collect();
    // An album with a few songs, on an even card (the first of its row of
    // two), with a row under it.
    let snapshot = services.library_songs.snapshot().get();
    let count = |id: &String| {
        album_songs_in_memory(id, &snapshot.songs, &snapshot.albums).map_or(0, |s| s.len())
    };
    let open = (0..cards.len().saturating_sub(2))
        .step_by(2)
        .find(|i| count(&cards[*i]) >= 3)
        .unwrap();
    let (next, below) = (open + 1, open + 2);
    vm.toggle_album(&cards[open]);
    let songs: Vec<i64> = vm.expanded_songs().get().iter().map(|s| s.id).collect();
    // A plain click on the band's second song: the pivot.
    vm.click(song(songs[1]), Modifiers::NONE, at(0));
    // Shift+click on the first card of the row under the band: the grid
    // ranges over the cards from its own anchor.
    press_card(&vm, below, Modifiers::SHIFT);
    vm.grid_selection()
        .select_keys(cards[open..=below].to_vec(), false);
    assert_eq!(
        vm.grid_selection().selected_keys(),
        vec![cards[below].clone()]
    );
    // The songs from the pivot on, and, the card being an end of the range,
    // its own songs (`select_range`).
    let selected = vm.selection().get().selected;
    assert!(songs[1..].iter().all(|id| selected.contains(&song(*id))));
    assert!(!selected.contains(&song(songs[0])));
    let below_songs =
        album_songs_in_memory(&cards[below], &snapshot.songs, &snapshot.albums).unwrap();
    assert_eq!(selected.len(), songs.len() - 1 + below_songs.len());
    // Adjacent: the band is after the row, so the range holds no song.
    press_card(&vm, open, Modifiers::NONE);
    vm.grid_selection().select(cards[open].clone());
    press_card(&vm, next, Modifiers::SHIFT);
    vm.grid_selection()
        .select_keys(vec![cards[open].clone(), cards[next].clone()], false);
    let mut keys = vm.grid_selection().selected_keys();
    keys.sort();
    let mut wanted = vec![cards[open].clone(), cards[next].clone()];
    wanted.sort();
    assert_eq!(keys, wanted);
    // Both ends are cards: their songs, nothing else (`select_range`).
    let ends: HashSet<LibraryListItem> = [open, next]
        .iter()
        .flat_map(|i| album_songs_in_memory(&cards[*i], &snapshot.songs, &snapshot.albums).unwrap())
        .map(|s| song(s.id))
        .collect();
    assert_eq!(vm.selection().get().selected, ends);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_selection_carries_from_the_grid_to_a_list_and_back() {
    // Review #11c: Iced keeps one `selected_items` across the modes.
    let (services, vm) = loaded();
    let card = services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    vm.grid_selection().select(card.clone());
    let songs = vm.selected_songs().len();
    assert!(songs > 0);
    vm.set_view_mode(LibraryViewMode::DetailedList);
    assert!(
        vm.selection()
            .get()
            .selected
            .contains(&LibraryListItem::Album(card.clone()))
    );
    assert_eq!(
        vm.selected_songs().len(),
        songs,
        "Play takes the same songs"
    );
    assert_eq!(vm.delete_targets().len(), songs);
    vm.set_view_mode(LibraryViewMode::Grid);
    assert_eq!(vm.grid_selection().selected_keys(), vec![card]);
    assert!(vm.selection().get().selected.is_empty());
    assert_eq!(vm.selected_songs().len(), songs);
    assert!(services.ctx.shutdown());
}

#[test]
fn cards_the_grid_drops_leave_the_open_albums_songs_selected() {
    // Review #11d: a search that drops a selected card from the grid's
    // selection is no gesture; Iced's selection survives a search
    // (`app.rs:2797`).
    let (services, vm) = loaded();
    let list = services.albums.list();
    let (a, b) = (
        list.with_item(0, |c| c.id.clone()).unwrap(),
        list.with_item(1, |c| c.id.clone()).unwrap(),
    );
    vm.toggle_album(&a);
    let first = vm.expanded_songs().get()[0].id;
    vm.click(song(first), Modifiers::NONE, at(0));
    press_card(&vm, 1, ctrl());
    vm.grid_selection().toggle(b.clone());
    assert_eq!(vm.selected_items().len(), 2);
    // The grid drops `b`, which its albums no longer hold.
    vm.grid_selection().clear();
    assert!(vm.selection().get().selected.contains(&song(first)));
    assert!(services.ctx.shutdown());
}

#[test]
fn a_reveal_in_the_grid_opens_the_album_and_selects_the_song() {
    let (services, vm) = loaded();
    let list = services.albums.list();
    let (tile, card) = (0..list.len())
        .find_map(|i| {
            list.with_item(i, |c| (c.title == "Starman").then(|| (i, c.clone())))
                .flatten()
        })
        .unwrap();
    let snapshot = services.library_songs.snapshot().get();
    let target = album_songs_in_memory(&card.id, &snapshot.songs, &snapshot.albums)
        .unwrap()
        .remove(1);
    vm.reveal_path(&target.full_file_path);
    assert_eq!(vm.expanded_album().get(), Some(card.id.clone()));
    assert_eq!(
        vm.selection().get().selected,
        [song(target.id)].into_iter().collect()
    );
    assert_eq!(vm.grid_selection().count(), 0);
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::GridSong {
            tile,
            row: 1,
            force_top: true
        })
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_36_the_shared_offset_stays_within_the_overflowing_content() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let widths = default_widths();
    let content = content_width(LibraryViewMode::SimpleList, &widths);
    let expected: f32 = columns_for(LibraryViewMode::SimpleList)
        .into_iter()
        .map(|c| f32::from(widths[&c]))
        .sum::<f32>()
        + 29.0
        + 5.0;
    assert_eq!(content, expected);
    vm.scroll_horizontally(-50.0, 700.0);
    assert_eq!(vm.h_offset().get(), 0.0);
    vm.scroll_horizontally(120.0, 700.0);
    assert_eq!(vm.h_offset().get(), 120.0);
    vm.scroll_horizontally(10_000.0, 700.0);
    assert_eq!(vm.h_offset().get(), content - 700.0);
    vm.clamp_h_offset(content);
    assert_eq!(vm.h_offset().get(), 0.0, "a viewport the content fits in");
    assert!(services.ctx.shutdown());
}

// ---- G4: a refresh clears the selection, a search does not ---------------

/// A widget whose build wires a view-model to the facade's events, as the
/// panel's build does.
#[derive(Debug)]
struct Wired(LibraryViewModel);

impl Widget for Wired {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.0.wire(ctx);
        Vec::new()
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }
}

/// The loaded view-model, wired through a tree whose events `pump` delivers.
fn wired() -> (
    AppServices,
    LibraryViewModel,
    WidgetTree,
    crate::teksilo_ui::test_support::EventPump,
) {
    let (services, vm) = loaded();
    let (mut tree, _runtime, pump) = crate::teksilo_ui::test_support::tree_with_pump(&services.ctx);
    tree.add(Wired(vm.clone()));
    crate::teksilo_ui::test_support::layout(&mut tree);
    (services, vm, tree, pump)
}

/// The library changed (a scan, a delete): the event, then the reload the
/// library model makes on it.
fn library_changed(
    services: &AppServices,
    tree: &WidgetTree,
    pump: &crate::teksilo_ui::test_support::EventPump,
) {
    services.ctx.hub.publish(Event::Library(
        crate::frontend::events::LibraryEvent::Changed,
    ));
    assert!(run_until(&services.runtime, TIMEOUT, || pump.deliver(tree) > 0));
    services.library_songs.reload();
    settle(services);
}

#[test]
fn g4_a_library_refresh_clears_the_selection_and_keeps_the_focus() {
    // `InternalLibraryRefreshed` empties `selected_items` (`app.rs:3944`).
    let (services, vm, tree, pump) = wired();
    list_mode(&vm);
    vm.grid_selection().select(first_card(&services));
    let picked = bowie(&vm).songs[0].id;
    vm.click(song(picked), Modifiers::NONE, at(0));
    assert!(!vm.selection().get().selected.is_empty());
    assert!(!vm.grid_selection().selected_keys().is_empty());
    library_changed(&services, &tree, &pump);
    assert!(vm.selection().get().selected.is_empty());
    assert!(vm.grid_selection().selected_keys().is_empty());
    assert_eq!(vm.selection().get().focused, Some(song(picked)));
    assert_eq!(
        vm.stats().get().songs,
        vm.structure().get().songs.len() as u64
    );
    // Only that refresh: a selection made afterwards stays.
    vm.click(song(picked), Modifiers::NONE, at(1_000));
    services.library_songs.reload();
    settle(&services);
    assert_eq!(
        vm.selection().get().selected,
        [song(picked)].into_iter().collect()
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn g4_a_search_or_a_filter_answer_keeps_the_selection() {
    // `LibraryFilteredLoaded` (`app.rs:3909`) leaves `selected_items` alone.
    let (services, vm, _tree, _pump) = wired();
    list_mode(&vm);
    let picked = bowie(&vm).songs[0].clone();
    vm.click(song(picked.id), Modifiers::NONE, at(0));
    let title = picked.title.as_deref().unwrap_or_default().to_string();
    vm.search().set(title);
    settle(&services);
    assert!(vm.structure().get().songs.iter().any(|s| s.id == picked.id));
    assert_eq!(
        vm.selection().get().selected,
        [song(picked.id)].into_iter().collect()
    );
    vm.search().set(String::new());
    services
        .library_songs
        .update_params(|params| params.artist = Some(BOWIE.to_string()));
    settle(&services);
    assert_eq!(
        vm.selection().get().selected,
        [song(picked.id)].into_iter().collect()
    );
    assert!(services.ctx.shutdown());
}

// ---- G1 and G6: a filter change -------------------------------------------

#[test]
fn g1_a_filter_change_brings_the_library_back_to_its_top() {
    // `ChangeGeneralFilter`, `SelectSubfilter`, `SelectTreeNode` scroll the
    // library to the top (`app.rs:4276-4282`, `:4313-4316`, `:4349-4354`).
    let (services, vm) = loaded();
    list_mode(&vm);
    vm.click(song(bowie(&vm).songs[3].id), Modifiers::NONE, at(0));
    assert!(vm.navigate(LibraryNavDir::Down, Modifiers::NONE));
    assert!(matches!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::Reveal { .. })
    ));
    vm.on_filter_changed();
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::Top)
    );
    assert!(services.ctx.shutdown());
}

/// The column sets the library's filter to `artist`, then tells the library,
/// as `FiltersViewModel::select_node` and the App do.
fn filter_by_artist(services: &AppServices, vm: &LibraryViewModel, artist: &str) {
    services
        .library_songs
        .update_params(|params| params.artist = Some(artist.to_string()));
    vm.on_filter_changed();
}

#[test]
fn g6_a_filter_change_shows_the_filters_figures_until_its_songs_arrive() {
    // `selection_stats = None` "so that filter_stats has priority"
    // (`app.rs:4265-4269`), until `LibraryFilteredLoaded` recomputes it over
    // the new songs (`:3909-3912`); `selected_items` stays.
    let (services, vm) = loaded();
    list_mode(&vm);
    let picked = bowie(&vm).songs[0].clone();
    vm.click(song(picked.id), Modifiers::NONE, at(0));
    assert_eq!(
        vm.stats().get(),
        compute_stats(std::slice::from_ref(&picked))
    );
    let shown = services.library_songs.snapshot().get().songs.clone();
    filter_by_artist(&services, &vm, BOWIE);
    assert_eq!(
        vm.stats().get(),
        compute_stats(&shown),
        "the filter's figures"
    );
    assert_eq!(
        vm.selection().get().selected,
        [song(picked.id)].into_iter().collect(),
        "the selection stays"
    );
    settle(&services);
    assert_eq!(
        vm.stats().get(),
        compute_stats(std::slice::from_ref(&picked)),
        "the selection's figures again, over the filter's songs"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn g6_a_filter_that_hides_the_selection_leaves_the_figures_to_its_songs() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let other = vm
        .structure()
        .get()
        .songs
        .iter()
        .find(|s| crate::utils::get_effective_artist(s) != BOWIE)
        .cloned()
        .unwrap();
    vm.click(song(other.id), Modifiers::NONE, at(0));
    filter_by_artist(&services, &vm, BOWIE);
    settle(&services);
    let bowie_songs = services.library_songs.snapshot().get().songs.clone();
    assert!(bowie_songs.iter().all(|s| s.id != other.id));
    assert_eq!(vm.stats().get(), compute_stats(&bowie_songs));
    assert!(services.ctx.shutdown());
}

#[test]
fn g6_a_selection_change_takes_the_figures_back_at_once() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let songs = bowie(&vm).songs.clone();
    vm.click(song(songs[0].id), Modifiers::NONE, at(0));
    filter_by_artist(&services, &vm, BOWIE);
    vm.click(song(songs[1].id), Modifiers::NONE, at(1_000));
    assert_eq!(
        vm.stats().get(),
        compute_stats(std::slice::from_ref(&songs[1]))
    );
    settle(&services);
    assert!(services.ctx.shutdown());
}

#[test]
fn g6_a_letter_that_queries_nothing_keeps_the_selections_figures() {
    // A letter leaves the library's filters as they were: nothing is
    // queried here (Iced queries the same songs again and recomputes the
    // same figures).
    let (services, vm) = loaded();
    list_mode(&vm);
    let picked = bowie(&vm).songs[0].clone();
    vm.click(song(picked.id), Modifiers::NONE, at(0));
    assert!(!services.library_songs.loading().get());
    vm.on_filter_changed();
    assert_eq!(
        vm.stats().get(),
        compute_stats(std::slice::from_ref(&picked))
    );
    assert!(services.ctx.shutdown());
}

// ---- G2: the grid's keys walk the open album's songs ----------------------

/// The grid, four cards to a row, its longest album open (its card
/// selected, so the keys start from it).
struct GridOpen {
    services: AppServices,
    vm: LibraryViewModel,
    /// The cards, in the grid's order.
    cards: Vec<String>,
    /// The open album's songs, as the band shows them.
    songs: Vec<i64>,
    /// The open card.
    open: usize,
    /// Another card of the open card's row.
    same_row: usize,
    /// A card of another row.
    other_row: usize,
}

fn grid_open() -> GridOpen {
    let (services, vm) = loaded();
    vm.set_grid_per_row(4);
    let list = services.albums.list();
    let cards: Vec<String> = (0..list.len())
        .filter_map(|i| list.with_item(i, |c| c.id.clone()))
        .collect();
    let snapshot = services.library_songs.snapshot().get();
    let open = (0..cards.len())
        .max_by_key(|&i| {
            album_songs_in_memory(&cards[i], &snapshot.songs, &snapshot.albums)
                .map_or(0, |songs| songs.len())
        })
        .unwrap();
    vm.toggle_album(&cards[open]);
    let songs: Vec<i64> = vm.expanded_songs().get().iter().map(|s| s.id).collect();
    assert!(songs.len() >= 4, "a long enough album: {}", songs.len());
    let row = open / 4 * 4;
    let same_row = (row..row + 4)
        .find(|&i| i != open && i < cards.len())
        .unwrap();
    let other_row = if row == 0 { 4 } else { 0 };
    assert!(other_row < cards.len());
    GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        same_row,
        other_row,
    }
}

fn grid_keys(vm: &LibraryViewModel) -> HashSet<String> {
    vm.grid_selection().selected_keys().into_iter().collect()
}

fn walk(vm: &LibraryViewModel, key: Key, modifiers: Modifiers) -> GridKey {
    vm.grid_key(&key, modifiers, false)
}

#[test]
fn g2_down_from_the_open_card_enters_its_songs_and_up_from_the_first_returns() {
    // `handle_key_nav`, Grid branch (`library.rs:808-872`).
    let GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        ..
    } = grid_open();
    assert_eq!(
        walk(&vm, Key::ArrowDown, Modifiers::NONE),
        GridKey::EnteredBand
    );
    let selection = vm.selection().get();
    assert_eq!(selection.selected, [song(songs[0])].into_iter().collect());
    assert_eq!(selection.focused, Some(song(songs[0])));
    assert!(grid_keys(&vm).is_empty(), "the card is no longer selected");
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::GridSong {
            tile: open,
            row: 0,
            force_top: false
        })
    );
    assert_eq!(walk(&vm, Key::ArrowDown, Modifiers::NONE), GridKey::Used);
    assert_eq!(vm.selection().get().focused, Some(song(songs[1])));
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::GridSong {
            tile: open,
            row: 1,
            force_top: false
        })
    );
    assert_eq!(walk(&vm, Key::ArrowUp, Modifiers::NONE), GridKey::Used);
    assert_eq!(vm.selection().get().focused, Some(song(songs[0])));
    assert_eq!(walk(&vm, Key::ArrowUp, Modifiers::NONE), GridKey::LeftBand);
    assert_eq!(grid_keys(&vm), [cards[open].clone()].into_iter().collect());
    assert!(vm.selection().get().selected.is_empty());
    assert_eq!(vm.grid_anchor(), Some(cards[open].clone()));
    assert_eq!(
        vm.scroll_request().get().map(|r| r.1),
        Some(ScrollRequest::GridTile(open))
    );
    // On the open card again, ↑ closes it (`app.rs:3187-3200`).
    assert_eq!(walk(&vm, Key::ArrowUp, Modifiers::NONE), GridKey::Used);
    assert_eq!(vm.expanded_album().get(), None);
    assert!(services.ctx.shutdown());
}

#[test]
fn g2_down_from_any_card_of_the_open_row_enters_the_band_and_other_steps_are_the_grids() {
    let GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        same_row,
        other_row,
    } = grid_open();
    vm.grid_selection().select(cards[same_row].clone());
    assert_eq!(vm.expanded_album().get(), Some(cards[open].clone()));
    assert_eq!(
        walk(&vm, Key::ArrowDown, Modifiers::NONE),
        GridKey::EnteredBand
    );
    assert_eq!(vm.selection().get().focused, Some(song(songs[0])));
    // From another row, a step between cards: the grid's own.
    vm.grid_selection().select(cards[other_row].clone());
    for key in [
        Key::ArrowDown,
        Key::ArrowUp,
        Key::ArrowLeft,
        Key::ArrowRight,
    ] {
        assert_eq!(walk(&vm, key, Modifiers::NONE), GridKey::Unused, "{key:?}");
    }
    // Sideways from the open row: the grid's own too.
    vm.grid_selection().select(cards[same_row].clone());
    assert_eq!(walk(&vm, Key::ArrowRight, Modifiers::NONE), GridKey::Unused);
    assert_eq!(walk(&vm, Key::ArrowLeft, Modifiers::NONE), GridKey::Unused);
    assert!(services.ctx.shutdown());
}

#[test]
fn g2_down_stops_at_the_last_song_and_left_and_right_stay() {
    let GridOpen {
        services,
        vm,
        songs,
        ..
    } = grid_open();
    let last = *songs.last().unwrap();
    vm.click(song(last), Modifiers::NONE, at(0));
    assert_eq!(walk(&vm, Key::ArrowDown, Modifiers::NONE), GridKey::Used);
    assert_eq!(vm.selection().get().focused, Some(song(last)));
    vm.click(song(songs[1]), Modifiers::NONE, at(1_000));
    vm.click(song(songs[2]), ctrl(), at(2_000));
    for key in [Key::ArrowLeft, Key::ArrowRight] {
        assert_eq!(walk(&vm, key, Modifiers::NONE), GridKey::Used);
        let selection = vm.selection().get();
        assert_eq!(selection.focused, Some(song(songs[2])), "{key:?}");
        // Iced's `navigate` to the same song selects it alone.
        assert_eq!(selection.selected, [song(songs[2])].into_iter().collect());
    }
    assert!(services.ctx.shutdown());
}

#[test]
fn g2_shift_extends_the_selection_over_the_songs_from_the_pivot() {
    let GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        ..
    } = grid_open();
    assert_eq!(
        walk(&vm, Key::ArrowDown, Modifiers::NONE),
        GridKey::EnteredBand
    );
    walk(&vm, Key::ArrowDown, Modifiers::SHIFT);
    walk(&vm, Key::ArrowDown, Modifiers::SHIFT);
    let selection = vm.selection().get();
    assert_eq!(
        selection.selected,
        songs[..3].iter().map(|id| song(*id)).collect()
    );
    assert_eq!(selection.pivot, Some(song(songs[0])));
    assert_eq!(selection.focused, Some(song(songs[2])));
    // Up past the first song, still extending: the range runs from the song
    // to the open card over Iced's order, the cards after it in its row
    // included, and the card, an end of it, brings all its songs.
    walk(&vm, Key::ArrowUp, Modifiers::SHIFT);
    walk(&vm, Key::ArrowUp, Modifiers::SHIFT);
    assert_eq!(walk(&vm, Key::ArrowUp, Modifiers::SHIFT), GridKey::LeftBand);
    let row_end = (open / 4 * 4 + 4).min(cards.len());
    assert_eq!(
        grid_keys(&vm),
        cards[open..row_end].iter().cloned().collect()
    );
    assert_eq!(
        vm.selection().get().selected,
        songs.iter().map(|id| song(*id)).collect()
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn g2_a_clicked_song_is_where_the_walk_starts_and_a_ctrl_click_on_a_card_ends_it() {
    let GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        other_row,
        ..
    } = grid_open();
    vm.click(song(songs[1]), Modifiers::NONE, at(0));
    assert_eq!(walk(&vm, Key::ArrowDown, Modifiers::NONE), GridKey::Used);
    assert_eq!(vm.selection().get().focused, Some(song(songs[2])));
    // The open card and a song Ctrl-picked with it, then a Ctrl-click on a
    // card of another row: the songs stay selected, and the walk starts
    // from that card (Iced's `focused_item`), a step the grid takes.
    vm.grid_selection().select(cards[open].clone());
    vm.click(song(songs[1]), ctrl(), at(1_000));
    assert_eq!(vm.selection().get().focused, Some(song(songs[1])));
    vm.grid_selection().toggle(cards[other_row].clone());
    assert!(vm.selection().get().selected.contains(&song(songs[1])));
    assert_eq!(walk(&vm, Key::ArrowDown, Modifiers::NONE), GridKey::Unused);
    assert_eq!(vm.selection().get().focused, None);
    assert!(services.ctx.shutdown());
}

#[test]
fn g2_a_shift_click_ranges_over_the_card_and_the_songs_as_iced_does() {
    // `handle_click` ranges over `get_visible_items`, cards and songs.
    let GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        ..
    } = grid_open();
    assert_eq!(
        walk(&vm, Key::ArrowDown, Modifiers::NONE),
        GridKey::EnteredBand
    );
    assert_eq!(walk(&vm, Key::ArrowUp, Modifiers::NONE), GridKey::LeftBand);
    // The pivot is the open card now: the range runs from it to the song,
    // the cards after it in its row included.
    vm.click(song(songs[1]), Modifiers::SHIFT, at(0));
    let row_end = (open / 4 * 4 + 4).min(cards.len());
    assert_eq!(
        grid_keys(&vm),
        cards[open..row_end].iter().cloned().collect()
    );
    assert!(vm.selection().get().selected.contains(&song(songs[1])));
    assert!(services.ctx.shutdown());
}

// ---- G3: the grid's rubber band ---------------------------------------------

#[test]
fn g3_a_sweep_selects_the_cards_and_the_band_songs_it_crosses() {
    let GridOpen {
        services,
        vm,
        cards,
        songs,
        open,
        ..
    } = grid_open();
    // A hand-made layout: 90 × 100 cards in rows of four, 100 px apart, the
    // band's songs 10 px rows under the open card's row.
    let band_row = open / 4;
    let rect_of = |spot: GridSpot, _shape: GridShape| match spot {
        GridSpot::Card(i) => ContentRect::new(
            (i % 4) as f32 * 100.0,
            (i / 4) as f32 * 100.0 + if i / 4 > band_row { 1000.0 } else { 0.0 },
            90.0,
            100.0,
        ),
        GridSpot::Song(row) => ContentRect::new(
            0.0,
            (band_row + 1) as f32 * 100.0 + row as f32 * 10.0,
            400.0,
            10.0,
        ),
    };
    let top = band_row as f32 * 100.0;
    let (first, second) = (band_row * 4, band_row * 4 + 1);
    // From inside the row's second card down over the first two songs.
    vm.select_grid_marquee(
        ContentRect::new(150.0, top + 50.0, 20.0, 65.0),
        Modifiers::NONE,
        rect_of,
    );
    assert_eq!(
        grid_keys(&vm),
        [cards[second].clone()].into_iter().collect()
    );
    assert_eq!(
        vm.selection().get().selected,
        [song(songs[0]), song(songs[1])].into_iter().collect()
    );
    // Shift adds, Ctrl toggles.
    vm.select_grid_marquee(
        ContentRect::new(50.0, top + 50.0, 20.0, 10.0),
        Modifiers::SHIFT,
        rect_of,
    );
    assert_eq!(
        grid_keys(&vm),
        [cards[first].clone(), cards[second].clone()]
            .into_iter()
            .collect()
    );
    vm.select_grid_marquee(
        ContentRect::new(150.0, top + 105.0, 20.0, 2.0),
        ctrl(),
        rect_of,
    );
    assert_eq!(
        vm.selection().get().selected,
        [song(songs[1])].into_iter().collect()
    );
    assert_eq!(grid_keys(&vm).len(), 2, "the cards it does not cross stay");
    assert!(services.ctx.shutdown());
}

#[test]
fn a_press_on_the_grids_background_empties_the_selection() {
    // Review #11a in the grid: Iced's grid background deselects on the press
    // (`library.rs:1956`), before the rubber band.
    let (services, vm) = loaded();
    let list = services.albums.list();
    let (a, b) = (
        list.with_item(0, |c| c.id.clone()).unwrap(),
        list.with_item(1, |c| c.id.clone()).unwrap(),
    );
    vm.toggle_album(&a);
    let first = vm.expanded_songs().get()[0].id;
    vm.click(song(first), Modifiers::NONE, at(0));
    press_card(&vm, 1, ctrl());
    vm.grid_selection().toggle(b);
    assert_eq!(vm.selected_items().len(), 2);
    vm.grid_press((0.5, 0.5), Modifiers::SHIFT, |_, _| {
        ContentRect::new(-10.0, -10.0, 1.0, 1.0)
    });
    assert!(vm.selected_items().is_empty());
    assert!(services.ctx.shutdown());
}
