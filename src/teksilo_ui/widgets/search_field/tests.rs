use teksilo::core::accesskit::Role;
use teksilo::core::{Key, Modifiers, WidgetTree};
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::teksilo_ui::test_support::{access_click, find_by_name, layout, with_shipped_messages};

fn mounted(text: Signal<String>) -> (WidgetTree, WidgetId) {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let id = tree.add(SearchField::new(text, lit!("Buscar..."), 180.0));
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

fn field(tree: &WidgetTree) -> WidgetId {
    tree.find_by_role(Role::TextInput).unwrap()
}

#[test]
fn the_strip_is_the_callers_width_and_24_pixels_high() {
    let (tree, id) = mounted(Signal::new(String::new()));
    let bounds = tree.bounds(id);
    assert_eq!((bounds.width, bounds.height), (180.0, HEIGHT));
}

#[test]
fn the_text_is_centred_in_the_strip_and_flush_with_its_leading_edge() {
    let (tree, id) = mounted(Signal::new(String::new()));
    let strip = tree.bounds(id);
    let text = tree.bounds(field(&tree));
    assert_eq!(text.x, strip.x);
    assert_eq!(text.center().y, strip.center().y);
    assert_eq!(text.width, 180.0 - TRAILING_RESERVE);
}

#[test]
fn the_field_is_named_after_its_placeholder() {
    let (tree, _) = mounted(Signal::new(String::new()));
    assert_eq!(
        tree.accessibility_node(field(&tree)).name(),
        Some("Buscar...")
    );
}

#[test]
fn the_clear_button_shows_only_while_there_is_text() {
    with_shipped_messages("en-US", || {
        let text = Signal::new(String::new());
        let (mut tree, id) = mounted(text.clone());
        let clear = find_by_name(&tree, id, "Clear search")[0];
        assert!(!tree.is_visible(clear));
        text.set("bowie".to_string());
        layout(&mut tree);
        assert!(tree.is_visible(clear));
        let strip = tree.bounds(id);
        let button = tree.bounds(clear);
        assert_eq!((button.width, button.height), (CLEAR_SIZE, CLEAR_SIZE));
        assert_eq!(
            strip.x + strip.width - (button.x + button.width),
            CLEAR_TRAILING
        );
        assert_eq!(button.center().y, strip.center().y);
    });
}

#[test]
fn the_clear_button_empties_the_query() {
    with_shipped_messages("en-US", || {
        let text = Signal::new("bowie".to_string());
        let (mut tree, id) = mounted(text.clone());
        let clear = find_by_name(&tree, id, "Clear search")[0];
        access_click(&mut tree, clear);
        assert_eq!(text.get(), "");
        assert!(!tree.is_visible(clear));
    });
}

#[test]
fn typing_writes_the_query() {
    let text = Signal::new(String::new());
    let (mut tree, _) = mounted(text.clone());
    let input = field(&tree);
    tree.focus(input);
    tree.type_text(input, "ziggy stardust");
    layout(&mut tree);
    assert_eq!(text.get(), "ziggy stardust");
}

/// The search field under a container that counts the Escape presses that
/// reach it, as an overlay closing on Escape would.
fn mounted_under_escape_counter(
    text: Signal<String>,
) -> (WidgetTree, std::rc::Rc<std::cell::Cell<u32>>) {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let escapes = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = escapes.clone();
    let id = tree.add(SearchField::new(text, lit!("Buscar..."), 180.0));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(id)
            .child(Spacer::new())
            .on_key(move |event, _ctx| match event {
                teksilo::core::WidgetEvent::KeyDown {
                    key: Key::Escape, ..
                } => {
                    seen.set(seen.get() + 1);
                    EventResponse::Handled
                }
                _ => EventResponse::Ignored,
            }),
    );
    layout(&mut tree);
    let _ = tree.render();
    (tree, escapes)
}

#[test]
fn escape_clears_the_query_without_reaching_the_overlay() {
    let text = Signal::new("ziggy".to_string());
    let (mut tree, escapes) = mounted_under_escape_counter(text.clone());
    tree.focus(field(&tree));
    tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut tree);
    assert_eq!(text.get(), "");
    assert_eq!(escapes.get(), 0);
}

#[test]
fn escape_on_an_empty_field_reaches_the_overlay() {
    let text = Signal::new(String::new());
    let (mut tree, escapes) = mounted_under_escape_counter(text.clone());
    tree.focus(field(&tree));
    tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut tree);
    assert_eq!(text.get(), "");
    assert_eq!(escapes.get(), 1);
}

#[test]
fn the_spanish_clear_button_is_borrar_busqueda() {
    with_shipped_messages("es-ES", || {
        let (tree, id) = mounted(Signal::new("x".to_string()));
        assert_eq!(find_by_name(&tree, id, "Borrar búsqueda").len(), 1);
    });
}
