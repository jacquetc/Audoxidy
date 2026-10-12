use std::time::Duration;

use teksilo::core::{PointerButton, PointerPhase, WidgetTree};
use teksilo::widgets::Spacer;

use super::*;
use crate::frontend::commands::playlist;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::bottom_bar::{self, METER_SLOT_WIDTH};
use crate::teksilo_ui::library::test_util::{
    REGION, TIMEOUT, all_of_type, loaded, mount_at, plain, settle_layout, tree_for,
};
use crate::teksilo_ui::mock_data::BOWIE;
use crate::teksilo_ui::test_support::{
    access_click, drain_dialog_titles, find_by_name, key_down, key_down_with, layout, run_until,
    with_shipped_messages,
};

/// A recognisable stand-in for the meter.
#[derive(Debug)]
struct FakeMeter;

impl Widget for FakeMeter {
    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(400.0, 24.0).into()
    }
}

struct Mounted {
    services: AppServices,
    vm: LibraryViewModel,
    tree: WidgetTree,
    id: WidgetId,
}

fn mounted_with(panel: impl FnOnce(LibraryViewModel) -> LibraryPanel) -> Mounted {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let id = mount_at(&mut tree, panel(vm.clone()), REGION.0, REGION.1);
    settle_layout(&mut tree);
    Mounted {
        services,
        vm,
        tree,
        id,
    }
}

fn mounted() -> Mounted {
    mounted_with(LibraryPanel::new)
}

/// [`mounted_with`], in a region `width` px wide.
fn mounted_at(width: f32, panel: impl FnOnce(LibraryViewModel) -> LibraryPanel) -> Mounted {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let id = mount_at(&mut tree, panel(vm.clone()), width, REGION.1);
    settle_layout(&mut tree);
    Mounted {
        services,
        vm,
        tree,
        id,
    }
}

impl Mounted {
    fn node(&self, suffix: &str) -> WidgetId {
        all_of_type(&self.tree, self.id, suffix)[0]
    }

    fn named(&self, name: &str) -> WidgetId {
        find_by_name(&self.tree, self.id, name)[0]
    }

    fn list(&self) -> WidgetId {
        let mut stack = vec![self.id];
        while let Some(id) = stack.pop() {
            if self
                .tree
                .widget_type_name(id)
                .is_some_and(|name| name.contains("ListView<"))
            {
                return id;
            }
            stack.extend(self.tree.children(id));
        }
        panic!("no list")
    }

    fn relayout(&mut self) {
        settle_layout(&mut self.tree);
    }
}

#[test]
fn the_panel_stacks_the_column_header_the_body_and_the_bottom_bar() {
    let m = mounted();
    let panel = m.tree.bounds(m.id);
    assert_eq!((panel.width, panel.height), REGION);
    let header = m.tree.bounds(m.node("ModeHeader"));
    assert_eq!((header.y - panel.y, header.height), (0.0, 30.0));
    let bar = m.tree.bounds(m.node("BottomBar"));
    assert_eq!(bar.height, bottom_bar::HEIGHT);
    assert_eq!(bar.y + bar.height, panel.y + panel.height);
    assert_eq!(bar.width, panel.width);
    let body = m.tree.bounds(m.node("ModeBody"));
    assert_eq!(body.y, header.y + 30.0);
    assert_eq!(body.height, panel.height - 30.0 - 40.0);
    assert!(m.services.ctx.shutdown());
}

/// A meter that is shown: it wants Iced's 400 px.
#[derive(Debug)]
struct WideMeter;

impl Widget for WideMeter {
    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(METER_SLOT_WIDTH, 24.0).into()
    }
}

/// The trailing edge of the bar's content (15 px inside the panel).
fn content_end(m: &Mounted) -> f32 {
    let panel = m.tree.bounds(m.id);
    panel.x + panel.width - 15.0
}

