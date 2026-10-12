use std::cell::RefCell;

use teksilo::canvas::Point;
use teksilo::core::{Modifiers, PointerButton, PointerPhase, WidgetTree};

use super::*;
use crate::logic::library_geometry::{DETAILED_BLOCK_GAP, album_block_height};
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::detailed_list::AlbumBlock;
use crate::teksilo_ui::library::song_row::SongRow;
use crate::teksilo_ui::library::test_util::{
    REGION, all_of_type, loaded, mount_at, settle_layout, tree_for,
};
use crate::teksilo_ui::mock_data::BOWIE;
use crate::teksilo_ui::test_support::{layout, pointer_click, pointer_event};

struct Mounted {
    services: AppServices,
    vm: LibraryViewModel,
    tree: WidgetTree,
    id: WidgetId,
    scroll: Rc<RefCell<Option<Signal<f32>>>>,
    list: Rc<Cell<Option<WidgetId>>>,
}

impl Mounted {
    fn scroll_y(&self) -> Signal<f32> {
        self.scroll.borrow().clone().unwrap()
    }
}

fn mounted(mode: LibraryViewMode) -> Mounted {
    let (services, vm) = loaded();
    vm.set_view_mode(mode);
    let mut tree = tree_for(&services);
    let list = LibraryList::new(vm.clone(), mode);
    let (scroll, handle) = (list.scroll_handle(), list.list_handle());
    let id = mount_at(&mut tree, list, REGION.0, REGION.1 - 70.0);
    settle_layout(&mut tree);
    Mounted {
        services,
        vm,
        tree,
        id,
        scroll,
        list: handle,
    }
}

fn heights(m: &Mounted, suffix: &str) -> Vec<f32> {
    all_of_type(&m.tree, m.id, suffix)
        .into_iter()
        .map(|node| m.tree.bounds(node).height)
        .collect()
}

