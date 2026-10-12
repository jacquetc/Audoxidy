use teksilo::canvas::Point;
use teksilo::core::{PointerButton, WidgetTree};
use teksilo::widgets::Spacer;

use super::*;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::test_util::{
    REGION, all_of_type, loaded, resent_selected, settle_layout, tree_for,
};
use crate::teksilo_ui::test_support::pointer_click;

/// The band of the first album, alone, `REGION.0` wide and as tall as it
/// wants.
fn mounted() -> (AppServices, LibraryViewModel, WidgetTree, WidgetId) {
    let (services, vm) = loaded();
    let id = services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    vm.toggle_album(&id);
    let mut tree = tree_for(&services);
    let detail = tree.add(GridDetail::new(vm.clone()));
    let sized = tree.add(FixedSize::new().width(REGION.0).child(detail));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(sized)
            .child(Spacer::new()),
    );
    settle_layout(&mut tree);
    (services, vm, tree, detail)
}

#[test]
fn the_band_is_the_iced_block_32_px_rows_padded_and_no_header() {
    let (services, vm, tree, detail) = mounted();
    let count = vm.expanded_songs().get().len();
    assert!(count > 1);
    let band = tree.bounds(detail);
    assert_eq!(band.height, band_height(count));
    assert_eq!(band.height, 32.0 + count as f32 * 32.0 + 32.0 + 5.0);
    assert!(all_of_type(&tree, detail, "HeaderRow").is_empty());
    let rows = all_of_type(&tree, detail, "SongRow");
    assert_eq!(rows.len(), count, "alone, every row is realized");
    let (first, second) = (tree.bounds(rows[0]), tree.bounds(rows[1]));
    assert_eq!(first.height, 32.0);
    assert_eq!(first.y - band.y, PADDING_TOP);
    assert_eq!(first.x - band.x, LEADING);
    assert_eq!(second.y - first.y, 32.0);
    let shift = all_of_type(&tree, detail, "HShift")[0];
    let shift = tree.bounds(shift);
    assert_eq!(
        band.x + band.width - (shift.x + shift.width),
        TRAILING,
        "10 px after the rows"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn the_band_is_iced_s_height_and_takes_no_space_without_songs() {
    assert_eq!(band_height(0), 0.0);
    assert_eq!(band_height(3), 32.0 + 96.0 + 37.0);
}

#[test]
fn the_window_is_the_rows_iced_renders() {
    // The first row 284 px down (under a 252 px row of cards and the 32 px
    // padding), a 300 px viewport at the top, 150 px of margin.
    assert_eq!(rows_in_span(284.0, 20, -150.0, 450.0), (0, 6));
    // A row is kept while its bottom reaches the span's top.
    assert_eq!(rows_in_span(284.0, 20, 400.0, 900.0), (3, 20));
    assert_eq!(rows_in_span(284.0, 20, 380.0, 900.0), (2, 20));
    assert_eq!(rows_in_span(284.0, 4, 2000.0, 2600.0), (4, 4));
    assert_eq!(rows_in_span(284.0, 0, 0.0, 600.0), (0, 0));
}

#[test]
fn a_click_on_a_band_song_selects_it_and_clears_the_tiles() {
    let (services, vm, mut tree, detail) = mounted();
    assert_eq!(vm.grid_selection().count(), 1);
    let row = all_of_type(&tree, detail, "SongRow")[1];
    let bounds = tree.bounds(row);
    pointer_click(
        &mut tree,
        Point::new(bounds.x + 120.0, bounds.y + 16.0),
        PointerButton::Primary,
        1_000,
    );
    let song = vm.expanded_songs().get()[1].id;
    assert_eq!(
        vm.selection().get().selected,
        [LibraryListItem::Song(song)].into_iter().collect()
    );
    assert_eq!(vm.grid_selection().count(), 0);
    assert!(services.ctx.shutdown());
}

#[test]
fn quirk_53_the_rows_follow_the_search_whatever_its_case() {
    let (services, vm, mut tree, detail) = mounted();
    let all = all_of_type(&tree, detail, "SongRow").len();
    let first = vm.expanded_songs().get()[0].clone();
    // Iced matches the text as typed against lowercased tags, so a capital
    // letter hid every song.
    let query = first.title.as_deref().unwrap_or_default().to_uppercase();
    vm.search().set(query.clone());
    settle_layout(&mut tree);
    let shown = all_of_type(&tree, detail, "SongRow").len();
    assert!(shown >= 1 && shown <= all, "{shown} of {all} for {query:?}");
    assert!(services.ctx.shutdown());
}

#[test]
fn with_no_song_to_show_the_band_takes_no_space() {
    let (services, vm, mut tree, detail) = mounted();
    vm.search().set("zzqqxxnothing".to_string());
    settle_layout(&mut tree);
    assert_eq!(tree.bounds(detail).height, 0.0, "Iced draws no block");
    assert!(all_of_type(&tree, detail, "SongRow").is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn g7_a_selection_made_without_an_event_reaches_the_accessibility_tree() {
    // Ctrl+A in the grid selects the open album's songs too.
    let (services, vm, mut tree, detail) = mounted();
    let _ = tree.sync_accessibility();
    let rows = all_of_type(&tree, detail, "SongRow");
    assert!(rows.len() > 1);
    vm.select_all();
    crate::teksilo_ui::test_support::layout(&mut tree);
    assert_eq!(resent_selected(&mut tree, &rows), rows);
    assert!(services.ctx.shutdown());
}
