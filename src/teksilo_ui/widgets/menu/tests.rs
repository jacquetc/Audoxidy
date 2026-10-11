use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::event::PointerButton;
use teksilo::core::{Key, Modifiers, WidgetTree};
use teksilo::widgets::{FixedSize, Spacer, VStack};

use super::*;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::test_support::{access_click, first_of_type, layout};

/// A tree in the Audoxidy theme with the menu surface installed.
fn tree() -> WidgetTree {
    let mut theme = style::audoxidy_dark();
    install(&mut theme.style_slots);
    let mut tree = WidgetTree::new();
    tree.set_theme(theme);
    tree
}

/// A counter and an action that adds `amount` to it.
fn counter() -> Rc<Cell<u32>> {
    Rc::new(Cell::new(0))
}

fn adds(count: &Rc<Cell<u32>>, amount: u32) -> impl Fn(&mut EventContext) + 'static {
    let count = count.clone();
    move |_| count.set(count.get() + amount)
}

/// The playlist row menu's first entries (`playlist.rs:1036-1060`): two
/// commands around a divider, then a disabled stub.
fn row_entries(count: &Rc<Cell<u32>>) -> Vec<MenuEntry> {
    vec![
        MenuEntry::item(lit!("Reproducir"))
            .icon(icons::window::close)
            .on_activate(adds(count, 1)),
        MenuEntry::divider(),
        MenuEntry::item(lit!("Agregar archivo"))
            .icon(icons::window::close)
            .on_activate(adds(count, 10)),
        MenuEntry::item(lit!("Información"))
            .icon(icons::window::close)
            .on_activate(adds(count, 100))
            .enabled(false),
        MenuEntry::item(lit!("Mostrar en la biblioteca")).on_activate(adds(count, 1000)),
    ]
}

fn mounted(widget: impl Widget + 'static) -> (WidgetTree, WidgetId) {
    let mut tree = tree();
    let id = tree.add(widget);
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(id)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    (tree, id)
}

#[test]
fn the_context_menu_is_as_wide_as_its_longest_label_allows() {
    assert_eq!(context_menu_width(0), 80.0);
    assert_eq!(context_menu_width(3), 80.0);
    // "Mostrar en la biblioteca": 24 characters.
    assert_eq!(context_menu_width(24), 232.0);
    let count = counter();
    let (mut tree, at) = with_context_menu(&count);
    right_click(&mut tree, at);
    let overlay = tree.active_overlays()[0];
    assert_eq!(tree.overlay_content_bounds(overlay).unwrap().width, 232.0);
}

#[test]
fn context_rows_are_32_pixels_with_the_icon_and_label_where_iced_puts_them() {
    let count = counter();
    let (tree, id) = mounted(context_menu(row_entries(&count)));
    let menu = tree.bounds(id);
    let play = tree.find_by_label("Reproducir").unwrap();
    let row = tree.bounds(play);
    assert_eq!(row.height, ROW_HEIGHT);
    // 5 px of padding above the first row, as in Iced.
    assert_eq!(row.y - menu.y, 5.0);
    let icon = first_of_type(&tree, play, "IconWidget").unwrap();
    let icon = tree.bounds(icon);
    assert_eq!((icon.width, icon.height), (24.0, 24.0));
    // 1 px of frame padding and 10 px into the row.
    assert_eq!(icon.x - menu.x, 11.0);
    let label = first_of_type(&tree, play, "MenuLabel")
        .or_else(|| first_of_type(&tree, play, "TextWidget"))
        .unwrap();
    assert_eq!(tree.bounds(label).x - menu.x, 11.0 + 24.0 + 15.0);
}

#[test]
fn a_divider_is_one_pixel_between_two_rows() {
    let count = counter();
    let (tree, id) = mounted(context_menu(row_entries(&count)));
    let divider = first_of_type(&tree, id, "MenuDivider").unwrap();
    let divider = tree.bounds(divider);
    assert_eq!(divider.height, 1.0);
    let play = tree.bounds(tree.find_by_label("Reproducir").unwrap());
    let add = tree.bounds(tree.find_by_label("Agregar archivo").unwrap());
    assert_eq!(divider.y, play.y + play.height);
    assert_eq!(add.y, divider.y + 1.0);
}

#[test]
fn the_renombrar_lista_icon_sits_one_pixel_further_out() {
    let (tree, id) = mounted(context_menu(vec![
        MenuEntry::item(lit!("Renombrar lista"))
            .icon(icons::window::close)
            .icon_shift(1.0),
    ]));
    let menu = tree.bounds(id);
    let row = tree.find_by_label("Renombrar lista").unwrap();
    let icon = tree.bounds(first_of_type(&tree, row, "IconWidget").unwrap());
    assert_eq!(icon.x - menu.x, 10.0);
}

