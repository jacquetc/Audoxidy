//! Headless tests of [`Stepper`]: layout at the Iced size, the arrows and
//! their hold-to-repeat on a manual clock, the text field's commit rules,
//! reset, the disabled state, the keyboard and the accessible node.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use teksilo::canvas::{Point, RenderFrame};
use teksilo::core::accessibility::widget_id_to_node_id;
use teksilo::core::accesskit::{Action, ActionData, Role};
use teksilo::core::{Key, Modifiers, PointerButton, PointerPhase, WidgetTree};
use teksilo::prelude::*;
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{HStack, Spacer, VStack};

use super::face::{WIDTH, Zone};
use super::*;
use crate::teksilo_ui::style;
use crate::teksilo_ui::test_support::{
    a11y_node, key_down, key_down_with, layout, pointer_click, pointer_event, type_text,
    with_shipped_messages,
};

struct Fixture {
    tree: WidgetTree,
    stepper: WidgetId,
    value: Signal<f64>,
    changes: Rc<RefCell<Vec<f64>>>,
    now: Rc<Cell<Instant>>,
    t0: Instant,
}

/// A "Fade in" stepper (0…10000 ms, reset to 1000) at the top-left, on a
/// manual clock an hour ahead of the real one, so the tree never serves its
/// wake deadlines by itself.
fn fixture(initial: f64, configure: impl FnOnce(Stepper) -> Stepper) -> Fixture {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let value = Signal::new(initial);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let seen = changes.clone();
    let t0 = Instant::now() + Duration::from_secs(3600);
    let now = Rc::new(Cell::new(t0));
    let clock = now.clone();
    let stepper = tree.add(configure(
        Stepper::new(
            value.clone(),
            0.0..=10000.0,
            StepperUnit::Milliseconds,
            lit!("Fade in"),
        )
        .reset_to(1000.0)
        .clock(Rc::new(move || clock.get()))
        .on_change(move |v| seen.borrow_mut().push(v)),
    ));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Top)
                    .child(stepper)
                    .child(Spacer::new()),
            )
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    Fixture {
        tree,
        stepper,
        value,
        changes,
        now,
        t0,
    }
}

