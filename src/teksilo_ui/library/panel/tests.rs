use std::time::Duration;

use teksilo::core::WidgetTree;
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

#[test]
fn quirk_10_the_meter_slot_keeps_400_px_without_a_meter() {
    let m = mounted();
    let search = m.tree.bounds(m.node("SearchField"));
    assert_eq!(search.width, bottom_bar::SEARCH_WIDTH);
    let play = m.tree.bounds(m.named("Play the selection"));
    let stats = m.tree.bounds(m.node("Stats"));
    // search, 15, the figures, 10, the 400 px slot, 10, the actions.
    assert_eq!(stats.x - (search.x + search.width), 15.0);
    assert_eq!(
        play.x - (stats.x + stats.width),
        10.0 + METER_SLOT_WIDTH + 10.0
    );
    assert!(m.services.ctx.shutdown());
}

#[test]
fn quirk_10_the_meter_fills_its_slot() {
    let m = mounted_with(|vm| LibraryPanel::new(vm).meter_slot(FakeMeter));
    let meter = m.tree.bounds(m.node("FakeMeter"));
    assert_eq!(
        (meter.width, meter.height),
        (METER_SLOT_WIDTH, bottom_bar::HEIGHT)
    );
    let stats = m.tree.bounds(m.node("Stats"));
    assert_eq!(meter.x - (stats.x + stats.width), 10.0);
    let _ = Spacer::new();
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
fn quirk_2_the_search_field_keeps_enter_delete_and_the_arrows() {
    let (mut m, _list) = simple_with_a_selection();
    let field = m
        .tree
        .find_by_role(teksilo::core::accesskit::Role::TextInput)
        .unwrap();
    let selection = m.vm.selection().get();
    key_down(&mut m.tree, field, Key::Delete);
    key_down(&mut m.tree, field, Key::ArrowDown);
    key_down(&mut m.tree, field, Key::Enter);
    assert!(drain_dialog_titles(&mut m.tree).is_empty());
    assert_eq!(m.vm.selection().get(), selection);
    assert!(m.services.ctx.shutdown());
}

#[test]
fn alt_down_in_the_grid_opens_the_selected_album_in_the_detail_panel() {
    let mut m = mounted();
    let card = m
        .services
        .albums
        .list()
        .with_item(0, |c| c.id.clone())
        .unwrap();
    m.vm.grid_selection().select(card.clone());
    let grid = m.node("LibraryGrid");
    let target = m.tree.children(grid)[0];
    key_down_with(&mut m.tree, target, Key::ArrowDown, Modifiers::ALT);
    assert_eq!(m.vm.expanded_album().get(), Some(card));
    // The pane slides open: pump frames until it has (at most a second).
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        m.tree.frame_tick().set(0.016);
        m.relayout();
        if m.tree.bounds(m.node("GridDetail")).height >= MIN_PANE
            || std::time::Instant::now() > deadline
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let detail = m.tree.bounds(m.node("GridDetail"));
    assert!(
        detail.height >= MIN_PANE,
        "the detail pane shows: {detail:?}"
    );
    assert!(!all_of_type(&m.tree, m.node("GridDetail"), "SongRow").is_empty());
    assert!(m.services.ctx.shutdown());
}

#[test]
fn the_detail_pane_is_hidden_while_no_album_is_open() {
    let mut m = mounted();
    m.relayout();
    let grid = m.tree.bounds(m.node("LibraryGrid"));
    let body = m.tree.bounds(m.node("ModeBody"));
    assert_eq!(grid.height, body.height, "the grid takes the whole body");
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