#[test]
fn a_row_is_lit_while_hovered_or_highlighted_but_not_pressed() {
    assert_eq!(row_fill(false, false), SurfaceRole::Main);
    assert_eq!(row_fill(true, false), SurfaceRole::Raised);
    assert_eq!(row_fill(true, true), SurfaceRole::Main);
}

/// A 100 × 100 target opening the row menu on a right-click.
fn with_context_menu(count: &Rc<Cell<u32>>) -> (WidgetTree, Point) {
    let mut tree = tree();
    let entries = count.clone();
    let target = tree.add(
        FixedSize::new()
            .width(100.0)
            .height(100.0)
            .child(Spacer::new())
            .context_menu(move |_, _| Some(Box::new(context_menu(row_entries(&entries))))),
    );
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(target)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    (tree, Point::new(50.0, 50.0))
}

fn right_click(tree: &mut WidgetTree, at: Point) {
    tree.pointer_down_button(at, PointerButton::Secondary);
    tree.pointer_up_button(at, PointerButton::Secondary);
    layout(tree);
}

fn press(tree: &mut WidgetTree, key: Key) {
    tree.press_key(key, Modifiers::NONE);
    layout(tree);
}

#[test]
fn a_right_click_opens_the_menu_at_the_pointer() {
    let count = counter();
    let (mut tree, at) = with_context_menu(&count);
    right_click(&mut tree, at);
    let overlays = tree.active_overlays();
    assert_eq!(overlays.len(), 1);
    let bounds = tree.overlay_content_bounds(overlays[0]).unwrap();
    assert_eq!((bounds.x, bounds.y), (at.x, at.y));
}

#[test]
fn the_arrows_skip_the_divider_and_enter_activates() {
    let count = counter();
    let (mut tree, at) = with_context_menu(&count);
    right_click(&mut tree, at);
    press(&mut tree, Key::ArrowDown);
    press(&mut tree, Key::ArrowDown);
    press(&mut tree, Key::Enter);
    assert_eq!(count.get(), 10, "the second entry, past the divider");
    assert!(
        tree.active_overlays().is_empty(),
        "activation closes the menu"
    );
}

#[test]
fn escape_closes_the_menu_without_activating() {
    let count = counter();
    let (mut tree, at) = with_context_menu(&count);
    right_click(&mut tree, at);
    press(&mut tree, Key::Escape);
    assert!(tree.active_overlays().is_empty());
    assert_eq!(count.get(), 0);
}

#[test]
fn a_disabled_stub_is_greyed_and_does_nothing() {
    let count = counter();
    let (mut tree, at) = with_context_menu(&count);
    right_click(&mut tree, at);
    let stub = tree.find_by_label("Información").unwrap();
    assert!(tree.accessibility_node(stub).is_disabled());
    access_click(&mut tree, stub);
    tree.click(stub);
    layout(&mut tree);
    assert_eq!(count.get(), 0);
}

#[test]
fn the_send_to_entry_opens_a_submenu_of_playlists() {
    let count = counter();
    let lists = count.clone();
    let mut tree = tree();
    let target = tree.add(
        FixedSize::new()
            .width(100.0)
            .height(100.0)
            .child(Spacer::new())
            .context_menu(move |_, _| {
                let lists = lists.clone();
                Some(Box::new(context_menu(vec![
                    MenuEntry::submenu(lit!("Enviar a otra lista"), move || {
                        vec![
                            MenuEntry::item(lit!("Nueva lista")),
                            MenuEntry::divider(),
                            MenuEntry::item(lit!("Oxidy Drift")).on_activate(adds(&lists, 7)),
                        ]
                    })
                    .icon(icons::window::close),
                ])))
            }),
    );
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(target)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    right_click(&mut tree, Point::new(50.0, 50.0));
    press(&mut tree, Key::ArrowDown);
    press(&mut tree, Key::ArrowRight);
    assert_eq!(
        tree.active_overlays().len(),
        2,
        "the submenu opened beside the menu"
    );
    let target = tree.find_by_label("Oxidy Drift").unwrap();
    tree.click(target);
    layout(&mut tree);
    assert_eq!(count.get(), 7);
}

/// `menu` opened by a right-click at (50, 50); the tree and the menu's bounds.
fn opened(menu: impl Fn() -> MenuList + 'static) -> (WidgetTree, Rect) {
    let mut tree = tree();
    let target = tree.add(
        FixedSize::new()
            .width(100.0)
            .height(100.0)
            .child(Spacer::new())
            .context_menu(move |_, _| Some(Box::new(menu()))),
    );
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(target)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    right_click(&mut tree, Point::new(50.0, 50.0));
    let overlay = tree.active_overlays()[0];
    let bounds = tree.overlay_content_bounds(overlay).unwrap();
    (tree, bounds)
}

