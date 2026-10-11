//! Headless tests of [`EqSlider`]: layout at the Iced size, the pointer and
//! keyboard contract, the look in each state, and the accessible node.

use std::cell::RefCell;
use std::rc::Rc;

use teksilo::canvas::{Point, RenderFrame};
use teksilo::core::accessibility::widget_id_to_node_id;
use teksilo::core::accesskit::{Action, ActionData, Node, Role};
use teksilo::core::{EventTime, Key, PointerButton, PointerPhase, PointerSample, WidgetTree};
use teksilo::prelude::*;
use teksilo::tokens::{Color, VAlignment};
use teksilo::widgets::{HStack, Spacer};

use super::*;
use crate::teksilo_ui::style;
use crate::teksilo_ui::test_support::{key_down, layout, with_shipped_messages};

/// A slider at the window's top-left corner (a tree's root always takes the
/// whole viewport), its value and the values `on_change` reported.
struct Fixture {
    tree: WidgetTree,
    root: WidgetId,
    slider: WidgetId,
    value: Signal<f32>,
    changes: Rc<RefCell<Vec<f32>>>,
}

fn fixture(initial: f32, eq_enabled: Signal<bool>) -> Fixture {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let value = Signal::new(initial);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let seen = changes.clone();
    let root = tree.add(
        EqSlider::new(value.clone(), lit!("1k"))
            .eq_enabled(eq_enabled)
            .on_change(move |v, _| seen.borrow_mut().push(v)),
    );
    tree.add(
        HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(root)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    let slider = tree.find_by_role(Role::Slider).unwrap();
    Fixture {
        tree,
        root,
        slider,
        value,
        changes,
    }
}

fn pointer(tree: &mut WidgetTree, phase: PointerPhase, at: Point, button: PointerButton, ms: u64) {
    tree.dispatch_pointer(
        PointerSample::mouse(phase, at, EventTime::from_millis(ms)).with_button(button),
    );
    layout(tree);
}

fn node(tree: &mut WidgetTree, id: WidgetId) -> Node {
    let wanted = widget_id_to_node_id(id);
    tree.sync_accessibility()
        .nodes
        .into_iter()
        .find(|(id, _)| *id == wanted)
        .map(|(_, node)| node)
        .unwrap()
}

/// The colour of the 16 × 16 handle quad painted at `(x, y)`, or of its
/// outline when it has a border.
fn handle_color(frame: &RenderFrame, y: f32) -> [f32; 4] {
    frame
        .shapes
        .iter()
        .find(|shape| shape.screen == [4.0, y, 16.0, 16.0])
        .map(|shape| shape.color)
        .unwrap()
}

fn rgba(color: Color) -> [f32; 4] {
    color.to_array()
}

#[test]
fn the_slider_is_twenty_four_by_two_hundred_and_forty() {
    let f = fixture(0.0, Signal::new(true));
    let bounds = f.tree.bounds(f.root);
    assert_eq!((bounds.width, bounds.height), (24.0, 240.0));
    assert_eq!(f.tree.bounds(f.slider), bounds);
}

#[test]
fn a_primary_press_moves_the_value_to_the_pointer_at_once() {
    let mut f = fixture(0.0, Signal::new(true));
    pointer(
        &mut f.tree,
        PointerPhase::Down,
        Point::new(12.0, 60.0),
        PointerButton::Primary,
        10,
    );
    assert!((f.value.get() - 4.5).abs() < 1e-5);
    assert_eq!(f.changes.borrow().len(), 1);
    pointer(
        &mut f.tree,
        PointerPhase::Up,
        Point::new(12.0, 60.0),
        PointerButton::Primary,
        20,
    );
    assert!((f.value.get() - 4.5).abs() < 1e-5);
}

#[test]
fn a_drag_follows_the_pointer_and_clamps_at_the_ends() {
    let mut f = fixture(0.0, Signal::new(true));
    pointer(
        &mut f.tree,
        PointerPhase::Down,
        Point::new(12.0, 120.0),
        PointerButton::Primary,
        10,
    );
    for (i, y) in [100.0, 60.0, 400.0].into_iter().enumerate() {
        pointer(
            &mut f.tree,
            PointerPhase::Move,
            Point::new(12.0, y),
            PointerButton::Primary,
            20 + i as u64 * 10,
        );
    }
    assert_eq!(f.value.get(), -9.0);
    pointer(
        &mut f.tree,
        PointerPhase::Up,
        Point::new(12.0, 400.0),
        PointerButton::Primary,
        100,
    );
    assert_eq!(f.value.get(), -9.0);
    assert_eq!(f.changes.borrow().last(), Some(&-9.0));
}

#[test]
fn a_secondary_press_resets_the_band_without_moving_it_first() {
    let mut f = fixture(3.0, Signal::new(true));
    pointer(
        &mut f.tree,
        PointerPhase::Down,
        Point::new(12.0, 20.0),
        PointerButton::Secondary,
        10,
    );
    pointer(
        &mut f.tree,
        PointerPhase::Up,
        Point::new(12.0, 20.0),
        PointerButton::Secondary,
        20,
    );
    assert_eq!(f.value.get(), 0.0);
    assert_eq!(*f.changes.borrow(), vec![0.0]);
}

#[test]
fn the_reset_action_does_what_the_secondary_click_does() {
    let mut f = fixture(-4.2, Signal::new(true));
    let actions = node(&mut f.tree, f.slider).custom_actions().to_vec();
    assert_eq!(actions.len(), 1);
    f.tree.dispatch_event(WidgetEvent::AccessAction {
        action: Action::CustomAction,
        target: Some(f.slider),
        target_node: widget_id_to_node_id(f.slider),
        data: Some(ActionData::CustomAction(actions[0].id)),
    });
    layout(&mut f.tree);
    assert_eq!(f.value.get(), 0.0);
    assert_eq!(*f.changes.borrow(), vec![0.0]);
}

#[test]
fn up_and_down_step_by_a_tenth_and_left_and_right_do_nothing() {
    let mut f = fixture(0.0, Signal::new(true));
    key_down(&mut f.tree, f.slider, Key::ArrowUp);
    assert!((f.value.get() - 0.1).abs() < 1e-5);
    key_down(&mut f.tree, f.slider, Key::ArrowDown);
    key_down(&mut f.tree, f.slider, Key::ArrowDown);
    assert!((f.value.get() + 0.1).abs() < 1e-5);
    key_down(&mut f.tree, f.slider, Key::ArrowLeft);
    key_down(&mut f.tree, f.slider, Key::ArrowRight);
    assert!((f.value.get() + 0.1).abs() < 1e-5);
    assert_eq!(f.changes.borrow().len(), 3);
}

#[test]
fn page_keys_step_by_one_db_and_home_and_end_reach_the_limits() {
    let mut f = fixture(0.0, Signal::new(true));
    key_down(&mut f.tree, f.slider, Key::PageUp);
    assert!((f.value.get() - 1.0).abs() < 1e-5);
    key_down(&mut f.tree, f.slider, Key::Home);
    assert_eq!(f.value.get(), -9.0);
    key_down(&mut f.tree, f.slider, Key::End);
    assert_eq!(f.value.get(), 9.0);
}

#[test]
fn the_handle_is_red_at_rest_and_light_when_selected() {
    let mut f = fixture(0.0, Signal::new(true));
    let frame = f.tree.render();
    assert_eq!(handle_color(&frame, 112.0), rgba(style::ACCENT));
    f.tree.focus(f.slider);
    layout(&mut f.tree);
    let frame = f.tree.render();
    assert_eq!(handle_color(&frame, 112.0), rgba(style::TEXT_PRIMARY));
}

#[test]
fn with_the_equalizer_off_the_handle_is_dark_and_outlined_when_selected() {
    let mut f = fixture(0.0, Signal::new(false));
    let frame = f.tree.render();
    assert_eq!(handle_color(&frame, 112.0), rgba(style::CONTRAST));
    f.tree.focus(f.slider);
    layout(&mut f.tree);
    let frame = f.tree.render();
    assert_eq!(handle_color(&frame, 112.0), rgba(style::TEXT_SECONDARY));
    assert!(
        frame
            .shapes
            .iter()
            .any(|s| s.screen == [5.0, 113.0, 14.0, 14.0] && s.color == rgba(style::CONTRAST))
    );
}

#[test]
fn turning_the_equalizer_off_restyles_without_disabling() {
    let enabled = Signal::new(true);
    let mut f = fixture(0.0, enabled.clone());
    enabled.set(false);
    layout(&mut f.tree);
    let frame = f.tree.render();
    assert_eq!(handle_color(&frame, 112.0), rgba(style::CONTRAST));
    key_down(&mut f.tree, f.slider, Key::ArrowUp);
    assert!((f.value.get() - 0.1).abs() < 1e-5);
}

#[test]
fn the_track_has_no_coloured_part() {
    let mut f = fixture(5.0, Signal::new(true));
    let frame = f.tree.render();
    let tracks: Vec<_> = frame
        .shapes
        .iter()
        .filter(|s| s.screen[0] == 8.0 && s.screen[2] == 8.0)
        .collect();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].screen, [8.0, 0.0, 8.0, 240.0]);
    assert_eq!(tracks[0].color, rgba(style::CONTRAST));
}

#[test]
fn the_slider_is_announced_with_its_band_range_step_and_db_value() {
    with_shipped_messages("en-US", || {
        let mut f = fixture(-3.0, Signal::new(true));
        let node = node(&mut f.tree, f.slider);
        assert_eq!(node.role(), Role::Slider);
        assert_eq!(node.label(), Some("1k"));
        assert_eq!(node.numeric_value(), Some(-3.0));
        assert_eq!(node.min_numeric_value(), Some(-9.0));
        assert_eq!(node.max_numeric_value(), Some(9.0));
        assert!((node.numeric_value_step().unwrap() - 0.1).abs() < 1e-6);
        assert_eq!(node.value(), Some("-3.0 dB"));
        assert!(node.supports_action(Action::Increment));
        let actions = node.custom_actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(&*actions[0].description, "Reset to default");
    });
}

#[test]
fn the_spanish_slider_offers_the_iced_reset_wording() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture(1.5, Signal::new(true));
        let node = node(&mut f.tree, f.slider);
        assert_eq!(node.value(), Some("1.5 dB"));
        assert_eq!(
            &*node.custom_actions()[0].description,
            "Restablecer al valor por defecto"
        );
    });
}
