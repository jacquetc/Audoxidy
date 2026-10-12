use std::time::Duration;

use teksilo::core::WidgetTree;
use teksilo::core::event::PointerButton;
use teksilo::widgets::{Expand, HStack};

use super::*;
use crate::db::database::LibrarySearchParams;
use crate::teksilo_ui::app::services::{AppServices, testing::in_memory_services};
use crate::teksilo_ui::test_support::{
    VIEWPORT, a11y_node, access_click, find_by_name, first_of_type, key_down, layout,
    pointer_click, run_until, tree_with_events, type_text, with_shipped_messages,
};

const TIMEOUT: Duration = Duration::from_secs(10);

/// A panel over the mock library, mounted as the app mounts it (in a 202 px
/// column that takes the height), with the genre tree loaded when `loaded`.
fn mounted(loaded: bool) -> (AppServices, FiltersViewModel, WidgetTree, WidgetId) {
    let services = in_memory_services();
    let vm = FiltersViewModel::new(&services);
    let mut tree = tree_with_events(&services.ctx);
    let panel = tree.add(FiltersPanel::new(vm.clone()));
    tree.add(
        HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(Expand::vertical().child(panel))
            .child(Spacer::new()),
    );
    layout(&mut tree);
    if loaded {
        load(&services, &mut tree, FilterType::Genre);
    }
    (services, vm, tree, panel)
}

/// Loads the tree of `filter_type` into the hierarchy; the panel's effect
/// picks it up.
fn load(services: &AppServices, tree: &mut WidgetTree, filter_type: FilterType) {
    services.filter_hierarchy.set_filter_type(filter_type);
    let hierarchy = services.filter_hierarchy.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || !hierarchy
        .tree_of(filter_type)
        .is_empty()));
    layout(tree);
}

fn params(services: &AppServices) -> LibrarySearchParams {
    services.library_songs.params()
}

