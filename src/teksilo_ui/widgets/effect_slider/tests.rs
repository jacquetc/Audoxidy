//! Headless tests of [`EffectSlider`]: layout at the Iced sizes, the slider's
//! pointer and keyboard contract, the value field's editing rules, and the
//! accessible nodes.

use std::cell::RefCell;
use std::rc::Rc;

use teksilo::canvas::{Point, RenderFrame};
use teksilo::core::accessibility::widget_id_to_node_id;
use teksilo::core::accesskit::{Action, ActionData, Node, Role};
use teksilo::core::{
    EventTime, Key, Modifiers, PointerButton, PointerPhase, PointerSample, WidgetTree,
};
use teksilo::prelude::*;
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{FixedSize, HStack, Spacer, VStack};

use super::*;
use crate::teksilo_ui::test_support::{key_down, key_down_with, layout, with_shipped_messages};

/// The row width the slider is laid out in.
const ROW: f32 = 300.0;

struct Fixture {
    tree: WidgetTree,
    root: WidgetId,
    slider: WidgetId,
    value: Signal<f32>,
    changes: Rc<RefCell<Vec<f32>>>,
}

/// A "Nivel dB" slider (-4…24, reset to 0) in a 300 px row at the top-left.
fn fixture(initial: f32, configure: impl FnOnce(EffectSlider) -> EffectSlider) -> Fixture {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let value = Signal::new(initial);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let seen = changes.clone();
    let root = tree.add(configure(
        EffectSlider::new(value.clone(), -4.0..=24.0, 0.0, lit!("Nivel dB"))
            .on_change(move |v, _| seen.borrow_mut().push(v)),
    ));
    let row = tree.add(FixedSize::new().width(ROW).child(root));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Top)
                    .child(row)
                    .child(Spacer::new()),
            )
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

fn plain(slider: EffectSlider) -> EffectSlider {
    slider
}

fn click(tree: &mut WidgetTree, at: Point, button: PointerButton, ms: u64) {
    for (phase, t) in [(PointerPhase::Down, ms), (PointerPhase::Up, ms + 5)] {
        tree.dispatch_pointer(
            PointerSample::mouse(phase, at, EventTime::from_millis(t)).with_button(button),
        );
        layout(tree);
    }
}

/// Types `text` into the focused widget, one key per character.
fn type_text(tree: &mut WidgetTree, text: &str) {
    for ch in text.chars() {
        let id = tree.focused().unwrap();
        tree.dispatch_event(WidgetEvent::KeyDown {
            key: Key::Character(ch),
            modifiers: Modifiers::NONE,
            text: Some(ch.to_string().into()),
        });
        layout(tree);
        assert_eq!(tree.focused(), Some(id));
    }
}

