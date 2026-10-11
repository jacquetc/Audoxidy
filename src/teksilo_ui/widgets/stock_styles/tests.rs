use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Checkbox, ComboBox, FixedSize, MinSize, RadioButton, Toggle, VStack};

use super::*;
use crate::teksilo_ui::test_support::{access_click, first_of_type, layout};

/// `widget` at the top-leading corner of a window-sized column (a tree's root
/// always takes the whole viewport).
fn mounted(widget: impl Widget + 'static) -> (WidgetTree, WidgetId) {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
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

fn size_of(tree: &WidgetTree, id: WidgetId) -> (f32, f32) {
    let bounds = tree.bounds(id);
    (bounds.width, bounds.height)
}

#[test]
fn each_toggler_look_is_twice_as_wide_as_its_iced_size() {
    for (look, size) in [
        (ToggleLook::equalizer(), 18.0),
        (ToggleLook::effect(), 17.0),
        (ToggleLook::expander_mode(), 13.0),
    ] {
        let (tree, id) = mounted(toggle(
            Toggle::new(Signal::new(true)),
            look,
            lit!("Activar Ecualizador"),
        ));
        assert_eq!(size_of(&tree, id), (2.0 * size, size));
        assert_eq!(
            tree.accessibility_node(id).name(),
            Some("Activar Ecualizador")
        );
    }
}

#[test]
fn the_thumb_is_inset_by_a_rounded_fifth_of_the_size() {
    let inset_and_diameter = |look: ToggleLook| (look.thumb_inset(), look.thumb_diameter());
    assert_eq!(inset_and_diameter(ToggleLook::equalizer()), (4.0, 10.0));
    assert_eq!(inset_and_diameter(ToggleLook::effect()), (3.0, 11.0));
    assert_eq!(inset_and_diameter(ToggleLook::expander_mode()), (3.0, 7.0));
}

#[test]
fn a_styled_toggle_still_flips_as_a_switch() {
    let on = Signal::new(false);
    let (mut tree, id) = mounted(toggle(
        Toggle::new(on.clone()),
        ToggleLook::effect(),
        lit!("Compresor"),
    ));
    assert_eq!(tree.accessibility_node(id).role(), Role::Switch);
    access_click(&mut tree, id);
    assert!(on.get());
    let _ = tree.render();
}

#[test]
fn the_checkbox_draws_its_iced_square_inside_the_hit_area() {
    let checked = Signal::new(false);
    let (mut tree, id) = mounted(checkbox(Checkbox::new(checked.clone()), lit!("Fundido")));
    let square = first_of_type(&tree, id, "CheckboxBox").unwrap();
    assert_eq!(size_of(&tree, square), (CHECKBOX_SIZE, CHECKBOX_SIZE));
    assert_eq!(tree.accessibility_node(id).role(), Role::CheckBox);
    assert_eq!(tree.accessibility_node(id).name(), Some("Fundido"));
    access_click(&mut tree, id);
    assert!(checked.get());
    let _ = tree.render();
}

#[test]
fn the_check_glyph_shows_only_while_checked() {
    let checked = Signal::new(false);
    let (mut tree, id) = mounted(
        Checkbox::new(checked.clone())
            .labelled_externally()
            .style(checkbox_style()),
    );
    let glyph = first_of_type(&tree, id, "TextWidget").unwrap();
    assert!(!tree.is_visible(glyph));
    checked.set(true);
    layout(&mut tree);
    assert!(tree.is_visible(glyph));
}

#[test]
fn the_radio_circle_is_sixteen_pixels_and_its_label_eight_pixels_after_it() {
    let selected = Signal::new(1_usize);
    let (tree, id) = mounted(radio(0, selected, lit!("20")));
    let circle = first_of_type(&tree, id, "RadioCircle").unwrap();
    assert_eq!(size_of(&tree, circle), (RADIO_SIZE, RADIO_SIZE));
    let label = first_of_type(&tree, id, "TextWidget").unwrap();
    let circle_end = tree.bounds(circle).x + RADIO_SIZE;
    assert_eq!(tree.bounds(label).x - circle_end, RADIO_LABEL_SPACING);
}

#[test]
fn a_click_on_the_radio_or_its_label_selects_the_value() {
    let selected = Signal::new(1_usize);
    let (mut tree, id) = mounted(radio(0, selected.clone(), lit!("20")));
    let button = first_of_type(&tree, id, "RadioButton").unwrap();
    assert_eq!(tree.accessibility_node(button).role(), Role::RadioButton);
    assert_eq!(tree.accessibility_node(button).name(), Some("20"));
    access_click(&mut tree, button);
    assert_eq!(selected.get(), 0);

    selected.set(1);
    layout(&mut tree);
    let label = first_of_type(&tree, id, "TextWidget").unwrap();
    tree.click(label);
    layout(&mut tree);
    assert_eq!(selected.get(), 0);
}

#[test]
fn the_combo_box_trigger_lights_up_with_the_accent_while_hovered() {
    let value = Signal::new(Some("44100".to_string()));
    let (mut tree, id) = mounted(
        FixedSize::new()
            .width(125.0)
            .child(combo_box(ComboBox::new(["44100", "48000"], value)).label(lit!("Muestreo"))),
    );
    let combo = first_of_type(&tree, id, "ComboBox<alloc::string::String>")
        .or_else(|| first_of_type(&tree, id, "String>"))
        .unwrap();
    assert_eq!(tree.accessibility_node(combo).role(), Role::ComboBox);
    assert_eq!(tree.bounds(combo).width, 125.0);
    let center = tree.bounds(combo).center();
    tree.pointer_move(center);
    layout(&mut tree);
    let _ = tree.render();
}

#[test]
fn a_thumb_paints_the_trailing_half_of_the_bar() {
    let bar = Rect::new(100.0, 0.0, 8.0, 400.0);
    let thumb = thumb_rect(bar, true, 0.0, 0.25, 24.0, false).unwrap();
    assert_eq!(thumb, Rect::new(104.0, 0.0, 4.0, 100.0));
    let end = thumb_rect(bar, true, 1.0, 0.25, 24.0, false).unwrap();
    assert_eq!(end.y + end.height, 400.0);

    let bar = Rect::new(0.0, 50.0, 300.0, 8.0);
    let thumb = thumb_rect(bar, false, 0.0, 0.5, 24.0, false).unwrap();
    assert_eq!(thumb, Rect::new(0.0, 54.0, 150.0, 4.0));
}

#[test]
fn a_thumb_keeps_its_minimum_length_and_mirrors_in_right_to_left() {
    let bar = Rect::new(0.0, 0.0, 8.0, 400.0);
    let thumb = thumb_rect(bar, true, 0.5, 0.001, 24.0, false).unwrap();
    assert_eq!(thumb.height, 24.0);

    let bar = Rect::new(0.0, 0.0, 300.0, 8.0);
    let start = thumb_rect(bar, false, 0.0, 0.5, 24.0, true).unwrap();
    assert_eq!(start.x, 150.0);
}

#[test]
fn content_that_fits_has_no_thumb() {
    let bar = Rect::new(0.0, 0.0, 8.0, 400.0);
    assert_eq!(thumb_rect(bar, true, 0.0, 1.0, 24.0, false), None);
}

#[test]
fn a_scroll_area_over_tall_content_draws_an_eight_pixel_bar() {
    let mut theme = style::audoxidy_dark();
    install(&mut theme.style_slots);
    let mut tree = WidgetTree::new();
    tree.set_theme(theme);
    let area = tree.add(scroll_area().child(MinSize::new(100.0, 2000.0).child(Spacer::new())));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(FixedSize::new().width(200.0).height(300.0).child(area))
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    let thumb = first_of_type(&tree, area, "ScrollThumb").unwrap();
    assert_eq!(tree.bounds(thumb).width, SCROLL_BAR_THICKNESS);
}

#[test]
fn install_fills_the_slots_of_the_restyled_controls() {
    let mut theme = teksilo::prelude::intui::dark();
    install(&mut theme.style_slots);
    let installed = theme.style_slots.installed();
    for slot in ["checkbox", "radio", "combo_box", "scroll_bar", "toggle"] {
        assert!(installed.contains(&slot), "{slot} in {installed:?}");
    }
}

#[test]
fn a_disabled_radio_button_ignores_a_click() {
    let selected = Signal::new(1_usize);
    let (mut tree, id) = mounted(
        RadioButton::new(0, selected.clone())
            .style(radio_style())
            .enabled(false)
            .access_label(lit!("20")),
    );
    access_click(&mut tree, id);
    assert_eq!(selected.get(), 1);
    let _ = tree.render();
}
