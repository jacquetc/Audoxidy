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
fn the_delete_targets_add_whole_artists_and_albums_to_the_picked_songs() {
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
    let selected: HashSet<LibraryListItem> = [
        LibraryListItem::Album(id),
        song(album.songs[0].0.id),
        song(lone),
    ]
    .into_iter()
    .collect();
    let mut expected: Vec<i64> = album.songs.iter().map(|(s, _)| s.id).collect();
    expected.push(lone);
    expected.sort_unstable();
    assert_eq!(delete_target_ids(&selected, &structure.data()), expected);
    // Play narrows the album to the picked song; delete does not.
    let narrowed = selected_songs(&selected, &structure.data(), &structure.songs);
    assert_eq!(narrowed.len(), 2);
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_1_delete_asks_first_and_deletes_only_after_the_confirmation() {
    let (services, vm) = loaded();
    list_mode(&vm);
    let (mut tree, _) = tree_with_async(&services.ctx);
    let victim = bowie(&vm).songs[0].clone();
    vm.click(song(victim.id), Modifiers::NONE, at(0));
    let asked = vm.clone();
    press(&mut tree, move |ctx| asked.request_delete(ctx));
    let titles = drain_dialog_titles(&mut tree);
    assert_eq!(titles, vec!["Delete from the library?".to_string()]);
    services.library_songs.reload();
    settle(&services);
    assert!(
        services
            .library_songs
            .snapshot()
            .get()
            .songs
            .iter()
            .any(|s| s.id == victim.id),
        "nothing is deleted before the confirmation"
    );

    let op = vm.delete_songs(vm.delete_targets());
    assert!(vm.is_deleting());
    let deleted = loop {
        if let Some(result) = services.ctx.ops.take_result::<usize>(op) {
            break result.unwrap();
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(deleted, 1);
    services.library_songs.reload();
    settle(&services);
    assert!(
        !services
            .library_songs
            .snapshot()
            .get()
            .songs
            .iter()
            .any(|s| s.id == victim.id)
    );
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

#[test]
fn a_compilation_opens_with_the_songs_of_its_album_artist() {
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
    vm.toggle_album(&summer);
    assert_eq!(vm.expanded_songs().get().len(), 2);
    assert!(services.ctx.shutdown());
}

#[test]
fn alt_down_opens_and_alt_up_closes_the_selected_tile_and_up_closes_the_open_one() {
    let (services, vm) = loaded();
    let id = first_card(&services);
    vm.grid_selection().select(id.clone());
    assert_eq!(vm.grid_anchor(), Some(id.clone()));
    assert!(
        !vm.grid_key(&Key::ArrowUp, Modifiers::ALT),
        "nothing to close"
    );
    assert!(vm.grid_key(&Key::ArrowDown, Modifiers::ALT));
    assert_eq!(vm.expanded_album().get(), Some(id.clone()));
    assert!(
        !vm.grid_key(&Key::ArrowDown, Modifiers::ALT),
        "already open"
    );
    assert!(vm.grid_key(&Key::ArrowUp, Modifiers::ALT));
    assert_eq!(vm.expanded_album().get(), None);
    vm.toggle_album(&id);
    assert!(vm.grid_key(&Key::ArrowUp, Modifiers::NONE));
    assert_eq!(vm.expanded_album().get(), None);
    assert!(!vm.grid_key(&Key::ArrowLeft, Modifiers::ALT));
    assert!(services.ctx.shutdown());
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
    vm.grid_selection().toggle(b.clone());
    assert!(
        vm.selection().get().selected.is_empty(),
        "from no tile to one: a plain click"
    );
    vm.click(song(first), ctrl(), at(1000));
    vm.grid_selection().toggle(a.clone());
    assert!(
        vm.selection().get().selected.contains(&song(first)),
        "a Ctrl toggle keeps the songs"
    );
    assert_eq!(vm.selected_items().len(), 3);
    // A plain click on one of two selected tiles leaves one: a replacement.
    vm.grid_selection().select(b);
    assert!(vm.selection().get().selected.is_empty());
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
        Some(ScrollRequest::DetailRow(1))
    );
    let _ = tile;
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