fn plain(stepper: Stepper) -> Stepper {
    stepper
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const MINUS: Point = Point { x: 7.0, y: 6.0 };
const VALUE: Point = Point { x: 48.0, y: 6.0 };
const PLUS: Point = Point { x: 88.0, y: 6.0 };

/// Moves the manual clock to `t0 + offset` and runs one frame.
fn frame_at(f: &mut Fixture, offset: Duration) {
    f.now.set(f.t0 + offset);
    f.tree.frame_tick().set(0.016);
    layout(&mut f.tree);
}

fn retype(tree: &mut WidgetTree, text: &str) {
    let field = tree.focused().unwrap();
    key_down_with(tree, field, Key::A, Modifiers::COMMAND);
    key_down(tree, field, Key::Delete);
    type_text(tree, text);
}

fn editing(f: &Fixture) -> bool {
    f.tree.focused().is_some_and(|id| {
        f.tree
            .widget_type_name(id)
            .is_some_and(|name| name.ends_with("TextInputField"))
    })
}

fn fill_at(frame: &RenderFrame, rect: [f32; 4]) -> Option<[f32; 4]> {
    frame
        .decorations
        .iter()
        .filter(|d| d.rect == rect)
        .map(|d| d.color)
        .chain(
            frame
                .shapes
                .iter()
                .filter(|s| s.screen == rect)
                .map(|s| s.color),
        )
        .last()
}

#[test]
fn the_stepper_is_ninety_six_by_thirteen() {
    let f = fixture(1000.0, plain);
    let bounds = f.tree.bounds(f.stepper);
    assert_eq!((bounds.width, bounds.height), (WIDTH, 13.0));
}

#[test]
fn a_click_on_an_arrow_steps_once_by_the_unit_step() {
    let mut f = fixture(1000.0, plain);
    pointer_click(&mut f.tree, PLUS, PointerButton::Primary, 0);
    assert_eq!(f.value.get(), 1050.0);
    pointer_click(&mut f.tree, MINUS, PointerButton::Primary, 0);
    pointer_click(&mut f.tree, MINUS, PointerButton::Primary, 0);
    assert_eq!(f.value.get(), 950.0);
    assert_eq!(*f.changes.borrow(), vec![1050.0, 1000.0, 950.0]);
}

#[test]
fn a_step_is_clamped_to_the_range() {
    let mut f = fixture(9980.0, plain);
    pointer_click(&mut f.tree, PLUS, PointerButton::Primary, 0);
    assert_eq!(f.value.get(), 10000.0);
    pointer_click(&mut f.tree, PLUS, PointerButton::Primary, 0);
    assert_eq!(f.changes.borrow().len(), 1);
}

#[test]
fn a_held_arrow_repeats_after_four_tenths_then_every_tenth_of_a_second() {
    let mut f = fixture(1000.0, plain);
    pointer_event(
        &mut f.tree,
        PointerPhase::Down,
        PLUS,
        PointerButton::Primary,
        0,
    );
    assert_eq!(f.value.get(), 1050.0);
    frame_at(&mut f, ms(300));
    assert_eq!(f.value.get(), 1050.0);
    frame_at(&mut f, ms(400));
    assert_eq!(f.value.get(), 1100.0);
    frame_at(&mut f, ms(450));
    assert_eq!(f.value.get(), 1100.0);
    // A late frame catches up: repeats due at 0.5, 0.6 and 0.7 s.
    frame_at(&mut f, ms(750));
    assert_eq!(f.value.get(), 1250.0);
}

#[test]
fn the_hold_wakes_the_window_at_each_deadline_and_never_after_the_release() {
    let mut f = fixture(1000.0, plain);
    let wake = f.tree.wake_at_handle();
    pointer_event(
        &mut f.tree,
        PointerPhase::Down,
        PLUS,
        PointerButton::Primary,
        0,
    );
    assert_eq!(wake.get(), Some(f.t0 + ms(400)));
    frame_at(&mut f, ms(420));
    assert_eq!(wake.get(), Some(f.t0 + ms(500)));
    pointer_event(
        &mut f.tree,
        PointerPhase::Up,
        PLUS,
        PointerButton::Primary,
        0,
    );
    assert_eq!(wake.get(), None);
    frame_at(&mut f, ms(2000));
    assert_eq!(f.value.get(), 1100.0);
}

#[test]
fn a_cancelled_hold_stops_repeating() {
    let mut f = fixture(1000.0, plain);
    let finger = f.tree.new_contact();
    f.tree.touch_down(finger, PLUS);
    layout(&mut f.tree);
    assert_eq!(f.value.get(), 1050.0);
    f.tree.touch_cancel(finger, PLUS);
    layout(&mut f.tree);
    frame_at(&mut f, ms(1000));
    assert_eq!(f.value.get(), 1050.0);
    assert_eq!(f.tree.wake_at_handle().get(), None);
}

#[test]
fn a_secondary_press_resets_and_does_nothing_without_a_default() {
    let mut f = fixture(4000.0, plain);
    pointer_click(&mut f.tree, VALUE, PointerButton::Secondary, 0);
    assert_eq!(f.value.get(), 1000.0);
    assert!(!editing(&f));
    let mut f = fixture(4000.0, |s| {
        Stepper::new(
            s.value.clone(),
            500.0..=5000.0,
            StepperUnit::Milliseconds,
            lit!("Hold"),
        )
    });
    pointer_click(&mut f.tree, PLUS, PointerButton::Secondary, 0);
    assert_eq!(f.value.get(), 4000.0);
}

#[test]
fn the_caret_lands_where_iced_puts_it() {
    assert_eq!(caret_at(0.0, "1000"), 0);
    assert_eq!(caret_at(15.0, "1000"), 2);
    assert_eq!(caret_at(60.0, "1000"), 4);
    assert_eq!(caret_at(-3.0, "1000"), 0);
}

#[test]
fn a_press_on_the_value_edits_it_and_enter_commits_without_rounding() {
    let mut f = fixture(1000.0, plain);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    assert!(editing(&f));
    retype(&mut f.tree, "1234");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert!(!editing(&f));
    assert_eq!(f.value.get(), 1234.0);
    assert_eq!(f.tree.focused(), Some(f.stepper));
}

#[test]
fn a_committed_value_is_clamped_and_an_unparsable_one_dropped() {
    let mut f = fixture(1000.0, plain);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    retype(&mut f.tree, "99999 ms");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert_eq!(f.value.get(), 10000.0);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    retype(&mut f.tree, "-");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert_eq!(f.value.get(), 10000.0);
}

#[test]
fn escape_cancels_the_edit() {
    let mut f = fixture(1000.0, plain);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    retype(&mut f.tree, "42");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Escape);
    assert!(!editing(&f));
    assert_eq!(f.value.get(), 1000.0);
    assert!(f.changes.borrow().is_empty());
}

