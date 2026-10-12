use teksilo::core::WidgetTree;

use super::*;
use crate::frontend::AppContext;
use crate::logic::library_sort::SortState;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::grid_detail;
use crate::teksilo_ui::library::library_vm::album_songs_in_memory;
use crate::teksilo_ui::library::test_util::{
    REGION, TIMEOUT, all_of_type, loaded, mount_at, settle_layout, tree_for,
};
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{
    access_click, find_by_name, run_until, with_shipped_messages,
};
use crate::utils::SortColumn;

fn mounted(
    width: f32,
) -> (
    AppServices,
    LibraryViewModel,
    WidgetTree,
    WidgetId,
    Signal<GridSizing>,
) {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let grid = LibraryGrid::new(vm.clone());
    let sizing = grid.sizing();
    let id = mount_at(&mut tree, grid, width, REGION.1);
    settle_layout(&mut tree);
    (services, vm, tree, id, sizing)
}

fn columns(sizing: &Signal<GridSizing>) -> usize {
    match sizing.get() {
        GridSizing::FixedColumnCount { count, height } => {
            assert_eq!(height, ROW_HEIGHT);
            count
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_column_count_follows_the_width_between_two_and_ten() {
    for (width, wanted) in [
        (300.0, 2),
        (561.0, 3),
        (758.0, 4),
        (1200.0, 6),
        (2000.0, 10),
    ] {
        let (services, _vm, _tree, _id, sizing) = mounted(width);
        assert_eq!(columns(&sizing), wanted, "{width} px");
        assert!(services.ctx.shutdown());
    }
}

#[test]
fn low_resource_mode_caps_the_grid_at_five_columns() {
    let (services, _) = loaded();
    let vm = LibraryViewModel::with_low_resource(&services, true);
    let mut tree = tree_for(&services);
    let grid = LibraryGrid::new(vm);
    let sizing = grid.sizing();
    mount_at(&mut tree, grid, 1300.0, REGION.1);
    settle_layout(&mut tree);
    assert_eq!(columns(&sizing), 5);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_resize_changes_the_column_count() {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let width = Signal::new(758.0);
    let grid = LibraryGrid::new(vm);
    let sizing = grid.sizing();
    let id = tree.add(grid);
    tree.add(
        teksilo::widgets::VStack::new()
            .spacing(0.0)
            .alignment(teksilo::tokens::HAlignment::Leading)
            .child(
                FixedSize::new()
                    .width(width.clone())
                    .height(600.0)
                    .child(id),
            )
            .child(Spacer::new()),
    );
    settle_layout(&mut tree);
    assert_eq!(columns(&sizing), 4);
    width.set(1000.0);
    settle_layout(&mut tree);
    assert_eq!(columns(&sizing), 5);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_cards_are_182_by_252_at_the_iced_positions() {
    let (services, _vm, tree, id, sizing) = mounted(REGION.0);
    let count = columns(&sizing);
    let tiles = all_of_type(&tree, id, "AlbumTile");
    assert!(tiles.len() > count, "two rows at least");
    let origin = tree.bounds(id);
    let first = tree.bounds(tiles[0]);
    assert_eq!(first.x - origin.x, LEADING_INSET);
    assert_eq!(first.y - origin.y, 0.0);
    // The card inside the tile.
    let card = tree.children(tree.children(tiles[0])[0])[0];
    let card = tree.bounds(card);
    assert_eq!((card.width, card.height), (CARD_WIDTH, ROW_HEIGHT));
    let next_row = tree.bounds(tiles[count]);
    assert_eq!(next_row.y - first.y, ROW_HEIGHT, "252 px rows, no gap");
    let second = tree.bounds(tiles[1]);
    assert!(second.x - first.x >= CARD_WIDTH + COLUMN_SPACING);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_card_shows_the_cover_and_the_four_lines_of_text() {
    let (services, _vm, tree, id, _) = mounted(REGION.0);
    let tile = all_of_type(&tree, id, "AlbumTile")[0];
    let art = all_of_type(&tree, tile, "AlbumArt")[0];
    let art_bounds = tree.bounds(art);
    let tile_bounds = tree.bounds(tile);
    assert_eq!(
        (art_bounds.width, art_bounds.height),
        (COVER_SIZE, COVER_SIZE)
    );
    assert_eq!(art_bounds.x - tile_bounds.x, CARD_PADDING);
    assert_eq!(art_bounds.y - tile_bounds.y, CARD_PADDING_TOP);
    // Four 14 px lines under the cover, 2 px apart, 5 px below it.
    let lines: Vec<_> = all_of_type(&tree, tile, "FixedSize")
        .into_iter()
        .map(|node| tree.bounds(node))
        .filter(|b| b.height == LINE_HEIGHT)
        .collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].y, art_bounds.y + COVER_SIZE + COVER_GAP);
    assert_eq!(lines[1].y - lines[0].y, LINE_HEIGHT + LINE_SPACING);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_card_text_is_cut_as_iced_estimates_it() {
    // 122 px at 12 px: 6.6 px a character, 18 characters.
    assert_eq!(
        smart_truncate("Station to Station", 12.0, INFO_WIDTH),
        "Station to Station"
    );
    assert_eq!(
        smart_truncate("The Rise and Fall of Ziggy Stardust", 12.0, INFO_WIDTH),
        "The Rise and Fa..."
    );
    assert_eq!(smart_truncate("", 12.0, INFO_WIDTH), "");
}

#[test]
fn the_chevron_opens_and_closes_the_album_and_selects_its_card() {
    let (services, vm, mut tree, id, _) = mounted(REGION.0);
    let card = services.albums.list().with_item(0, |c| c.clone()).unwrap();
    let chevron = |tree: &WidgetTree| {
        find_by_name(
            tree,
            id,
            &format!("Open or close \u{2068}{}\u{2069}", card.title),
        )
        .into_iter()
        .chain(find_by_name(
            tree,
            id,
            &format!("Open or close {}", card.title),
        ))
        .next()
        .unwrap()
    };
    let first = chevron(&tree);
    access_click(&mut tree, first);
    assert_eq!(vm.expanded_album().get(), Some(card.id.clone()));
    assert_eq!(vm.grid_selection().selected_keys(), vec![card.id.clone()]);
    let again = chevron(&tree);
    access_click(&mut tree, again);
    assert_eq!(vm.expanded_album().get(), None);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_tiles_are_named_after_their_album() {
    let (services, _vm, mut tree, id, _) = mounted(REGION.0);
    let card = services.albums.list().with_item(0, |c| c.clone()).unwrap();
    let _ = tree.sync_accessibility();
    assert!(!find_by_name(&tree, id, &format!("{} — {}", card.artist, card.title)).is_empty());
    assert!(services.ctx.shutdown());
}

fn empty_services() -> AppServices {
    let ctx = AppContext::in_memory().unwrap();
    let services = AppServices::new(&AppHandles::new(ctx), AsyncRuntimeHandle::new());
    services.library_songs.reload();
    let version = services.library_songs.version();
    assert!(run_until(&services.runtime, TIMEOUT, || version.get() >= 1));
    services
}

#[test]
fn an_empty_grid_says_so_and_another_message_while_searching() {
    let services = empty_services();
    let vm = LibraryViewModel::with_low_resource(&services, false);
    let mut tree = tree_for(&services);
    let id = mount_at(&mut tree, LibraryGrid::new(vm.clone()), REGION.0, REGION.1);
    settle_layout(&mut tree);
    let shown = |tree: &WidgetTree, text: &str| {
        find_by_name(tree, id, text)
            .into_iter()
            .any(|node| tree.bounds(node).height > 0.0)
    };
    assert!(shown(&tree, "The library is empty."));
    vm.search().set("x".to_string());
    settle_layout(&mut tree);
    assert!(shown(&tree, "No albums found."));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_empty_messages_are_the_iced_spanish_ones() {
    with_shipped_messages("es-ES", || {
        assert_eq!(
            tr!(library_empty()).resolve_now(),
            "La biblioteca está vacía."
        );
        assert_eq!(
            tr!(library_no_albums()).resolve_now(),
            "No se encontraron álbumes."
        );
    });
}

#[test]
fn a_reveal_scrolls_the_grid_to_the_tile_and_a_search_back_to_the_top() {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let grid = LibraryGrid::new(vm.clone());
    let id = mount_at(&mut tree, grid, REGION.0, 500.0);
    settle_layout(&mut tree);
    let list = services.albums.list();
    let last = list.len() - 1;
    let card = list.with_item(last, |c| c.clone()).unwrap();
    let snapshot = services.library_songs.snapshot().get();
    let song = crate::teksilo_ui::library::library_vm::album_songs_in_memory(
        &card.id,
        &snapshot.songs,
        &snapshot.albums,
    )
    .unwrap()
    .remove(0);
    vm.reveal_path(&song.full_file_path);
    settle_layout(&mut tree);
    let grid_bounds = tree.bounds(id);
    let first = tree.bounds(all_of_type(&tree, id, "AlbumTile")[0]);
    assert!(first.y < grid_bounds.y, "scrolled down to the last row");
    vm.search().set("a".to_string());
    settle_layout(&mut tree);
    let first = tree.bounds(all_of_type(&tree, id, "AlbumTile")[0]);
    assert_eq!(first.y, grid_bounds.y, "back at the top");
    assert!(services.ctx.shutdown());
}

#[test]
fn an_open_album_shows_its_songs_inside_the_grid_under_its_row() {
    let (services, vm, mut tree, id, sizing) = mounted(REGION.0);
    let count = columns(&sizing);
    let card = services.albums.list().with_item(0, |c| c.clone()).unwrap();
    vm.toggle_album(&card.id);
    settle_layout(&mut tree);
    let grid = tree.bounds(id);
    let band = all_of_type(&tree, id, "GridDetail")[0];
    let band_bounds = tree.bounds(band);
    let tiles = all_of_type(&tree, id, "AlbumTile");
    let first = tree.bounds(tiles[0]);
    assert_eq!(band_bounds.y, first.y + ROW_HEIGHT, "under the album's row");
    let songs = vm.expanded_songs().get().len();
    assert_eq!(band_bounds.height, grid_detail::band_height(songs));
    let row = tree.bounds(all_of_type(&tree, band, "SongRow")[0]);
    assert_eq!(
        row.x - grid.x,
        LEADING_INSET + grid_detail::LEADING,
        "10 px in from the body, as in Iced"
    );
    assert_eq!(row.y, band_bounds.y + grid_detail::PADDING_TOP);
    assert_eq!(
        tree.bounds(tiles[count]).y,
        band_bounds.y + band_bounds.height,
        "the next row of cards moves down"
    );
    assert!(services.ctx.shutdown());
}

/// Sends the screen reader's `action` to `id`.
fn access_action(tree: &mut WidgetTree, id: WidgetId, action: teksilo::core::accesskit::Action) {
    tree.dispatch_event(teksilo::core::WidgetEvent::AccessAction {
        action,
        target: Some(id),
        target_node: teksilo::core::accessibility::widget_id_to_node_id(id),
        data: None,
    });
    settle_layout(tree);
}

#[test]
fn the_grid_s_own_expand_and_collapse_open_and_close_the_album() {
    use teksilo::core::accesskit::Action;
    let (services, vm, mut tree, id, _) = mounted(REGION.0);
    let card = services.albums.list().with_item(0, |c| c.clone()).unwrap();
    let label = format!("{} — {}", card.artist, card.title);
    // The tile, not the band, which takes its name.
    let tile = |tree: &mut WidgetTree| {
        let _ = tree.sync_accessibility();
        find_by_name(tree, id, &label)
            .into_iter()
            .find(|node| {
                tree.accessibility_node(*node).role() == teksilo::core::accesskit::Role::GridCell
            })
            .unwrap()
    };
    let first = tile(&mut tree);
    access_action(&mut tree, first, Action::Expand);
    assert_eq!(vm.expanded_album().get(), Some(card.id.clone()));
    assert_eq!(vm.grid_selection().selected_keys(), vec![card.id.clone()]);
    assert_eq!(all_of_type(&tree, id, "GridDetail").len(), 1);
    let again = tile(&mut tree);
    access_action(&mut tree, again, Action::Collapse);
    assert_eq!(vm.expanded_album().get(), None);
    assert!(all_of_type(&tree, id, "GridDetail").is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn the_open_tile_follows_the_album_through_a_sort() {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let grid = LibraryGrid::new(vm.clone());
    let expanded = grid.expanded();
    mount_at(&mut tree, grid, REGION.0, REGION.1);
    settle_layout(&mut tree);
    let list = services.albums.list();
    let card = list.with_item(0, |c| c.id.clone()).unwrap();
    vm.toggle_album(&card);
    assert_eq!(expanded.get(), Some(0));
    vm.set_sort(SortState {
        column: Some(SortColumn::Album),
        ascending: Some(false),
    });
    settle_layout(&mut tree);
    let moved = (0..list.len())
        .find(|&i| list.with_item(i, |c| c.id == card) == Some(true))
        .unwrap();
    assert_ne!(moved, 0, "the sort moved the album");
    assert_eq!(expanded.get(), Some(moved));
    assert_eq!(vm.expanded_album().get(), Some(card));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_band_realizes_only_the_rows_near_the_viewport_as_iced_does() {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let grid = LibraryGrid::new(vm.clone());
    let id = mount_at(&mut tree, grid, REGION.0, 100.0);
    settle_layout(&mut tree);
    let snapshot = services.library_songs.snapshot().get();
    let list = services.albums.list();
    // The album with the most songs.
    let (card, songs) = (0..list.len())
        .filter_map(|i| list.with_item(i, |c| c.clone()))
        .map(|c| {
            let songs =
                album_songs_in_memory(&c.id, &snapshot.songs, &snapshot.albums).unwrap_or_default();
            (c, songs)
        })
        .max_by_key(|(_, songs)| songs.len())
        .unwrap();
    let count = songs.len();
    assert!(count >= 8, "an album longer than the window: {count}");
    let grid_top = tree.bounds(id).y;
    // Its first song revealed: at the top of a 100 px viewport, with 100 px
    // of margin, the rows down to 200 px under it, seven.
    vm.reveal_path(&vm_first_song(&vm, &card.id).full_file_path);
    settle_layout(&mut tree);
    let band = all_of_type(&tree, id, "GridDetail")[0];
    assert_eq!(tree.bounds(band).height, grid_detail::band_height(count));
    let rows = all_of_type(&tree, band, "SongRow");
    assert_eq!(rows.len(), 7);
    assert_eq!(
        tree.bounds(rows[0]).y,
        grid_top,
        "the revealed song at the top"
    );
    // The last one: the window follows, the first rows go.
    let last = vm.expanded_songs().get().last().cloned().unwrap();
    vm.reveal_path(&last.full_file_path);
    settle_layout(&mut tree);
    let band = all_of_type(&tree, id, "GridDetail")[0];
    let band_top = tree.bounds(band).y;
    let rows = all_of_type(&tree, band, "SongRow");
    assert!(rows.len() < count, "{} rows of {count}", rows.len());
    let first = tree.bounds(rows[0]).y;
    assert!(
        first > band_top + grid_detail::PADDING_TOP,
        "the first song is no longer built"
    );
    let last_row = tree.bounds(*rows.last().unwrap());
    assert_eq!(
        last_row.y,
        band_top + grid_detail::PADDING_TOP + (count - 1) as f32 * 32.0
    );
    assert!(
        last_row.y >= grid_top && last_row.y + 32.0 <= grid_top + 100.0,
        "the revealed song is in view"
    );
    assert!(services.ctx.shutdown());
}

/// The first song of the album `id`, as the band lists them.
fn vm_first_song(vm: &LibraryViewModel, id: &str) -> std::sync::Arc<crate::db::database::SongData> {
    let services = vm.services();
    let snapshot = services.library_songs.snapshot().get();
    let mut songs = album_songs_in_memory(id, &snapshot.songs, &snapshot.albums).unwrap();
    let sort = vm.sort().get();
    crate::logic::library_sort::sort_songs(&mut songs, sort.column, sort.ascending);
    songs.remove(0)
}

#[test]
fn g3_the_rubber_bands_rectangles_are_where_the_grid_draws_its_cards_and_songs() {
    use crate::logic::library_selection::{GridShape, GridSpot};
    let (services, vm, mut tree, id, sizing) = mounted(REGION.0);
    let columns = columns(&sizing);
    let card = services
        .albums
        .list()
        .with_item(1, |c| c.id.clone())
        .unwrap();
    vm.toggle_album(&card);
    settle_layout(&mut tree);
    let songs = vm.expanded_songs().get().len();
    let shape = GridShape {
        cards: services.albums.list().len(),
        per_row: columns,
        open: Some(1),
        songs,
    };
    let grid = tree.bounds(id);
    let tiles = all_of_type(&tree, id, "AlbumTile");
    for index in [0, 1, columns - 1, columns, columns + 1] {
        let tile = tree.bounds(tiles[index]);
        let rect = spot_rect(GridSpot::Card(index), shape, grid.width);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (tile.x - grid.x, tile.y - grid.y, CARD_WIDTH, ROW_HEIGHT),
            "card {index}"
        );
    }
    let band = all_of_type(&tree, id, "GridDetail")[0];
    let rows = all_of_type(&tree, band, "SongRow");
    for (row, node) in rows.iter().enumerate().take(3) {
        let bounds = tree.bounds(*node);
        let rect = spot_rect(GridSpot::Song(row), shape, grid.width);
        assert_eq!(rect.y, bounds.y - grid.y, "song {row}");
        assert_eq!(rect.height, bounds.height);
        assert_eq!((rect.x, rect.width), (0.0, grid.width), "the grid's width");
    }
    assert!(services.ctx.shutdown());
}