#[test]
fn the_filter_menu_is_172_wide_with_18_pixel_icons() {
    let (tree, menu) = opened(|| {
        filter_menu(vec![
            MenuEntry::item(lit!("Carpeta")).icon(icons::window::close),
            MenuEntry::item(lit!("Género"))
                .icon(icons::window::close)
                .selected(true),
        ])
    });
    assert_eq!(menu.width, FILTER_MENU_WIDTH);
    let genre = tree.find_by_label("Género").unwrap();
    let icon = tree.bounds(first_of_type(&tree, genre, "IconWidget").unwrap());
    assert_eq!((icon.width, icon.height), (18.0, 18.0));
    // 1 px of frame padding and 15 px into the row.
    assert_eq!(icon.x - menu.x, 16.0);
    assert_eq!(tree.bounds(genre).height, ROW_HEIGHT);
}

#[test]
fn a_menu_is_drawn_in_the_iced_frame() {
    let count = counter();
    let (tree, id) = mounted(context_menu(row_entries(&count)));
    assert!(first_of_type(&tree, id, "Frame").is_some());
}

/// The player menu's entries (`player.rs:478-498`), shortened: two stubs, a
/// divider and an enabled entry.
fn player_entries(count: &Rc<Cell<u32>>) -> Vec<MenuEntry> {
    vec![
        MenuEntry::item(lit!("Abrir Archivo")).enabled(false),
        MenuEntry::item(lit!("Abrir Carpeta")).on_activate(adds(count, 1)),
        MenuEntry::divider(),
        MenuEntry::item(lit!("Ecualizador")).on_activate(adds(count, 10)),
        MenuEntry::item(lit!("Acerca")).enabled(false),
    ]
}

#[test]
fn the_player_menu_is_180_wide_with_15_pixels_of_padding() {
    let count = counter();
    let (tree, id) = mounted(PlayerMenu::new(player_entries(&count), lit!("Menú")));
    let menu = tree.bounds(id);
    assert_eq!(menu.width, PLAYER_MENU_WIDTH);
    let first = tree.bounds(tree.find_by_label("Abrir Archivo").unwrap());
    assert_eq!(first.x - menu.x, PLAYER_MENU_PADDING);
    assert_eq!(first.y - menu.y, PLAYER_MENU_PADDING);
    assert_eq!(first.width, PLAYER_MENU_WIDTH - 2.0 * PLAYER_MENU_PADDING);
    let divider = tree.bounds(first_of_type(&tree, id, "MenuDivider").unwrap());
    let open = tree.bounds(tree.find_by_label("Abrir Carpeta").unwrap());
    let equalizer = tree.bounds(tree.find_by_label("Ecualizador").unwrap());
    assert_eq!(divider.y - (open.y + open.height), PLAYER_DIVIDER_SPACE);
    assert_eq!(equalizer.y - (divider.y + 1.0), PLAYER_DIVIDER_SPACE);
}

#[test]
fn the_player_menu_is_a_menu_of_menu_items() {
    let count = counter();
    let (tree, id) = mounted(PlayerMenu::new(player_entries(&count), lit!("Menú")));
    assert_eq!(tree.accessibility_node(id).role(), Role::Menu);
    assert_eq!(tree.find_by_role(Role::Menu), Some(id));
    assert_eq!(tree.accessibility_node(id).name(), Some("Menú"));
    let stub = tree.find_by_label("Abrir Archivo").unwrap();
    assert_eq!(tree.accessibility_node(stub).role(), Role::MenuItem);
    assert!(tree.accessibility_node(stub).is_disabled());
}

#[test]
fn up_and_down_move_between_the_enabled_player_entries() {
    let enabled = [false, true, true, false];
    assert_eq!(next_row(&enabled, None, &Key::ArrowDown), Some(1));
    assert_eq!(next_row(&enabled, Some(1), &Key::ArrowDown), Some(2));
    assert_eq!(next_row(&enabled, Some(2), &Key::ArrowDown), Some(1));
    assert_eq!(next_row(&enabled, Some(1), &Key::ArrowUp), Some(2));
    assert_eq!(next_row(&enabled, None, &Key::ArrowUp), Some(2));
    assert_eq!(next_row(&enabled, Some(1), &Key::Enter), None);
    assert_eq!(next_row(&[false, false], None, &Key::ArrowDown), None);
}

#[test]
fn the_keyboard_walks_and_activates_the_player_menu() {
    let count = counter();
    let (mut tree, _) = mounted(PlayerMenu::new(player_entries(&count), lit!("Menú")));
    let open = tree.find_by_label("Abrir Carpeta").unwrap();
    let equalizer = tree.find_by_label("Ecualizador").unwrap();
    tree.focus(open);
    press(&mut tree, Key::ArrowDown);
    assert_eq!(tree.focused(), Some(equalizer));
    press(&mut tree, Key::Enter);
    assert_eq!(count.get(), 10);
    press(&mut tree, Key::ArrowDown);
    assert_eq!(
        tree.focused(),
        Some(open),
        "the stubs are skipped and the list wraps"
    );
    let stub = tree.find_by_label("Acerca").unwrap();
    access_click(&mut tree, stub);
    assert_eq!(count.get(), 10);
}