#[test]
fn an_arrow_pressed_while_editing_steps_from_the_committed_value() {
    let mut f = fixture(1000.0, plain);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    retype(&mut f.tree, "1234");
    pointer_click(&mut f.tree, PLUS, PointerButton::Primary, 0);
    assert!(!editing(&f));
    assert_eq!(*f.changes.borrow(), vec![1234.0, 1284.0]);
}

#[test]
fn leaving_the_field_commits_it() {
    let mut f = fixture(1000.0, plain);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    retype(&mut f.tree, "777");
    f.tree.focus(f.stepper);
    layout(&mut f.tree);
    assert!(!editing(&f));
    assert_eq!(f.value.get(), 777.0);
}

#[test]
fn the_field_takes_only_number_characters() {
    let mut f = fixture(-5.0, |s| {
        Stepper::new(
            s.value.clone(),
            -80.0..=0.0,
            StepperUnit::Decibels,
            lit!("Threshold"),
        )
    });
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    retype(&mut f.tree, "-1x2.5dB");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert_eq!(f.value.get(), -12.5);
}

#[test]
fn up_and_down_step_and_enter_edits_when_focused() {
    let mut f = fixture(1000.0, plain);
    key_down(&mut f.tree, f.stepper, Key::ArrowUp);
    assert_eq!(f.value.get(), 1050.0);
    key_down(&mut f.tree, f.stepper, Key::ArrowDown);
    key_down(&mut f.tree, f.stepper, Key::ArrowDown);
    assert_eq!(f.value.get(), 950.0);
    key_down(&mut f.tree, f.stepper, Key::Enter);
    assert!(editing(&f));
}

#[test]
fn the_stepper_is_a_tab_stop() {
    let f = fixture(1000.0, plain);
    assert!(f.tree.tab_stops_within(f.stepper).contains(&f.stepper));
}

#[test]
fn a_disabled_stepper_ignores_every_path_and_dims() {
    let enabled = Signal::new(false);
    let flag = enabled.clone();
    let mut f = fixture(1000.0, move |s| s.enabled(flag));
    pointer_click(&mut f.tree, PLUS, PointerButton::Primary, 0);
    pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
    pointer_click(&mut f.tree, VALUE, PointerButton::Secondary, 0);
    key_down(&mut f.tree, f.stepper, Key::ArrowUp);
    assert_eq!(f.value.get(), 1000.0);
    assert!(!editing(&f));
    let node = a11y_node(&mut f.tree, f.stepper);
    assert!(node.is_disabled());
    assert!(!node.supports_action(Action::Increment));
    assert!(!f.tree.tab_stops_within(f.stepper).contains(&f.stepper));
    enabled.set(true);
    layout(&mut f.tree);
    pointer_click(&mut f.tree, PLUS, PointerButton::Primary, 0);
    assert_eq!(f.value.get(), 1050.0);
}