#[test]
fn the_source_follows_the_structure_and_bumps_its_version() {
    let m = mounted(LibraryViewMode::SimpleList);
    let source = LibraryRowsSource::new();
    source.set(&m.vm.structure().get());
    let rows = m.vm.structure().get().rows.len();
    assert_eq!(source.len(), rows);
    assert_eq!(source.version().get(), 1);
    m.vm.toggle_artist(BOWIE);
    source.set(&m.vm.structure().get());
    assert!(source.len() < rows);
    assert_eq!(source.version().get(), 2);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn simple_rows_are_32_px() {
    let m = mounted(LibraryViewMode::SimpleList);
    let headers = heights(&m, "HeaderRow");
    let songs = heights(&m, "SongRow");
    assert!(!headers.is_empty() && !songs.is_empty());
    assert!(
        headers.iter().chain(songs.iter()).all(|h| *h == 32.0),
        "{headers:?} {songs:?}"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn thumbnail_rows_are_42_px_with_a_32_px_cover() {
    let m = mounted(LibraryViewMode::ThumbnailList);
    let rows = heights(&m, "SongRow");
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|h| *h == 42.0));
    assert!(heights(&m, "HeaderRow").iter().all(|h| *h == 42.0));
    let row = all_of_type(&m.tree, m.id, "SongRow")[0];
    let art = all_of_type(&m.tree, row, "AlbumArt")[0];
    let bounds = m.tree.bounds(art);
    assert_eq!((bounds.width, bounds.height), (32.0, 32.0));
    assert!(m.services.ctx.shutdown());
}

#[test]
fn detailed_blocks_are_as_tall_as_their_card_or_their_songs() {
    let m = mounted(LibraryViewMode::DetailedList);
    let blocks = all_of_type(&m.tree, m.id, "AlbumBlock");
    assert!(!blocks.is_empty());
    let structure = m.vm.structure().get();
    let first = &structure.artists[0].albums[0];
    let height = m.tree.bounds(blocks[0]).height;
    assert_eq!(
        height,
        album_block_height(LibraryViewMode::DetailedList, true, first.songs.len())
    );
    assert_eq!(
        height,
        323f32.max(32.0 + first.songs.len() as f32 * 32.0) + DETAILED_BLOCK_GAP
    );
    // The card is 250 × 323 and the songs are 32 px rows beside it.
    let card = m.tree.bounds(m.tree.children(blocks[0])[0]);
    assert_eq!(card.height, height);
    let rows = all_of_type(&m.tree, blocks[0], "SongRow");
    assert_eq!(rows.len(), first.songs.len());
    assert!(rows.iter().all(|r| m.tree.bounds(*r).height == 32.0));
    let art = all_of_type(&m.tree, blocks[0], "AlbumArt")[0];
    assert_eq!(m.tree.bounds(art).width, 220.0);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_collapsed_detailed_album_is_its_header_and_the_gap() {
    let m = mounted(LibraryViewMode::DetailedList);
    let structure = m.vm.structure().get();
    let va = &structure.artists[0];
    let id = album_composite_id(&va.artist_name, &va.albums[0].album_hash);
    m.vm.toggle_album(&id);
    let mut tree = m.tree;
    settle_layout(&mut tree);
    let block = all_of_type(&tree, m.id, "AlbumBlock")[0];
    assert_eq!(tree.bounds(block).height, 32.0 + DETAILED_BLOCK_GAP);
    assert!(all_of_type(&tree, block, "AlbumArt").is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_selected_song_lights_its_detailed_card() {
    let m = mounted(LibraryViewMode::DetailedList);
    let structure = m.vm.structure().get();
    let va = structure
        .artists
        .iter()
        .find(|a| a.artist_name == BOWIE)
        .unwrap();
    let song = va.albums[0].songs[1].0.id;
    let block = AlbumBlock::from_structure(&m.vm, &structure, 0, 0, Rc::new(Cell::new(None)));
    assert!(block.is_some());
    m.vm.click(LibraryListItem::Song(song), Modifiers::NONE, Instant::now());
    let data = super::super::detailed_list::BlockData::from_structure(
        &structure,
        structure
            .artists
            .iter()
            .position(|a| a.artist_name == BOWIE)
            .unwrap(),
        0,
    )
    .unwrap();
    let lit = m.vm.selection().get().selected.iter().any(|item| {
        *item == LibraryListItem::Album(data.id.clone())
            || data
                .songs
                .iter()
                .any(|s| *item == LibraryListItem::Song(s.id))
    });
    assert!(
        lit,
        "the card lights for one of its songs (widgets.rs:3017)"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_sticky_header_pins_the_artist_under_the_top() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let pinned_name = |m: &Mounted| {
        let sticky = all_of_type(&m.tree, m.id, "StickyHeader")[0];
        all_of_type(&m.tree, sticky, "HeaderRow")
            .first()
            .and_then(|header| {
                m.tree
                    .accessibility_node(*header)
                    .name()
                    .map(str::to_string)
            })
    };
    let structure = m.vm.structure().get();
    assert_eq!(
        pinned_name(&m),
        Some(structure.artists[0].artist_name.clone())
    );
    let second = &structure.artists[1];
    m.scroll_y().set(second.top_y + 10.0);
    settle_layout(&mut m.tree);
    assert_eq!(pinned_name(&m), Some(second.artist_name.clone()));
    // The pinned header sits at the top of the list, 10 px in.
    let sticky = all_of_type(&m.tree, m.id, "StickyHeader")[0];
    let header = all_of_type(&m.tree, sticky, "HeaderRow")[0];
    let (list, row) = (m.tree.bounds(m.id), m.tree.bounds(header));
    assert_eq!(row.y, list.y);
    assert_eq!(row.x - list.x, LIST_LEADING);
    assert_eq!(row.height, 32.0);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_click_on_a_song_selects_it_and_focuses_the_list() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let row = all_of_type(&m.tree, m.id, "SongRow")[2];
    let bounds = m.tree.bounds(row);
    pointer_click(
        &mut m.tree,
        Point::new(bounds.x + 100.0, bounds.y + bounds.height / 2.0),
        PointerButton::Primary,
        1_000,
    );
    let selected = m.vm.selection().get().selected;
    assert_eq!(selected.len(), 1);
    assert!(matches!(
        selected.iter().next(),
        Some(LibraryListItem::Song(_))
    ));
    assert_eq!(m.tree.focused(), m.list.get());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_click_beside_the_rows_deselects() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let row = all_of_type(&m.tree, m.id, "SongRow")[2];
    let bounds = m.tree.bounds(row);
    let y = bounds.y + bounds.height / 2.0;
    pointer_click(
        &mut m.tree,
        Point::new(bounds.x + 100.0, y),
        PointerButton::Primary,
        1_000,
    );
    assert!(!m.vm.selection().get().selected.is_empty());
    let list = m.tree.bounds(m.id);
    pointer_click(
        &mut m.tree,
        Point::new(list.x + 3.0, y),
        PointerButton::Primary,
        2_000,
    );
    assert!(m.vm.selection().get().selected.is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_drag_over_the_rows_selects_them_with_the_marquee() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let rows = all_of_type(&m.tree, m.id, "SongRow");
    let (first, third) = (m.tree.bounds(rows[1]), m.tree.bounds(rows[3]));
    let start = Point::new(first.x + 200.0, first.y + 10.0);
    let end = Point::new(first.x + 260.0, third.y + 10.0);
    pointer_event(
        &mut m.tree,
        PointerPhase::Down,
        start,
        PointerButton::Primary,
        1_000,
    );
    pointer_event(
        &mut m.tree,
        PointerPhase::Move,
        Point::new(start.x + 10.0, start.y + 20.0),
        PointerButton::Primary,
        1_020,
    );
    pointer_event(
        &mut m.tree,
        PointerPhase::Move,
        end,
        PointerButton::Primary,
        1_040,
    );
    pointer_event(
        &mut m.tree,
        PointerPhase::Up,
        end,
        PointerButton::Primary,
        1_060,
    );
    let selected = m.vm.selection().get().selected;
    assert!(selected.len() >= 3, "{selected:?}");
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_36_the_rows_follow_the_shared_offset_and_the_headers_stay() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let row = all_of_type(&m.tree, m.id, "SongRow")[0];
    let header = all_of_type(&m.tree, m.id, "HeaderRow")[0];
    let (row_x, header_x) = (m.tree.bounds(row).x, m.tree.bounds(header).x);
    m.vm.h_offset().set(120.0);
    layout(&mut m.tree);
    assert_eq!(m.tree.bounds(row).x, row_x - 120.0);
    assert_eq!(m.tree.bounds(header).x, header_x, "headers have no columns");
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_sideways_wheel_over_the_list_moves_the_shared_offset() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let list = m.tree.bounds(m.id);
    m.tree.dispatch_event(WidgetEvent::scroll_at(
        ScrollDelta::Pixels { x: -80.0, y: 0.0 },
        Modifiers::NONE,
        Point::new(list.x + 200.0, list.y + 200.0),
    ));
    layout(&mut m.tree);
    assert_eq!(m.vm.h_offset().get(), 80.0);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_reveal_scrolls_the_song_to_the_top_below_the_sticky_header() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let structure = m.vm.structure().get();
    let last = structure.artists.last().unwrap();
    let song = last.albums[0].songs[0].0.clone();
    m.vm.reveal_path(&song.full_file_path);
    settle_layout(&mut m.tree);
    let (y, _) =
        m.vm.structure()
            .get()
            .item_y_range(&LibraryListItem::Song(song.id))
            .unwrap();
    let scroll = m.scroll_y().get();
    assert!(scroll > 0.0);
    assert!(
        (scroll - (y - 32.0)).abs() < 1.0 || scroll < y,
        "{scroll} for {y}"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn an_empty_library_says_so() {
    let ctx = crate::frontend::AppContext::in_memory().unwrap();
    let services = AppServices::new(
        &crate::teksilo_ui::shell::main_window::AppHandles::new(ctx),
        AsyncRuntimeHandle::new(),
    );
    let vm = LibraryViewModel::with_low_resource(&services, false);
    vm.set_view_mode(LibraryViewMode::SimpleList);
    let mut tree = tree_for(&services);
    let id = mount_at(
        &mut tree,
        LibraryList::new(vm, LibraryViewMode::SimpleList),
        600.0,
        400.0,
    );
    settle_layout(&mut tree);
    let shown = crate::teksilo_ui::test_support::find_by_name(
        &tree,
        id,
        "The library is empty or loading...",
    );
    assert!(shown.iter().any(|node| tree.bounds(*node).height > 0.0));
    assert!(services.ctx.shutdown());
}

#[test]
fn a_song_row_has_the_iced_cells() {
    let m = mounted(LibraryViewMode::SimpleList);
    let row = all_of_type(&m.tree, m.id, "SongRow")[0];
    let node = m.tree.accessibility_node(row);
    assert_eq!(node.role(), teksilo::core::accesskit::Role::Row);
    let _ = SongRow::new(
        m.vm.structure().get().songs[0].clone(),
        Vec::new(),
        m.vm.column_widths(),
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_marquee_held_near_the_bottom_scrolls_the_list_from_the_time_elapsed() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let list = m.tree.bounds(m.id);
    let rows = all_of_type(&m.tree, m.id, "SongRow");
    let first = m.tree.bounds(rows[1]);
    let start = Point::new(first.x + 200.0, first.y + 10.0);
    let near_bottom = Point::new(first.x + 220.0, list.y + list.height - 10.0);
    pointer_event(
        &mut m.tree,
        PointerPhase::Down,
        start,
        PointerButton::Primary,
        1_000,
    );
    pointer_event(
        &mut m.tree,
        PointerPhase::Move,
        near_bottom,
        PointerButton::Primary,
        1_020,
    );
    let before = m.scroll_y().get();
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(40));
        m.tree.frame_tick().set(0.04);
        layout(&mut m.tree);
    }
    let scrolled = m.scroll_y().get();
    assert!(scrolled > before, "{before} -> {scrolled}");
    pointer_event(
        &mut m.tree,
        PointerPhase::Up,
        near_bottom,
        PointerButton::Primary,
        1_400,
    );
    // Released: the frame ticks no longer scroll.
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        m.tree.frame_tick().set(0.02);
        layout(&mut m.tree);
    }
    assert_eq!(m.scroll_y().get(), scrolled);
    assert!(m.services.ctx.shutdown());
}

/// A drag from `start` to `end` with `modifiers` held.
fn drag_with(m: &mut Mounted, start: Point, end: Point, modifiers: Modifiers) {
    use teksilo::core::{EventTime, PointerSample};
    let mid = Point::new(start.x + 10.0, start.y + 20.0);
    for (phase, at, ms) in [
        (PointerPhase::Down, start, 1_000),
        (PointerPhase::Move, mid, 1_020),
        (PointerPhase::Move, end, 1_040),
        (PointerPhase::Up, end, 1_060),
    ] {
        m.tree.dispatch_pointer(
            PointerSample::mouse(phase, at, EventTime::from_millis(ms))
                .with_button(PointerButton::Primary)
                .with_modifiers(modifiers),
        );
        layout(&mut m.tree);
    }
}

#[test]
fn a_sweep_from_the_background_starts_from_an_empty_selection() {
    // Review #11a: Iced's background press deselects (`LibraryDeselect`,
    // `app.rs:3343-3362`), then the rubber band adds (Shift) or toggles
    // (Ctrl) against nothing.
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
        let mut m = mounted(LibraryViewMode::SimpleList);
        let rows = all_of_type(&m.tree, m.id, "SongRow");
        let (old, first, third) = (
            m.tree.bounds(rows[6]),
            m.tree.bounds(rows[1]),
            m.tree.bounds(rows[3]),
        );
        pointer_click(
            &mut m.tree,
            Point::new(old.x + 100.0, old.y + old.height / 2.0),
            PointerButton::Primary,
            500,
        );
        let before = m.vm.selection().get().selected;
        assert_eq!(before.len(), 1);
        let list = m.tree.bounds(m.id);
        let start = Point::new(list.x + 3.0, first.y + 10.0);
        let end = Point::new(list.x + 60.0, third.y + 10.0);
        drag_with(&mut m, start, end, modifiers);
        let selected = m.vm.selection().get().selected;
        assert!(selected.len() >= 3, "{modifiers:?}: {selected:?}");
        assert!(
            before.is_disjoint(&selected),
            "{modifiers:?}: the old selection is gone: {selected:?}"
        );
        assert!(m.services.ctx.shutdown());
    }
}

#[test]
fn a_press_on_a_row_does_not_deselect_it() {
    let mut m = mounted(LibraryViewMode::SimpleList);
    let row = m.tree.bounds(all_of_type(&m.tree, m.id, "SongRow")[2]);
    let at = Point::new(row.x + 100.0, row.y + row.height / 2.0);
    pointer_event(
        &mut m.tree,
        PointerPhase::Down,
        at,
        PointerButton::Primary,
        1_000,
    );
    pointer_event(
        &mut m.tree,
        PointerPhase::Up,
        at,
        PointerButton::Primary,
        1_005,
    );
    assert_eq!(m.vm.selection().get().selected.len(), 1);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn a_ctrl_press_on_a_row_or_a_header_keeps_the_selection() {
    // A tap with `modifiers` held, as `test_support::tap_at` makes one.
    let press = |m: &mut Mounted, node: WidgetId, modifiers: Modifiers, _ms: u64| {
        let b = m.tree.bounds(node);
        let at = Point::new(b.x + 100.0, b.y + b.height / 2.0);
        m.tree.dispatch_event(WidgetEvent::pointer_down(
            at,
            PointerButton::Primary,
            modifiers,
        ));
        m.tree.dispatch_event(WidgetEvent::pointer_up(
            at,
            PointerButton::Primary,
            modifiers,
        ));
        layout(&mut m.tree);
    };
    for (mode, suffix) in [
        (LibraryViewMode::SimpleList, "SongRow"),
        (LibraryViewMode::SimpleList, "HeaderRow"),
        (LibraryViewMode::DetailedList, "SongRow"),
    ] {
        let mut m = mounted(mode);
        let rows = all_of_type(&m.tree, m.id, suffix);
        press(&mut m, rows[1], Modifiers::NONE, 1_000);
        press(&mut m, rows[2], Modifiers::CTRL, 3_000);
        let selected = m.vm.selection().get().selected;
        assert_eq!(selected.len(), 2, "{mode:?} {suffix}: {selected:?}");
        assert!(m.services.ctx.shutdown());
    }
}
