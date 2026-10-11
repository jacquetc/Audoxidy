use teksilo::core::WidgetTree;

use super::*;
use crate::frontend::AppContext;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::test_util::{
    REGION, TIMEOUT, all_of_type, loaded, mount_at, settle_layout, tree_for,
};
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{
    access_click, find_by_name, run_until, with_shipped_messages,
};

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
