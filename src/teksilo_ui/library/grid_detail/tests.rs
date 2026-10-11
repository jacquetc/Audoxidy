use teksilo::canvas::Point;
use teksilo::core::{Modifiers, PointerButton, WidgetTree};

use super::*;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::test_util::{
    REGION, all_of_type, loaded, mount_at, settle_layout, tree_for,
};
use crate::teksilo_ui::test_support::{access_click, find_by_name, pointer_click};

fn mounted() -> (AppServices, LibraryViewModel, WidgetTree, WidgetId, String) {
    let (services, vm) = loaded();
    let id = services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    vm.toggle_album(&id);
    let mut tree = tree_for(&services);
    let detail = mount_at(&mut tree, GridDetail::new(vm.clone()), REGION.0, 300.0);
    settle_layout(&mut tree);
    (services, vm, tree, detail, id)
}

#[test]
fn the_detail_lists_the_open_album_under_its_header_in_32_px_rows() {
    let (services, vm, tree, detail, id) = mounted();
    let card = services.albums.card(&id).unwrap();
    let header = all_of_type(&tree, detail, "HeaderRow")[0];
    assert_eq!(
        tree.accessibility_node(header).name(),
        Some(card.title.as_str())
    );
    assert_eq!(tree.bounds(header).height, 32.0);
    let rows = all_of_type(&tree, detail, "SongRow");
    assert!(!rows.is_empty());
    assert!(rows.len() <= vm.expanded_songs().get().len());
    let first = tree.bounds(rows[0]);
    assert_eq!(first.height, 32.0);
    assert_eq!(first.y, tree.bounds(header).y + 32.0);
    assert_eq!(first.x - tree.bounds(detail).x, LEADING);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_click_on_a_detail_song_selects_it_and_clears_the_tiles() {
    let (services, vm, mut tree, detail, _id) = mounted();
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
    let _ = Modifiers::NONE;
    assert!(services.ctx.shutdown());
}

#[test]
fn the_header_chevron_closes_the_album() {
    let (services, vm, mut tree, detail, id) = mounted();
    let card = services.albums.card(&id).unwrap();
    let chevron = find_by_name(
        &tree,
        detail,
        &format!("Collapse \u{2068}{}\u{2069}", card.title),
    )
    .into_iter()
    .chain(find_by_name(
        &tree,
        detail,
        &format!("Collapse {}", card.title),
    ))
    .next()
    .unwrap();
    access_click(&mut tree, chevron);
    assert_eq!(vm.expanded_album().get(), None);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_rows_follow_the_search_as_iced_filters_them() {
    let (services, vm, mut tree, detail, _id) = mounted();
    let all = all_of_type(&tree, detail, "SongRow").len();
    let first = vm.expanded_songs().get()[0].clone();
    // Iced matches the text as typed against lowercased tags.
    let query = first.title.as_deref().unwrap_or_default().to_lowercase();
    vm.search().set(query.clone());
    settle_layout(&mut tree);
    let shown = all_of_type(&tree, detail, "SongRow").len();
    assert!(shown >= 1 && shown <= all, "{shown} of {all} for {query:?}");
    assert!(services.ctx.shutdown());
}