#[test]
fn the_arrow_under_the_pointer_lights_up() {
    let mut f = fixture(1000.0, plain);
    let minus = [0.0, 0.0, 15.0, 13.0];
    assert_eq!(
        fill_at(&f.tree.render(), minus),
        Some(style::CONTRAST.to_array())
    );
    pointer_event(
        &mut f.tree,
        PointerPhase::Move,
        MINUS,
        PointerButton::Primary,
        0,
    );
    assert_eq!(
        fill_at(&f.tree.render(), minus),
        Some(style::ACCENT.to_array())
    );
    pointer_event(
        &mut f.tree,
        PointerPhase::Move,
        VALUE,
        PointerButton::Primary,
        0,
    );
    assert_eq!(
        fill_at(&f.tree.render(), minus),
        Some(style::CONTRAST.to_array())
    );
    assert_eq!(
        fill_at(&f.tree.render(), [15.0, 0.0, 66.0, 13.0]),
        Some(style::BACKGROUND.to_array())
    );
}

#[test]
fn the_stepper_is_a_named_spin_button_with_its_value_and_actions() {
    with_shipped_messages("en-US", || {
        let mut f = fixture(1000.0, plain);
        let n = a11y_node(&mut f.tree, f.stepper);
        assert_eq!(n.role(), Role::SpinButton);
        assert_eq!(n.label(), Some("Fade in"));
        assert_eq!(n.value(), Some("1000 ms"));
        assert_eq!(n.numeric_value(), Some(1000.0));
        assert_eq!(n.min_numeric_value(), Some(0.0));
        assert_eq!(n.max_numeric_value(), Some(10000.0));
        assert_eq!(n.numeric_value_step(), Some(50.0));
        assert!(n.supports_action(Action::Increment));
        assert!(n.supports_action(Action::Decrement));
        assert_eq!(&*n.custom_actions()[0].description, "Reset to default");
    });
}

#[test]
fn the_accessibility_actions_step_set_and_reset() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture(1000.0, plain);
        let target = widget_id_to_node_id(f.stepper);
        let act = |tree: &mut WidgetTree, action, data| {
            tree.dispatch_event(WidgetEvent::AccessAction {
                action,
                target: Some(f.stepper),
                target_node: target,
                data,
            });
            layout(tree);
        };
        act(&mut f.tree, Action::Increment, None);
        assert_eq!(f.value.get(), 1050.0);
        act(&mut f.tree, Action::Decrement, None);
        assert_eq!(f.value.get(), 1000.0);
        act(
            &mut f.tree,
            Action::SetValue,
            Some(ActionData::NumericValue(1234.0)),
        );
        assert_eq!(f.value.get(), 1234.0);
        let reset = a11y_node(&mut f.tree, f.stepper).custom_actions().to_vec();
        assert_eq!(&*reset[0].description, "Restablecer al valor por defecto");
        act(
            &mut f.tree,
            Action::CustomAction,
            Some(ActionData::CustomAction(reset[0].id)),
        );
        assert_eq!(f.value.get(), 1000.0);
    });
}

#[test]
fn only_the_arrow_zones_count_as_hovered() {
    assert_eq!(arrow_only(Zone::Minus), Zone::Minus);
    assert_eq!(arrow_only(Zone::Value), Zone::None);
    assert_eq!(arrow_only(Zone::Plus), Zone::Plus);
}

#[test]
fn the_field_opens_again_after_a_commit() {
    let mut f = fixture(1000.0, plain);
    for (text, expected) in [("1111", 1111.0), ("2222", 2222.0)] {
        pointer_click(&mut f.tree, VALUE, PointerButton::Primary, 0);
        assert!(editing(&f));
        retype(&mut f.tree, text);
        let field = f.tree.focused().unwrap();
        key_down(&mut f.tree, field, Key::Enter);
        assert_eq!(f.value.get(), expected);
    }
}