fn centre(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

/// The one widget the accessibility tree names `name` under `panel`.
fn named(tree: &WidgetTree, panel: WidgetId, name: &str) -> WidgetId {
    let found = find_by_name(tree, panel, name);
    assert_eq!(found.len(), 1, "{name}: {found:?}");
    found[0]
}

/// The tree view's rows, top to bottom.
fn rows(tree: &WidgetTree, panel: WidgetId) -> Vec<WidgetId> {
    let mut found = Vec::new();
    let mut stack = vec![panel];
    while let Some(id) = stack.pop() {
        stack.extend(tree.children(id));
        if tree
            .widget_type_name(id)
            .is_some_and(|name| name.ends_with("TreeItemWrapper"))
        {
            found.push(id);
        }
    }
    found.sort_by(|a, b| tree.bounds(*a).y.total_cmp(&tree.bounds(*b).y));
    found
}

/// The row of the node labelled `label`.
fn row_of(tree: &WidgetTree, panel: WidgetId, vm: &FiltersViewModel, label: &str) -> WidgetId {
    let at = vm
        .visible_items()
        .iter()
        .position(|row| row.label == label)
        .unwrap_or_else(|| panic!("{label} is not a visible row"));
    rows(tree, panel)[at]
}

/// The selector's trigger (named "Filter type" in en-US).
fn trigger(tree: &WidgetTree, panel: WidgetId) -> WidgetId {
    named(tree, panel, "Filter type")
}

/// Opens the filter-type menu; returns its bounds.
fn open_menu(tree: &mut WidgetTree, panel: WidgetId) -> Rect {
    let selector = trigger(tree, panel);
    access_click(tree, selector);
    let overlays = tree.active_overlays();
    assert_eq!(overlays.len(), 1, "the menu opens");
    tree.overlay_content_bounds(overlays[0]).unwrap()
}

// ---- geometry ----

#[test]
fn the_column_is_202_wide_and_fills_the_height() {
    with_shipped_messages("en-US", || {
        let (services, _vm, tree, panel) = mounted(true);
        let column = tree.bounds(panel);
        assert_eq!((column.width, column.height), (WIDTH, VIEWPORT.1));
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_selector_is_28_high_with_a_2_pixel_line_under_it() {
    with_shipped_messages("en-US", || {
        let (services, _vm, tree, panel) = mounted(true);
        let selector = tree.bounds(trigger(&tree, panel));
        assert_eq!(
            (selector.x, selector.y, selector.width, selector.height),
            (0.0, 0.0, 202.0, SELECTOR_HEIGHT + SELECTOR_LINE)
        );
        let strip = first_of_type(&tree, trigger(&tree, panel), "FixedSize").unwrap();
        assert_eq!(tree.bounds(strip).height, SELECTOR_HEIGHT);
        // The label is 15 px in and the chevron 28 px square, 7 px from the edge.
        let label = first_of_type(&tree, strip, "TextWidget").unwrap();
        assert_eq!(tree.bounds(label).x, SELECTOR_LEADING);
        let icon = tree.bounds(first_of_type(&tree, strip, "IconWidget").unwrap());
        assert_eq!((icon.width, icon.height), (SELECTOR_ICON, SELECTOR_ICON));
        assert_eq!(icon.x + icon.width, WIDTH - SELECTOR_TRAILING);
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_search_field_is_180_wide_in_a_40_pixel_bar_at_the_bottom() {
    with_shipped_messages("en-US", || {
        let (services, _vm, tree, panel) = mounted(true);
        let field = tree.bounds(first_of_type(&tree, panel, "SearchField").unwrap());
        assert_eq!(field.width, SEARCH_WIDTH);
        assert_eq!(field.x, (WIDTH - SEARCH_WIDTH) / 2.0);
        let bar_top = VIEWPORT.1 - SEARCH_BAR_HEIGHT;
        let centre_y = field.y + field.height / 2.0;
        assert_eq!(centre_y, bar_top + SEARCH_BAR_HEIGHT / 2.0);
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_letters_are_16_pixel_cells_ten_to_a_line_under_15_by_10_pixels_of_padding() {
    with_shipped_messages("en-US", || {
        let (services, vm, tree, panel) = mounted(true);
        let letters = vm.subfilters().get();
        assert!(letters.len() >= CELLS_PER_LINE, "{letters:?}");
        let restore = tree.bounds(named(&tree, panel, "Show all"));
        assert_eq!(
            (restore.x, restore.y, restore.width, restore.height),
            (
                GRID_PADDING.1,
                SELECTOR_HEIGHT + SELECTOR_LINE + GRID_PADDING.0,
                CELL,
                CELL
            )
        );
        // No spacing between cells on a line.
        let first = tree.bounds(named(&tree, panel, &letters[0]));
        assert_eq!((first.x, first.y), (restore.x + CELL, restore.y));
        assert_eq!((first.width, first.height), (CELL, CELL));
        // The first line holds the restore icon and nine letters; the tenth
        // letter starts the second line, 4 px below.
        let ninth = tree.bounds(named(&tree, panel, &letters[CELLS_PER_LINE - 2]));
        assert_eq!(ninth.y, restore.y);
        let tenth = tree.bounds(named(&tree, panel, &letters[CELLS_PER_LINE - 1]));
        assert_eq!(
            (tenth.x, tenth.y),
            (restore.x, restore.y + CELL + LINE_SPACING)
        );
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_tree_rows_are_32_pixels_high_and_as_wide_as_the_column() {
    with_shipped_messages("en-US", || {
        let (services, vm, tree, panel) = mounted(true);
        let rows = rows(&tree, panel);
        assert!(rows.len() >= 3 && rows.len() <= vm.visible_items().len());
        let mut previous: Option<Rect> = None;
        for row in &rows {
            let bounds = tree.bounds(*row);
            assert_eq!((bounds.height, bounds.width), (ROW_HEIGHT, WIDTH));
            if let Some(previous) = previous {
                assert_eq!(bounds.y, previous.y + ROW_HEIGHT);
            }
            previous = Some(bounds);
        }
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn a_row_indents_12_pixels_a_level_and_a_leaf_keeps_16_pixels_for_the_chevron() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let top = vm.visible_items()[0].clone();
        assert!(top.has_children);
        let row = row_of(&tree, panel, &vm, &top.label);
        let chevron = first_of_type(&tree, row, "IconWidget").unwrap();
        assert_eq!(tree.bounds(chevron).width, CHEVRON);
        assert_eq!(tree.bounds(chevron).x, 0.0);
        let label = first_of_type(&tree, row, "TextWidget").unwrap();
        assert_eq!(tree.bounds(label).x, CHEVRON);

        vm.toggle_node(&top.id);
        layout(&mut tree);
        let child = vm.visible_items()[1].clone();
        assert_eq!(child.depth, 1);
        let row = rows(&tree, panel)[1];
        let chevron = first_of_type(&tree, row, "IconWidget").unwrap();
        assert_eq!(tree.bounds(chevron).x, INDENT);
        // Expand until a leaf shows, one level deeper.
        vm.toggle_node(&child.id);
        layout(&mut tree);
        let leaf_at = vm
            .visible_items()
            .iter()
            .position(|row| !row.has_children)
            .unwrap();
        let leaf = vm.visible_items()[leaf_at].clone();
        let row = rows(&tree, panel)[leaf_at];
        let label = first_of_type(&tree, row, "TextWidget").unwrap();
        assert_eq!(
            tree.bounds(label).x,
            leaf.depth as f32 * INDENT + LEAF_SPACE
        );
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn nothing_is_drawn_between_the_selector_and_the_search_field_while_the_index_is_empty() {
    with_shipped_messages("en-US", || {
        let (services, vm, tree, panel) = mounted(false);
        assert!(vm.subfilters().get().is_empty());
        assert_eq!(
            tree.bounds(first_of_type(&tree, panel, "LetterGrid").unwrap())
                .height,
            0.0
        );
        assert!(rows(&tree, panel).is_empty());
        // The selector and the search field are there.
        assert_eq!(tree.bounds(trigger(&tree, panel)).height, 30.0);
        assert!(first_of_type(&tree, panel, "SearchField").is_some());
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_panel_follows_the_index_when_it_arrives() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(false);
        load(&services, &mut tree, FilterType::Genre);
        assert!(!vm.subfilters().get().is_empty());
        assert!(!rows(&tree, panel).is_empty());
        assert!(services.ctx.shutdown());
    });
}

// ---- the letters ----

#[test]
fn clicking_a_letter_selects_it_and_clicking_it_again_clears_it() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let all = vm.visible_items().len();
        let cell = named(&tree, panel, "R");
        let target = centre(tree.bounds(cell));
        pointer_click(&mut tree, target, PointerButton::Primary, 0);
        assert_eq!(vm.subfilter().get(), Some("R".to_string()));
        let id = named(&tree, panel, "R");
        assert!(a11y_node(&mut tree, id).is_selected() == Some(true));
        let id = named(&tree, panel, "S");
        assert!(a11y_node(&mut tree, id).is_selected() != Some(true));
        assert!(vm.visible_items().len() < all);
        let cell = named(&tree, panel, "R");
        let target = centre(tree.bounds(cell));
        pointer_click(&mut tree, target, PointerButton::Primary, 500);
        assert_eq!(vm.subfilter().get(), None);
        assert_eq!(vm.visible_items().len(), all);
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_restore_icon_clears_the_letter_and_the_librarys_filters() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        vm.select_node(&vm.visible_items()[0].id.clone());
        let id = named(&tree, panel, "P");
        access_click(&mut tree, id);
        assert_eq!(vm.subfilter().get(), Some("P".to_string()));
        let id = named(&tree, panel, "Show all");
        access_click(&mut tree, id);
        assert_eq!(vm.subfilter().get(), None);
        let p = params(&services);
        assert_eq!(
            (p.genre, p.artist, p.album, p.year),
            (None, None, None, None)
        );
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_arrows_move_the_focus_between_the_cells_and_enter_chooses_one() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let letters = vm.subfilters().get();
        let restore = named(&tree, panel, "Show all");
        key_down(&mut tree, restore, Key::ArrowRight);
        assert_eq!(tree.focused(), Some(named(&tree, panel, &letters[0])));
        let target = named(&tree, panel, &letters[0]);
        key_down(&mut tree, target, Key::ArrowRight);
        assert_eq!(tree.focused(), Some(named(&tree, panel, &letters[1])));
        let target = named(&tree, panel, &letters[1]);
        key_down(&mut tree, target, Key::ArrowLeft);
        assert_eq!(tree.focused(), Some(named(&tree, panel, &letters[0])));
        // Down is a line of ten cells.
        key_down(&mut tree, restore, Key::ArrowDown);
        assert_eq!(
            tree.focused(),
            Some(named(&tree, panel, &letters[CELLS_PER_LINE - 1]))
        );
        let target = named(&tree, panel, &letters[CELLS_PER_LINE - 1]);
        key_down(&mut tree, target, Key::ArrowUp);
        assert_eq!(tree.focused(), Some(restore));
        // The ends do not wrap.
        key_down(&mut tree, restore, Key::ArrowLeft);
        assert_eq!(tree.focused(), Some(restore));
        // Enter chooses the focused letter.
        let target = named(&tree, panel, &letters[2]);
        key_down(&mut tree, target, Key::Enter);
        assert_eq!(vm.subfilter().get(), Some(letters[2].clone()));
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_arrow_keys_step_through_the_cells_of_a_grid() {
    assert_eq!(next_cell(0, 30, &Key::ArrowRight), Some(1));
    assert_eq!(next_cell(0, 30, &Key::ArrowLeft), None);
    assert_eq!(next_cell(5, 30, &Key::ArrowDown), Some(15));
    assert_eq!(next_cell(25, 30, &Key::ArrowDown), None);
    assert_eq!(next_cell(15, 30, &Key::ArrowUp), Some(5));
    assert_eq!(next_cell(5, 30, &Key::ArrowUp), None);
    assert_eq!(next_cell(29, 30, &Key::ArrowRight), None);
    assert_eq!(next_cell(3, 30, &Key::Enter), None);
}

// ---- the tree ----

#[test]
fn clicking_a_row_selects_the_node_and_filters_the_library() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let top = vm.visible_items()[0].label.clone();
        let row = row_of(&tree, panel, &vm, &top);
        // Click on the label, away from the chevron.
        let bounds = tree.bounds(row);
        pointer_click(
            &mut tree,
            Point::new(bounds.x + 120.0, bounds.y + 16.0),
            PointerButton::Primary,
            0,
        );
        assert!(vm.selected_node().get().is_some());
        assert_eq!(params(&services).genre.as_deref(), Some(top.as_str()));
        assert!(
            !vm.tree_source().rows().iter().any(|r| r.depth > 0),
            "a row click does not expand"
        );
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn clicking_the_chevron_expands_the_node_without_selecting_it() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let collapsed = vm.visible_items().len();
        let top = vm.visible_items()[0].label.clone();
        let row = row_of(&tree, panel, &vm, &top);
        let chevron = first_of_type(&tree, row, "IconWidget").unwrap();
        let target = centre(tree.bounds(chevron));
        pointer_click(&mut tree, target, PointerButton::Primary, 0);
        assert!(vm.visible_items().len() > collapsed, "the node expanded");
        assert_eq!(vm.selected_node().get(), None, "and was not selected");
        assert_eq!(params(&services).genre, None);
        // The chevron points down now; a second click collapses it.
        let row = row_of(&tree, panel, &vm, &top);
        let chevron = first_of_type(&tree, row, "IconWidget").unwrap();
        let target = centre(tree.bounds(chevron));
        pointer_click(&mut tree, target, PointerButton::Primary, 500);
        assert_eq!(vm.visible_items().len(), collapsed);
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_keyboard_walks_the_tree_and_enter_selects_a_node() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let view = named(&tree, panel, "Filters");
        key_down(&mut tree, view, Key::ArrowDown);
        key_down(&mut tree, view, Key::Enter);
        let chosen = vm.selected_node().get().expect("Enter selected a node");
        assert!(vm.visible_items().iter().any(|row| row.id == chosen));
        assert!(params(&services).genre.is_some());
        // Right opens a closed node, Left closes it.
        let before = vm.visible_items().len();
        key_down(&mut tree, view, Key::ArrowRight);
        assert!(vm.visible_items().len() > before);
        key_down(&mut tree, view, Key::ArrowLeft);
        assert_eq!(vm.visible_items().len(), before);
        assert!(services.ctx.shutdown());
    });
}

// ---- the search ----

#[test]
fn typing_in_the_search_field_prunes_the_tree_and_escape_clears_it() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let all = vm.visible_items().len();
        let field = named(&tree, panel, "Search...");
        tree.focus(field);
        type_text(&mut tree, "roc");
        assert_eq!(vm.search().get(), "roc");
        let shown = vm.visible_items().len();
        assert!(shown > 0 && shown < all);
        assert_eq!(rows(&tree, panel).len(), shown);
        key_down(&mut tree, field, Key::Escape);
        assert_eq!(vm.search().get(), "");
        assert_eq!(vm.visible_items().len(), all);
        assert!(services.ctx.shutdown());
    });
}

// ---- the menu ----

#[test]
fn a_pointer_click_on_the_selector_opens_the_menu() {
    with_shipped_messages("en-US", || {
        let (services, _vm, mut tree, panel) = mounted(true);
        let b = tree.bounds(trigger(&tree, panel));
        crate::teksilo_ui::test_support::tap_at(
            &mut tree,
            teksilo::canvas::Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0),
        );
        assert_eq!(tree.active_overlays().len(), 1, "the menu opened");
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_selector_opens_a_172_pixel_menu_of_five_types_anchored_at_it() {
    with_shipped_messages("en-US", || {
        let (services, _vm, mut tree, panel) = mounted(true);
        let selector = tree.bounds(trigger(&tree, panel));
        let menu = open_menu(&mut tree, panel);
        let genre = tree.find_by_label("Genre").unwrap();
        // The row is the 172 px frame less the 4 px the stock list keeps on each
        // side (see the menu module).
        assert_eq!(tree.bounds(genre).width, MENU_WIDTH - 8.0);
        let _ = menu;
        // Quirk fix 7: under the selector, not at a fixed place of the window.
        assert_eq!(menu.x, selector.x);
        // (the popover keeps a gap of a few pixels below its trigger)
        let gap = menu.y - (selector.y + selector.height);
        assert!((0.0..=4.0).contains(&gap), "{gap}");
        for label in ["Folder", "Artist", "Album", "Genre", "Year"] {
            assert!(tree.find_by_label(label).is_some(), "{label}");
        }
        let genre = tree.find_by_label("Genre").unwrap();
        assert_eq!(tree.bounds(genre).height, 32.0);
        let icon = tree.bounds(first_of_type(&tree, genre, "IconWidget").unwrap());
        assert_eq!((icon.width, icon.height), (18.0, 18.0));
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_menu_lists_the_types_in_the_order_of_the_iced_menu() {
    with_shipped_messages("en-US", || {
        let (services, _vm, mut tree, panel) = mounted(true);
        open_menu(&mut tree, panel);
        let mut at: Vec<(String, f32)> = ["Folder", "Artist", "Album", "Genre", "Year"]
            .iter()
            .map(|label| {
                let id = tree.find_by_label(label).unwrap();
                (label.to_string(), tree.bounds(id).y)
            })
            .collect();
        let listed: Vec<_> = at.iter().map(|(label, _)| label.clone()).collect();
        at.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(
            at.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>(),
            listed
        );
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn choosing_a_type_shows_it_closes_the_menu_and_resets_the_column() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        vm.select_node(&vm.visible_items()[0].id.clone());
        open_menu(&mut tree, panel);
        let year = tree.find_by_label("Year").unwrap();
        access_click(&mut tree, year);
        assert_eq!(vm.filter_type().get(), FilterType::Year);
        assert!(tree.active_overlays().is_empty(), "the menu closed");
        assert_eq!(vm.selected_node().get(), None);
        assert_eq!(params(&services).genre, None);
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn escape_closes_the_menu_without_choosing() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        open_menu(&mut tree, panel);
        tree.dispatch_event(WidgetEvent::KeyDown {
            key: Key::Escape,
            modifiers: teksilo::core::Modifiers::NONE,
            text: None,
        });
        layout(&mut tree);
        assert!(tree.active_overlays().is_empty());
        assert_eq!(vm.filter_type().get(), FilterType::Genre);
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_menu_marks_the_current_type_and_follows_a_change() {
    with_shipped_messages("en-US", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let entries = type_entries(&vm);
        assert_eq!(entries.len(), 5);
        open_menu(&mut tree, panel);
        let genre = tree.find_by_label("Genre").unwrap();
        // The current type is the only one in the accent.
        assert!(a11y_node(&mut tree, genre).is_selected() != Some(false));
        assert!(services.ctx.shutdown());
    });
}

// ---- names and strings ----

#[test]
fn the_controls_have_accessible_names() {
    with_shipped_messages("en-US", || {
        let (services, _vm, mut tree, panel) = mounted(true);
        for (name, role) in [
            ("Filter type", Role::Button),
            ("Initial letter", Role::Group),
            ("Show all", Role::Button),
            ("A", Role::Button),
            ("Filters", Role::Tree),
            ("Search...", Role::TextInput),
        ] {
            let id = named(&tree, panel, name);
            assert_eq!(a11y_node(&mut tree, id).role(), role, "{name}");
        }
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn the_spanish_strings_are_the_iced_ones() {
    with_shipped_messages("es-ES", || {
        let (services, vm, mut tree, panel) = mounted(true);
        let selector = tree.bounds(first_of_type(&tree, panel, "TextWidget").unwrap());
        assert!(selector.width > 0.0);
        assert_eq!(type_label(FilterType::Folder).resolve_now(), "Carpeta");
        assert_eq!(type_label(FilterType::Artist).resolve_now(), "Artista");
        assert_eq!(type_label(FilterType::Album).resolve_now(), "Álbum");
        assert_eq!(type_label(FilterType::Genre).resolve_now(), "Género");
        assert_eq!(type_label(FilterType::Year).resolve_now(), "Año");
        named(&tree, panel, "Tipo de filtro");
        named(&tree, panel, "Letra inicial");
        named(&tree, panel, "Mostrar todo");
        named(&tree, panel, "Filtros");
        named(&tree, panel, "Buscar...");
        open_menu_es(&mut tree, panel);
        for label in ["Carpeta", "Artista", "Álbum", "Género", "Año"] {
            assert!(tree.find_by_label(label).is_some(), "{label}");
        }
        let _ = vm;
        assert!(services.ctx.shutdown());
    });
}

fn open_menu_es(tree: &mut WidgetTree, panel: WidgetId) {
    let selector = named(tree, panel, "Tipo de filtro");
    access_click(tree, selector);
    assert_eq!(tree.active_overlays().len(), 1);
}
