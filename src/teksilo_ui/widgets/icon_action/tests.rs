use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::accesskit::Role;
use teksilo::core::{Key, WidgetTree};
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::test_support::{access_click, key_down, layout};

/// `action` at the top-leading corner of a window-sized column, counting its
/// activations.
fn mounted(action: IconAction) -> (WidgetTree, WidgetId, Rc<Cell<u32>>) {
    let count = Rc::new(Cell::new(0));
    let hits = count.clone();
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let id = tree.add(action.on_activate(move |_| hits.set(hits.get() + 1)));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(id)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    (tree, id, count)
}

#[test]
fn a_plain_icon_takes_its_size() {
    let (tree, id, _) = mounted(IconAction::new(icons::window::close(), 31.0, lit!("Menú")));
    let bounds = tree.bounds(id);
    assert_eq!((bounds.width, bounds.height), (31.0, 31.0));
}

#[test]
fn a_padded_plain_icon_grows_by_its_padding() {
    let (tree, id, _) =
        mounted(IconAction::new(icons::window::close(), 24.0, lit!("Aleatorio")).padding(2.0));
    assert_eq!(tree.bounds(id).width, 28.0);
}

#[test]
fn an_icon_button_is_four_pixels_wider_than_its_icon_and_at_least_twenty() {
    assert_eq!(button_side(14.0), 20.0);
    assert_eq!(button_side(24.0), 28.0);
    let (tree, id, _) = mounted(IconAction::button(
        icons::window::close(),
        14.0,
        lit!("Cargar preset"),
    ));
    let bounds = tree.bounds(id);
    assert_eq!((bounds.width, bounds.height), (20.0, 20.0));
}

#[test]
fn the_icon_button_lights_up_only_when_hovered_and_enabled() {
    assert_eq!(
        button_colors(false, true),
        (SurfaceRole::Transparent, TextRole::Secondary)
    );
    assert_eq!(
        button_colors(true, true),
        (SurfaceRole::Accent, TextRole::Primary)
    );
    assert_eq!(
        button_colors(true, false),
        (SurfaceRole::Transparent, TextRole::Primary)
    );
}

#[test]
fn hovering_an_icon_button_repaints_it() {
    let (mut tree, id, _) = mounted(IconAction::button(
        icons::window::close(),
        14.0,
        lit!("Cargar preset"),
    ));
    tree.pointer_move(tree.bounds(id).center());
    layout(&mut tree);
    let _ = tree.render();
}

#[test]
fn a_click_enter_space_and_the_click_action_activate_it() {
    let (mut tree, id, count) =
        mounted(IconAction::new(icons::window::close(), 31.0, lit!("Menú")));
    let button = tree.find_by_role(Role::Button).unwrap();
    access_click(&mut tree, button);
    key_down(&mut tree, button, Key::Enter);
    key_down(&mut tree, button, Key::Space);
    tree.click(id);
    layout(&mut tree);
    assert_eq!(count.get(), 4);
}

#[test]
fn a_disabled_icon_ignores_every_activation_path() {
    let enabled = Signal::new(false);
    let (mut tree, id, count) = mounted(
        IconAction::button(icons::window::close(), 14.0, lit!("Guardar preset"))
            .enabled(enabled.clone()),
    );
    let button = tree.find_by_role(Role::Button).unwrap();
    assert!(tree.accessibility_node(button).is_disabled());
    access_click(&mut tree, button);
    key_down(&mut tree, button, Key::Enter);
    tree.click(id);
    assert_eq!(count.get(), 0);
    enabled.set(true);
    layout(&mut tree);
    access_click(&mut tree, button);
    assert_eq!(count.get(), 1);
}

#[test]
fn it_is_announced_as_a_button_with_its_label() {
    let (tree, _, _) = mounted(IconAction::button(
        icons::window::close(),
        14.0,
        lit!("Restablecer"),
    ));
    let button = tree.find_by_role(Role::Button).unwrap();
    assert_eq!(tree.accessibility_node(button).name(), Some("Restablecer"));
}