/// Replaces the focused field's text with `text`.
fn retype(tree: &mut WidgetTree, text: &str) {
    let field = tree.focused().unwrap();
    key_down_with(tree, field, Key::A, Modifiers::COMMAND);
    key_down(tree, field, Key::Delete);
    type_text(tree, text);
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

/// The centre of the value box.
fn input_point() -> Point {
    Point::new(ROW - INPUT_WIDTH / 2.0, HEIGHT / 2.0)
}

fn editing(f: &Fixture) -> bool {
    f.tree.focused().is_some_and(|id| {
        f.tree
            .widget_type_name(id)
            .is_some_and(|name| name.ends_with("TextInputField"))
    })
}

fn fill_width(frame: &RenderFrame) -> Option<f32> {
    frame
        .shapes
        .iter()
        .find(|s| s.screen[1] == 5.0 && s.screen[3] == 8.0 && s.color == style::ACCENT.to_array())
        .map(|s| s.screen[2])
}

#[test]
fn the_slider_fills_the_row_less_the_ten_pixel_gap_and_the_forty_pixel_field() {
    let f = fixture(0.0, plain);
    assert_eq!(f.tree.bounds(f.root).height, HEIGHT);
    let slider = f.tree.bounds(f.slider);
    assert_eq!(
        (slider.x, slider.width, slider.height),
        (0.0, ROW - INPUT_GAP - INPUT_WIDTH, HEIGHT)
    );
}

#[test]
fn a_press_on_the_slider_moves_the_value_and_a_secondary_press_resets_it() {
    let mut f = fixture(0.0, plain);
    // Halfway along the 250 px track: -4 + 0.5 × 28 = 10.
    click(
        &mut f.tree,
        Point::new(125.0, 9.0),
        PointerButton::Primary,
        10,
    );
    assert!((f.value.get() - 10.0).abs() < 1e-4);
    click(
        &mut f.tree,
        Point::new(10.0, 9.0),
        PointerButton::Secondary,
        100,
    );
    assert_eq!(f.value.get(), 0.0);
    assert_eq!(f.changes.borrow().last(), Some(&0.0));
}

#[test]
fn the_track_is_filled_up_to_the_value() {
    let mut f = fixture(10.0, plain);
    let frame = f.tree.render();
    assert_eq!(fill_width(&frame), Some(125.0));
}

#[test]
fn left_and_right_step_and_up_and_down_do_nothing() {
    let mut f = fixture(1.0, |s| s.step(1.0));
    key_down(&mut f.tree, f.slider, Key::ArrowRight);
    assert_eq!(f.value.get(), 2.0);
    key_down(&mut f.tree, f.slider, Key::ArrowLeft);
    key_down(&mut f.tree, f.slider, Key::ArrowLeft);
    assert_eq!(f.value.get(), 0.0);
    key_down(&mut f.tree, f.slider, Key::ArrowUp);
    key_down(&mut f.tree, f.slider, Key::ArrowDown);
    assert_eq!(f.value.get(), 0.0);
}

#[test]
fn the_field_shows_the_value_with_the_call_site_decimals() {
    assert_eq!(format_value(0.66, 2), "0.66");
    assert_eq!(format_value(12.0, 1), "12.0");
    assert_eq!(format_value(-60.04, 1), "-60.0");
}

#[test]
fn enter_rounds_to_the_step_and_clamps_while_a_blur_only_clamps() {
    let range = (-4.0, 24.0);
    assert_eq!(
        parse_field("3.14 dB", range, 0.1, true).map(|v| (v * 100.0).round()),
        Some(310.0)
    );
    assert_eq!(parse_field("3.14", range, 0.1, false), Some(3.14));
    assert_eq!(parse_field("99", range, 0.1, true), Some(24.0));
    assert_eq!(parse_field("-12 dB", range, 0.1, false), Some(-4.0));
    assert_eq!(parse_field("abc", range, 0.1, true), None);
    assert_eq!(parse_field("", range, 0.1, true), None);
}

#[test]
fn the_caret_lands_where_iced_puts_it() {
    assert_eq!(caret_at(0.0, "12.5"), 0);
    assert_eq!(caret_at(13.2, "12.5"), 2);
    assert_eq!(caret_at(39.0, "12.5"), 4);
}

#[test]
fn a_press_on_the_field_opens_it_and_enter_commits_the_rounded_value() {
    let mut f = fixture(0.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    assert!(editing(&f));
    retype(&mut f.tree, "3.14");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert!(!editing(&f));
    assert!((f.value.get() - 3.1).abs() < 1e-5);
    assert_eq!(f.tree.focused(), Some(f.slider));
    assert_eq!(f.changes.borrow().len(), 1);
}

#[test]
fn escape_closes_the_field_without_writing() {
    let mut f = fixture(5.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    retype(&mut f.tree, "7");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Escape);
    assert!(!editing(&f));
    assert_eq!(f.value.get(), 5.0);
    assert!(f.changes.borrow().is_empty());
}

#[test]
fn pressing_the_slider_commits_the_field_clamped_but_not_rounded_first() {
    let mut f = fixture(0.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    retype(&mut f.tree, "3.14");
    // The press commits 3.14, then moves the value to the pointer.
    click(
        &mut f.tree,
        Point::new(125.0, 9.0),
        PointerButton::Primary,
        200,
    );
    assert!(!editing(&f));
    let changes = f.changes.borrow();
    assert!((changes[0] - 3.14).abs() < 1e-5);
    assert!((f.value.get() - 10.0).abs() < 1e-4);
}

#[test]
fn leaving_the_field_commits_it_clamped_but_not_rounded() {
    let mut f = fixture(0.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    retype(&mut f.tree, "3.14");
    f.tree.focus(f.slider);
    layout(&mut f.tree);
    assert!(!editing(&f));
    assert!((f.value.get() - 3.14).abs() < 1e-5);
}

#[test]
fn an_unparsable_field_writes_nothing() {
    let mut f = fixture(2.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    retype(&mut f.tree, "-");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert_eq!(f.value.get(), 2.0);
    assert!(f.changes.borrow().is_empty());
}

#[test]
fn the_field_accepts_only_digits_dots_minus_and_spaces() {
    let mut f = fixture(0.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    retype(&mut f.tree, "1a2,b");
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Enter);
    assert_eq!(f.value.get(), 12.0);
}

#[test]
fn enter_on_the_slider_opens_the_field_and_a_secondary_press_on_the_box_resets() {
    let mut f = fixture(6.0, plain);
    key_down(&mut f.tree, f.slider, Key::Enter);
    assert!(editing(&f));
    let field = f.tree.focused().unwrap();
    key_down(&mut f.tree, field, Key::Escape);
    assert_eq!(f.tree.focused(), Some(f.slider));
    click(&mut f.tree, input_point(), PointerButton::Secondary, 300);
    assert_eq!(f.value.get(), 0.0);
}

#[test]
fn the_field_box_gets_an_accent_border_while_editing() {
    let mut f = fixture(0.0, plain);
    let accent = style::ACCENT.to_array();
    let border = |frame: &RenderFrame| {
        frame
            .shapes
            .iter()
            .any(|s| s.stroke_width == INPUT_BORDER && s.color == accent)
    };
    assert!(!border(&f.tree.render()));
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    assert!(border(&f.tree.render()));
}

#[test]
fn the_slider_is_named_has_its_range_and_a_reset_action() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture(3.0, plain);
        let slider = node(&mut f.tree, f.slider);
        assert_eq!(slider.label(), Some("Nivel dB"));
        assert_eq!(slider.min_numeric_value(), Some(-4.0));
        assert_eq!(slider.max_numeric_value(), Some(24.0));
        let actions = slider.custom_actions().to_vec();
        assert_eq!(&*actions[0].description, "Restablecer al valor por defecto");
        f.tree.dispatch_event(WidgetEvent::AccessAction {
            action: Action::CustomAction,
            target: Some(f.slider),
            target_node: widget_id_to_node_id(f.slider),
            data: Some(ActionData::CustomAction(actions[0].id)),
        });
        layout(&mut f.tree);
        assert_eq!(f.value.get(), 0.0);
    });
}

#[test]
fn the_open_field_is_a_text_input_named_after_the_parameter() {
    let mut f = fixture(3.0, plain);
    click(&mut f.tree, input_point(), PointerButton::Primary, 10);
    let field = f.tree.focused().unwrap();
    let node = node(&mut f.tree, field);
    assert_eq!(node.role(), Role::TextInput);
    assert_eq!(node.label(), Some("Nivel dB"));
}
