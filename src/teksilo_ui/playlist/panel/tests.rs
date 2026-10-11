//! The panel over the mock library, in the playlist's 400 × 480 region.

use std::time::Instant;

use teksilo::canvas::Point;
use teksilo::core::accesskit::Role;
use teksilo::core::{PointerButton, WidgetTree};

use super::*;
use crate::logic::selection_modifiers::SelectModifiers;
use crate::teksilo_ui::playlist::testing::{Env, REGION, all_of_type, mount, restored};
use crate::teksilo_ui::test_support::pointer_click;

fn mounted(env: &Env) -> (WidgetTree, WidgetId, Rc<Cell<Option<WidgetId>>>) {
    let panel = PlaylistPanel::new(env.vm.clone());
    let list = panel.list_id();
    let (tree, id, _, _) = mount(env, panel);
    (tree, id, list)
}

fn select_row(env: &Env, linear_idx: usize) {
    env.vm.click(
        linear_idx,
        SelectModifiers {
            toggle: false,
            range: false,
        },
        Instant::now(),
    );
}

#[test]
fn the_strip_the_list_and_the_bar_stack_in_the_region() {
    let env = restored();
    let (tree, root, list) = mounted(&env);
    let panel = tree.bounds(root);
    assert_eq!((panel.width, panel.height), REGION);
    let strip = tree.bounds(all_of_type(&tree, root, "TabsStrip")[0]);
    let bar = tree.bounds(all_of_type(&tree, root, "BottomBar")[0]);
    let list = tree.bounds(list.get().unwrap());
    assert_eq!((strip.y, strip.height, strip.width), (0.0, 40.0, 400.0));
    assert_eq!((list.y, list.height, list.width), (40.0, 400.0, 400.0));
    assert_eq!((bar.y, bar.height, bar.width), (440.0, 40.0, 400.0));
    let rows = all_of_type(&tree, root, "PlaylistRow");
    assert_eq!(
        tree.bounds(rows[0]).y,
        40.0,
        "the first row under the strip"
    );
}

#[test]
fn a_press_on_the_strip_or_the_bar_clears_the_selection() {
    let env = restored();
    let (mut tree, _, list) = mounted(&env);
    select_row(&env, 4);
    pointer_click(
        &mut tree,
        Point::new(385.0, 20.0),
        PointerButton::Primary,
        1_000,
    );
    assert!(env.selected().is_empty(), "the strip's free end");
    assert_eq!(tree.focused(), list.get());
    select_row(&env, 4);
    pointer_click(
        &mut tree,
        Point::new(220.0, 460.0),
        PointerButton::Primary,
        2_000,
    );
    assert!(env.selected().is_empty(), "between the field and the icons");
}

#[test]
fn a_press_on_a_control_keeps_the_selection() {
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    select_row(&env, 4);
    let field = tree.bounds(tree.find_by_role(Role::TextInput).unwrap());
    pointer_click(&mut tree, field.center(), PointerButton::Primary, 1_000);
    assert_eq!(env.selected(), vec![4], "the search field");
    let shuffle = tree.bounds(tree.find_by_label("Shuffle").unwrap());
    pointer_click(&mut tree, shuffle.center(), PointerButton::Primary, 2_000);
    assert!(env.vm.shuffle().get());
    assert_eq!(env.selected(), vec![4], "an icon");
}

#[test]
fn the_panel_subscribes_to_the_facade_and_keeps_its_view_model() {
    let env = restored();
    let panel = PlaylistPanel::new(env.vm.clone());
    assert!(panel.list_id().get().is_none(), "the list is made in build");
    let (tree, root, list) = mounted(&env);
    assert!(list.get().is_some());
    assert!(!all_of_type(&tree, root, "PlaylistList").is_empty());
}