#[test]
fn quirk_10_without_a_meter_the_actions_end_the_bar_after_a_free_slot() {
    let m = mounted();
    let search = m.tree.bounds(m.node("SearchField"));
    assert_eq!(search.width, bottom_bar::SEARCH_WIDTH);
    let stats = m.tree.bounds(m.node("Stats"));
    assert_eq!(stats.x - (search.x + search.width), 15.0);
    let view = m.tree.bounds(m.named("Change the view"));
    assert!((view.x + view.width - content_end(&m)).abs() < 0.5);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_10_a_meter_fills_its_slot_and_a_shown_one_gets_its_400_px() {
    // The library of a 1600 px window (998 px): room for Iced's 400 px meter.
    let m = mounted_at(998.0, |vm| LibraryPanel::new(vm).meter_slot(FakeMeter));
    let meter = m.tree.bounds(m.node("FakeMeter"));
    let stats = m.tree.bounds(m.node("Stats"));
    assert_eq!(meter.x - (stats.x + stats.width), 10.0);
    assert!(meter.width > METER_SLOT_WIDTH - 0.1, "{}", meter.width);
    assert_eq!(meter.height, bottom_bar::HEIGHT);
    let play = m.tree.bounds(m.named("Play the selection"));
    assert_eq!(play.x - (meter.x + meter.width), 10.0);
    let _ = Spacer::new();
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_10_in_a_narrow_bar_the_figures_give_way_and_the_actions_stay() {
    // The library of the default 1360 px window is 758 px wide: search,
    // figures, a shown meter and the actions do not fit. Iced lets the actions
    // fall off the bar; here the figures shrink.
    let m = mounted_with(|vm| LibraryPanel::new(vm).meter_slot(WideMeter));
    let view = m.tree.bounds(m.named("Change the view"));
    assert!(view.x + view.width <= content_end(&m) + 0.5);
    let stats = m.tree.bounds(m.node("Stats"));
    assert!(
        (stats.width - bottom_bar::STATS_MIN_WIDTH).abs() < 0.5,
        "the figures gave way first"
    );
    let meter = m.tree.bounds(m.node("WideMeter"));
    assert!(meter.width < METER_SLOT_WIDTH, "then the meter slot");
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_actions_are_the_iced_icons_at_the_iced_sizes_and_named() {
    let m = mounted();
    for (name, size) in [
        ("Play the selection", 36.0),
        ("Add a folder", 31.0),
        ("Show what is playing", 30.0),
        ("Change the view", 30.0),
    ] {
        let bounds = m.tree.bounds(m.named(name));
        assert_eq!((bounds.width, bounds.height), (size, size), "{name}");
    }
    assert!(!find_by_name(&m.tree, m.id, "Search the library").is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_view_button_cycles_the_mode_and_the_body_follows() {
    let mut m = mounted();
    assert!(!all_of_type(&m.tree, m.id, "LibraryGrid").is_empty());
    let button = m.named("Change the view");
    access_click(&mut m.tree, button);
    m.relayout();
    assert_eq!(m.vm.view_mode().get(), LibraryViewMode::DetailedList);
    assert!(all_of_type(&m.tree, m.id, "LibraryGrid").is_empty());
    assert!(!all_of_type(&m.tree, m.id, "AlbumBlock").is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_stats_read_the_figures_with_plurals() {
    let m = mounted();
    let stats = m.node("Stats");
    let text = m
        .tree
        .accessibility_node(m.tree.children(stats)[0])
        .name()
        .map(plain)
        .unwrap_or_default();
    let figures = m.vm.stats().get();
    assert!(
        text.starts_with(&format!("{} Songs | ", figures.songs)),
        "{text}"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_spanish_figures_use_the_iced_words_with_correct_plurals() {
    with_shipped_messages("es-ES", || {
        let one = crate::logic::library_groups::LibraryStats {
            songs: 1,
            albums: 1,
            artists: 1,
            duration_secs: 61.0,
            size_bytes: 2_097_152.0,
        };
        assert_eq!(
            plain(&bottom_bar::stats_text(&one).resolve_now()),
            "1 Canción | 1 Álbum | 1 Artista | 01:01 | 2.00 MB"
        );
        let many = crate::logic::library_groups::LibraryStats {
            songs: 220,
            albums: 28,
            artists: 3,
            ..one
        };
        assert_eq!(
            plain(&bottom_bar::stats_text(&many).resolve_now()),
            "220 Canciones | 28 Álbumes | 3 Artistas | 01:01 | 2.00 MB"
        );
        assert_eq!(tr!(library_search_placeholder()).resolve_now(), "Buscar...");
        assert_eq!(
            plain(&super::super::header_rows::artist_stats(1, 2, 75.0).resolve_now()),
            "1 Canción | 2 Álbumes | 01:15"
        );
        assert_eq!(
            plain(&super::super::header_rows::album_stats(3, 75.0).resolve_now()),
            "3 Canciones | 01:15"
        );
        assert_eq!(
            tr!(library_empty_or_loading()).resolve_now(),
            "La biblioteca está vacía o cargando..."
        );
        assert_eq!(tr!(library_delete_confirm()).resolve_now(), "Eliminar");
    });
}

/// The panel in the Simple list, a song clicked, the list focused.
fn simple_with_a_selection() -> (Mounted, WidgetId) {
    let mut m = mounted();
    m.vm.set_view_mode(LibraryViewMode::SimpleList);
    m.relayout();
    let list = m.list();
    let song = m.vm.structure().get().songs[0].id;
    m.vm.click(
        crate::logic::library_selection::LibraryListItem::Song(song),
        Modifiers::NONE,
        std::time::Instant::now(),
    );
    (m, list)
}

#[test]
fn quirk_1_delete_on_the_list_asks_for_confirmation() {
    let (mut m, list) = simple_with_a_selection();
    key_down(&mut m.tree, list, Key::Delete);
    assert_eq!(
        drain_dialog_titles(&mut m.tree),
        vec!["Delete from the library?".to_string()]
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_2_enter_on_the_list_appends_the_selection_to_the_active_playlist() {
    let (mut m, list) = simple_with_a_selection();
    m.services
        .session
        .start_restore(&m.services.ctx, &m.services.runtime);
    let active = m.services.active_playlist.id();
    assert!(run_until(&m.services.runtime, TIMEOUT, || active
        .get()
        .is_some()));
    let id = active.get().unwrap();
    let song = m.vm.selected_songs()[0].id;
    let holds = |m: &Mounted| {
        playlist::load_playlist(&m.services.ctx, id)
            .recv_blocking()
            .unwrap()
            .groups
            .iter()
            .any(|g| g.songs.iter().any(|s| s.song_id == song))
    };
    assert!(!holds(&m));
    key_down(&mut m.tree, list, Key::Enter);
    assert!(run_until(
        &m.services.runtime,
        Duration::from_secs(10),
        || holds(&m)
    ));
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_2_the_arrows_move_the_library_selection() {
    let (mut m, list) = simple_with_a_selection();
    let before = m.vm.selection().get().focused;
    key_down(&mut m.tree, list, Key::ArrowDown);
    let after = m.vm.selection().get().focused;
    assert_ne!(before, after);
    key_down_with(&mut m.tree, list, Key::ArrowDown, Modifiers::SHIFT);
    let extended = m.vm.selection().get();
    assert!(extended.selected.len() >= 2, "{:?}", extended.selected);
    assert_eq!(extended.pivot, after, "Shift extends from the pivot");
    key_down(&mut m.tree, list, Key::ArrowLeft);
    assert!(
        m.vm.selection().get().selected.iter().all(|item| !matches!(
            item,
            crate::logic::library_selection::LibraryListItem::Song(_)
        )),
        "Left leaves the collapsed headers selected"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_2_the_search_field_keeps_enter_delete_space_and_the_arrows() {
    let (mut m, _list) = simple_with_a_selection();
    m.services
        .session
        .start_restore(&m.services.ctx, &m.services.runtime);
    let active = m.services.active_playlist.id();
    assert!(run_until(&m.services.runtime, TIMEOUT, || active
        .get()
        .is_some()));
    let id = active.get().unwrap();
    let song = m.vm.selected_songs()[0].id;
    let holds = |m: &Mounted| {
        playlist::load_playlist(&m.services.ctx, id)
            .recv_blocking()
            .unwrap()
            .groups
            .iter()
            .any(|g| g.songs.iter().any(|s| s.song_id == song))
    };
    assert!(!holds(&m));
    let field = m
        .tree
        .find_by_role(teksilo::core::accesskit::Role::TextInput)
        .unwrap();
    let selection = m.vm.selection().get();
    key_down(&mut m.tree, field, Key::Delete);
    key_down(&mut m.tree, field, Key::ArrowDown);
    key_down(&mut m.tree, field, Key::Enter);
    crate::teksilo_ui::test_support::type_text(&mut m.tree, " ");
    assert!(drain_dialog_titles(&mut m.tree).is_empty(), "Delete");
    assert_eq!(m.vm.selection().get(), selection, "the arrows");
    assert_eq!(m.vm.search().get(), " ", "Space types a space");
    // Enter on the list appends the selection within a few ticks
    // (`quirk_2_enter_on_the_list_appends_the_selection_to_the_active_playlist`);
    // from the field it appends nothing.
    assert!(
        !run_until(&m.services.runtime, Duration::from_millis(500), || holds(
            &m
        )),
        "Enter"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn alt_down_in_the_grid_opens_the_selected_album_under_its_row_of_cards() {
    let mut m = mounted();
    let card = m
        .services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    m.vm.grid_selection().select(card.clone());
    let target = m.find("GridView<");
    key_down_with(&mut m.tree, target, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(m.vm.expanded_album().get(), Some(card));
    m.relayout();
    let band = m.node("GridDetail");
    let tile = m.tree.bounds(m.node("AlbumTile"));
    assert_eq!(
        m.tree.bounds(band).y,
        tile.y + crate::teksilo_ui::library::grid::ROW_HEIGHT,
        "directly under the album's row"
    );
    assert!(!all_of_type(&m.tree, band, "SongRow").is_empty());
    key_down_with(&mut m.tree, target, Key::ArrowUp, Modifiers::ALT);
    assert_eq!(m.vm.expanded_album().get(), None);
    m.relayout();
    assert!(all_of_type(&m.tree, m.id, "GridDetail").is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_grid_takes_the_whole_body() {
    let mut m = mounted();
    m.relayout();
    let grid = m.tree.bounds(m.node("LibraryGrid"));
    let body = m.tree.bounds(m.node("ModeBody"));
    assert_eq!(grid, body);
    assert!(all_of_type(&m.tree, m.id, "GridDetail").is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_36_the_column_header_and_the_rows_share_one_offset() {
    let mut m = mounted();
    m.vm.set_view_mode(LibraryViewMode::SimpleList);
    m.relayout();
    let title = find_by_name(&m.tree, m.id, "Title")
        .into_iter()
        .find(|id| {
            m.tree.accessibility_node(*id).role() == teksilo::core::accesskit::Role::ColumnHeader
        })
        .unwrap();
    let row = all_of_type(&m.tree, m.id, "SongRow")[0];
    let (title_x, row_x) = (m.tree.bounds(title).x, m.tree.bounds(row).x);
    let header_area = m.tree.bounds(m.node("ModeHeader"));
    m.tree.dispatch_event(WidgetEvent::scroll_at(
        ScrollDelta::Pixels { x: 0.0, y: -150.0 },
        Modifiers::NONE,
        teksilo::canvas::Point::new(header_area.x + 300.0, header_area.y + 10.0),
    ));
    layout(&mut m.tree);
    assert_eq!(
        m.vm.h_offset().get(),
        150.0,
        "a wheel over the header scrolls sideways"
    );
    assert_eq!(m.tree.bounds(title).x, title_x - 150.0);
    assert_eq!(m.tree.bounds(row).x, row_x - 150.0);
    assert_eq!(
        m.tree.bounds(title).x - m.tree.bounds(row).x,
        title_x - row_x,
        "the columns stay over their values"
    );
    let _ = BOWIE;
    assert!(m.services.ctx.shutdown());
}

#[test]
fn big_counts_are_written_as_iced_writes_them() {
    let big = crate::logic::library_groups::LibraryStats {
        songs: 27_797,
        albums: 2_280,
        artists: 1_043,
        duration_secs: 0.0,
        size_bytes: 0.0,
    };
    assert_eq!(
        plain(&bottom_bar::stats_text(&big).resolve_now()),
        "27797 Songs | 2280 Albums | 1043 Artists | 00:00 | 0.00 MB"
    );
    with_shipped_messages("es-ES", || {
        assert_eq!(
            plain(&bottom_bar::stats_text(&big).resolve_now()),
            "27797 Canciones | 2280 Álbumes | 1043 Artistas | 00:00 | 0.00 MB"
        );
    });
}

// ---- G2: the grid's keys walk the open album's songs, with the keyboard ----

/// [`mounted`], the panel `height` px tall.
fn mounted_tall(height: f32) -> Mounted {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let id = mount_at(&mut tree, LibraryPanel::new(vm.clone()), REGION.0, height);
    settle_layout(&mut tree);
    Mounted {
        services,
        vm,
        tree,
        id,
    }
}

impl Mounted {
    /// The first node whose type name holds `part`.
    fn find(&self, part: &str) -> WidgetId {
        let mut stack = vec![self.id];
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
        panic!("no {part}")
    }

    /// The band's song row reported selected, alone.
    fn selected_band_row(&self) -> WidgetId {
        let band = self.node("GridDetail");
        let rows: Vec<WidgetId> = all_of_type(&self.tree, band, "SongRow")
            .into_iter()
            .filter(|row| self.tree.accessibility_node(*row).is_selected())
            .collect();
        assert_eq!(rows.len(), 1, "one selected row");
        rows[0]
    }

    /// Whether `id` is `ancestor` or under it.
    fn is_within(&self, id: WidgetId, ancestor: WidgetId) -> bool {
        all_of_type(&self.tree, ancestor, "").contains(&id)
    }
}

/// The id of the album with the most songs.
fn longest_album(m: &Mounted) -> String {
    let list = m.services.albums.list();
    let snapshot = m.services.library_songs.snapshot().get();
    (0..list.len())
        .filter_map(|i| list.with_item(i, |c| c.id.clone()))
        .max_by_key(|id| {
            crate::teksilo_ui::library::library_vm::album_songs_in_memory(
                id,
                &snapshot.songs,
                &snapshot.albums,
            )
            .map_or(0, |songs| songs.len())
        })
        .unwrap()
}

/// `key` to whatever has the keyboard.
fn press_key(m: &mut Mounted, key: Key, modifiers: Modifiers) {
    m.tree.dispatch_event(WidgetEvent::KeyDown {
        key,
        modifiers,
        text: None,
    });
    m.relayout();
}

#[test]
fn g2_down_takes_the_keyboard_into_the_open_albums_songs_and_up_brings_it_back() {
    // Iced's walk (`library.rs:808-872`), with the grid's focus: the band
    // takes the keyboard, the current song stays in view.
    let mut m = mounted_tall(420.0);
    let card = longest_album(&m);
    m.vm.toggle_album(&card);
    m.relayout();
    let songs: Vec<i64> = m.vm.expanded_songs().get().iter().map(|s| s.id).collect();
    assert!(songs.len() >= 8, "an album taller than the view");
    let grid = m.find("GridView<");
    key_down(&mut m.tree, grid, Key::ArrowDown);
    m.relayout();
    let band = m.node("GridDetail");
    assert_eq!(m.tree.focused(), Some(band), "the band has the keyboard");
    let song = |id: i64| crate::logic::library_selection::LibraryListItem::Song(id);
    assert_eq!(m.vm.selection().get().focused, Some(song(songs[0])));
    let view = m.tree.bounds(m.node("LibraryGrid"));
    for (row, id) in songs.iter().enumerate().skip(1) {
        press_key(&mut m, Key::ArrowDown, Modifiers::NONE);
        assert_eq!(m.vm.selection().get().focused, Some(song(*id)));
        let current = m.tree.bounds(m.selected_band_row());
        assert!(
            current.y >= view.y - 0.5 && current.y + current.height <= view.y + view.height + 0.5,
            "song {row} in view: {current:?} in {view:?}"
        );
    }
    assert_eq!(m.tree.focused(), Some(band));
    for _ in 1..songs.len() {
        press_key(&mut m, Key::ArrowUp, Modifiers::NONE);
    }
    assert_eq!(m.vm.selection().get().focused, Some(song(songs[0])));
    press_key(&mut m, Key::ArrowUp, Modifiers::NONE);
    let focused = m.tree.focused().unwrap();
    assert_ne!(focused, band);
    assert!(
        m.is_within(focused, grid),
        "the grid has the keyboard again"
    );
    assert_eq!(m.vm.grid_selection().selected_keys(), vec![card.clone()]);
    assert!(m.vm.selection().get().selected.is_empty());
    assert_eq!(m.vm.expanded_album().get(), Some(card), "still open");
    assert!(m.services.ctx.shutdown());
}

#[test]
fn g2_a_click_on_a_band_song_gives_the_band_the_keyboard() {
    let mut m = mounted();
    let card = longest_album(&m);
    m.vm.toggle_album(&card);
    m.relayout();
    let band = m.node("GridDetail");
    let row = all_of_type(&m.tree, band, "SongRow")[1];
    let at = m.tree.bounds(row);
    crate::teksilo_ui::test_support::tap_at(
        &mut m.tree,
        teksilo::canvas::Point::new(at.x + 100.0, at.y + at.height / 2.0),
    );
    assert_eq!(m.tree.focused(), Some(band));
    let songs: Vec<i64> = m.vm.expanded_songs().get().iter().map(|s| s.id).collect();
    press_key(&mut m, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(
        m.vm.selection().get().focused,
        Some(crate::logic::library_selection::LibraryListItem::Song(
            songs[2]
        ))
    );
    assert!(m.services.ctx.shutdown());
}

// ---- G3: the grid's rubber band sweeps the band's songs too ---------------

#[test]
fn g3_a_drag_from_a_card_down_over_the_band_selects_the_card_and_the_songs() {
    let mut m = mounted();
    let list = m.services.albums.list();
    let card = list.with_item(0, |c| c.id.clone()).unwrap();
    let second = list.with_item(1, |c| c.id.clone()).unwrap();
    m.vm.toggle_album(&card);
    m.relayout();
    let tiles = all_of_type(&m.tree, m.id, "AlbumTile");
    let tile = m.tree.bounds(tiles[1]);
    let band = m.node("GridDetail");
    let rows = all_of_type(&m.tree, band, "SongRow");
    let second_row = m.tree.bounds(rows[1]);
    let start = teksilo::canvas::Point::new(tile.x + 60.0, tile.y + 200.0);
    let end = teksilo::canvas::Point::new(tile.x + 90.0, second_row.y + 10.0);
    let phases = [
        (PointerPhase::Down, start, 1_000),
        (
            PointerPhase::Move,
            teksilo::canvas::Point::new(start.x + 10.0, start.y + 20.0),
            1_020,
        ),
        (PointerPhase::Move, end, 1_040),
        (PointerPhase::Up, end, 1_060),
    ];
    for (phase, at, ms) in phases {
        crate::teksilo_ui::test_support::pointer_event(
            &mut m.tree,
            phase,
            at,
            PointerButton::Primary,
            ms,
        );
    }
    let songs: Vec<i64> = m.vm.expanded_songs().get().iter().map(|s| s.id).collect();
    assert_eq!(m.vm.grid_selection().selected_keys(), vec![second]);
    assert_eq!(
        m.vm.selection().get().selected,
        songs[..2]
            .iter()
            .map(|id| crate::logic::library_selection::LibraryListItem::Song(*id))
            .collect()
    );
    assert!(m.services.ctx.shutdown());
}

// ---- review #3: the panel's keys leave its controls theirs -----------------

/// [`simple_with_a_selection`]: a song selected, so Enter and Delete would
/// act on the library.
fn with_a_selection() -> Mounted {
    simple_with_a_selection().0
}

#[test]
fn enter_and_space_press_a_focused_action_of_the_bottom_bar() {
    let mut m = with_a_selection();
    let button = m.named("Change the view");
    key_down(&mut m.tree, button, Key::Enter);
    m.relayout();
    assert_eq!(m.vm.view_mode().get(), LibraryViewMode::Grid, "Enter");
    let button = m.named("Change the view");
    key_down(&mut m.tree, button, Key::Space);
    m.relayout();
    assert_eq!(
        m.vm.view_mode().get(),
        LibraryViewMode::DetailedList,
        "Space"
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn delete_on_a_focused_action_asks_nothing() {
    let mut m = with_a_selection();
    let button = m.named("Play the selection");
    key_down(&mut m.tree, button, Key::Delete);
    assert!(drain_dialog_titles(&mut m.tree).is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_arrows_resize_a_focused_column_grip() {
    let mut m = with_a_selection();
    m.vm.set_view_mode(LibraryViewMode::DetailedList);
    m.relayout();
    let selection = m.vm.selection().get();
    let grip = m.node("ColumnGrip");
    let before = m.vm.column_widths().get();
    key_down(&mut m.tree, grip, Key::ArrowRight);
    let after = m.vm.column_widths().get();
    assert_ne!(after, before, "→ widens the column");
    key_down(&mut m.tree, grip, Key::ArrowLeft);
    assert_eq!(m.vm.column_widths().get(), before, "← narrows it back");
    assert_eq!(m.vm.selection().get(), selection, "the rows did not move");
    assert!(m.services.ctx.shutdown());
}

#[test]
fn enter_on_the_meter_slot_opens_its_popup() {
    // `meter/slot.rs`: Enter or Space on the slot opens the popup.
    let (services, vm) = loaded();
    let meter = crate::teksilo_ui::meter::MeterViewModel::new(&services);
    meter.enabled().set(true);
    let mut tree = tree_for(&services);
    let id = mount_at(
        &mut tree,
        LibraryPanel::new(vm.clone()).meter_slot(crate::teksilo_ui::meter::MeterSlot::new(meter)),
        998.0,
        REGION.1,
    );
    settle_layout(&mut tree);
    vm.set_view_mode(LibraryViewMode::SimpleList);
    settle_layout(&mut tree);
    let song = vm.structure().get().songs[0].id;
    vm.click(
        crate::logic::library_selection::LibraryListItem::Song(song),
        Modifiers::NONE,
        std::time::Instant::now(),
    );
    let slot = find_by_name(&tree, id, "VU meter settings")[0];
    key_down(&mut tree, slot, Key::Enter);
    let _ = tree.render();
    assert_eq!(tree.active_overlays().len(), 1, "the popup opened");
    assert!(services.ctx.shutdown());
}

#[test]
fn the_keys_still_act_on_the_list_and_the_grid() {
    let (mut m, list) = simple_with_a_selection();
    key_down(&mut m.tree, list, Key::Delete);
    assert_eq!(drain_dialog_titles(&mut m.tree).len(), 1, "the list");
    m.vm.set_view_mode(LibraryViewMode::Grid);
    m.relayout();
    let card = m
        .services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    m.vm.grid_selection().select(card);
    let grid = m.find("GridView<");
    key_down(&mut m.tree, grid, Key::Delete);
    assert_eq!(drain_dialog_titles(&mut m.tree).len(), 1, "the grid");
    assert!(m.services.ctx.shutdown());
}

// ---- review #4: the shared offset follows a resize --------------------------

/// The panel alone in a window, so that the window's size is the panel's.
fn windowed() -> Mounted {
    let (services, vm) = loaded();
    let mut tree = tree_for(&services);
    let id = tree.add(LibraryPanel::new(vm.clone()));
    Mounted {
        services,
        vm,
        tree,
        id,
    }
}

impl Mounted {
    /// Lays the window out `width` px wide, as a resize does.
    fn resize(&mut self, width: f32) {
        let size = SizeProposal::exact(width, REGION.1);
        self.tree.layout(size);
        self.tree.layout(size);
    }

    /// The column header's "Title" cell.
    fn title_header(&self) -> WidgetId {
        find_by_name(&self.tree, self.id, "Title")
            .into_iter()
            .find(|id| {
                self.tree.accessibility_node(*id).role()
                    == teksilo::core::accesskit::Role::ColumnHeader
            })
            .unwrap()
    }

    /// A wheel notch sideways over the column header.
    fn wheel_header(&mut self, dx: f32) {
        let header = self.tree.bounds(self.node("ModeHeader"));
        self.tree.dispatch_event(WidgetEvent::scroll_at(
            ScrollDelta::Pixels { x: dx, y: 0.0 },
            Modifiers::NONE,
            teksilo::canvas::Point::new(header.x + 100.0, header.y + 10.0),
        ));
    }
}

#[test]
fn quirk_36_a_wider_window_brings_the_leading_columns_back() {
    let mut m = windowed();
    m.vm.set_view_mode(LibraryViewMode::DetailedList);
    let content = crate::teksilo_ui::library::library_vm::content_width(
        LibraryViewMode::DetailedList,
        &m.vm.column_widths().get(),
    );
    let narrow = content - 300.0;
    m.resize(narrow);
    let title_x = m.tree.bounds(m.title_header()).x;
    m.wheel_header(-10_000.0);
    m.resize(narrow);
    assert_eq!(m.vm.h_offset().get(), 300.0, "scrolled to the end");
    // Maximized: the content fits, nothing is left to scroll.
    m.resize(content + 100.0);
    assert_eq!(m.vm.h_offset().get(), 0.0);
    assert_eq!(m.tree.bounds(m.title_header()).x, title_x, "back in place");
    // Part of the way: the offset keeps only what still overflows.
    m.resize(narrow);
    m.wheel_header(-10_000.0);
    m.resize(content - 100.0);
    assert_eq!(m.vm.h_offset().get(), 100.0);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_36_narrowing_a_column_keeps_the_offset_within_the_content() {
    let mut m = windowed();
    m.vm.set_view_mode(LibraryViewMode::DetailedList);
    let content = |m: &Mounted| {
        crate::teksilo_ui::library::library_vm::content_width(
            LibraryViewMode::DetailedList,
            &m.vm.column_widths().get(),
        )
    };
    let width = content(&m) - 50.0;
    m.resize(width);
    m.wheel_header(-10_000.0);
    m.resize(width);
    assert_eq!(m.vm.h_offset().get(), 50.0);
    // ← on a grip narrows its column, with no layout at another width.
    let grip = m.node("ColumnGrip");
    m.tree.focus(grip);
    m.tree.dispatch_event(WidgetEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers::NONE,
        text: None,
    });
    let left = (content(&m) - width).max(0.0);
    assert!(left < 50.0, "the column narrowed: {left}");
    assert_eq!(m.vm.h_offset().get(), left, "before any layout");
    m.resize(width);
    assert_eq!(m.vm.h_offset().get(), left);
    assert!(m.services.ctx.shutdown());
}

// ---- review #6: a reveal gives the library the keyboard ---------------------

#[test]
fn a_reveal_in_a_list_mode_gives_the_list_the_keyboard() {
    let (mut m, list) = simple_with_a_selection();
    let field = m
        .tree
        .find_by_role(teksilo::core::accesskit::Role::TextInput)
        .unwrap();
    m.tree.focus(field);
    let path = m.vm.structure().get().songs[3].full_file_path.to_string();
    m.vm.reveal_path(&path);
    m.relayout();
    assert_eq!(m.tree.focused(), Some(list));
    assert!(m.services.ctx.shutdown());
}
