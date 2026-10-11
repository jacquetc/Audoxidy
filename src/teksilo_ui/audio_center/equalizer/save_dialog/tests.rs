//! Tests of the Save dialog: its size and position, the field and buttons,
//! saving by button and by Enter, and the empty name.

use teksilo::canvas::Point;
use teksilo::core::Key;
use teksilo::core::accesskit::Role;
use teksilo::core::widget_tree::WidgetTree;

use super::*;
use crate::teksilo_ui::audio_center::equalizer::equalizer_vm::tests::Fixture;
use crate::teksilo_ui::test_support::{
    access_click, find_by_name, key_down, layout, tap_at, type_text,
};

struct Open {
    f: Fixture,
    layer: WidgetId,
    dialog: WidgetId,
}

fn open_dialog() -> Open {
    let mut f = Fixture::new();
    let vm = f.vm.clone();
    f.press(move |ctx| open(&vm, ctx));
    let (layer, _) = f.open_modal();
    let dialog = find_by_name(&f.tree, layer, "Guardar Preset")
        .into_iter()
        .find(|id| f.tree.accessibility_node(*id).role() == Role::Dialog)
        .expect("the dialog node");
    Open { f, layer, dialog }
}

fn rect(tree: &WidgetTree, id: WidgetId) -> (f32, f32, f32, f32) {
    let b = tree.bounds(id);
    (b.x, b.y, b.width, b.height)
}

impl Open {
    fn named(&self, name: &str) -> WidgetId {
        match find_by_name(&self.f.tree, self.layer, name).first() {
            Some(id) => *id,
            None => panic!("no widget named {name:?}"),
        }
    }

    fn field(&self) -> WidgetId {
        self.f
            .tree
            .find_by_role(Role::TextInput)
            .expect("the name field")
    }

    fn type_name(&mut self, text: &str) {
        let field = self.field();
        self.f.tree.focus(field);
        type_text(&mut self.f.tree, text);
    }
}

#[test]
fn the_dialog_is_190_by_162_at_iceds_position() {
    let o = open_dialog();
    let (x, y, w, h) = rect(&o.f.tree, o.dialog);
    assert_eq!((w, h), (190.0, 162.0));
    // Panel at (210, 200): x + 940 - 194 - 15, y + 120.
    assert_eq!((x, y), (210.0 + 731.0, 320.0));
    o.f.finish();
}

#[test]
fn the_field_is_160_wide_and_the_buttons_74_by_30_eight_apart() {
    let o = open_dialog();
    let field = rect(&o.f.tree, o.field());
    // The editor inside the 160 px field, less its 5 px side padding.
    assert_eq!(field.2, 150.0);
    let cancel = rect(&o.f.tree, o.named("Cancelar"));
    let save = rect(&o.f.tree, o.named("Guardar"));
    assert_eq!((cancel.2, cancel.3), (74.0, 30.0));
    assert_eq!((save.2, save.3), (74.0, 30.0));
    assert_eq!(save.0 - (cancel.0 + cancel.2), 8.0);
    o.named("Nombre del preset:");
    o.f.finish();
}

#[test]
fn guardar_is_disabled_until_a_name_is_typed() {
    let mut o = open_dialog();
    let save = o.named("Guardar");
    assert!(o.f.tree.accessibility_node(save).is_disabled());
    o.type_name("Warm");
    layout(&mut o.f.tree);
    assert!(!o.f.tree.accessibility_node(save).is_disabled());
    o.f.finish();
}

#[test]
fn guardar_saves_the_preset_and_closes() {
    let mut o = open_dialog();
    o.f.vm.set_band_gain(1, 2.5);
    o.type_name("Warm");
    let save = o.named("Guardar");
    access_click(&mut o.f.tree, save);
    assert!(o.f.tree.drain_pending_modal_dismissal());
    let model = o.f.services.eq_presets.clone();
    o.f.wait(|| model.entry("Warm").is_some());
    assert_eq!(model.entry("Warm").unwrap().preset.get_gains_20()[1], 2.5);
    o.f.finish();
}

#[test]
fn enter_saves_and_an_empty_name_closes_without_saving() {
    let mut o = open_dialog();
    o.type_name("Bright");
    let field = o.field();
    key_down(&mut o.f.tree, field, Key::Enter);
    assert!(o.f.tree.drain_pending_modal_dismissal());
    let model = o.f.services.eq_presets.clone();
    o.f.wait(|| model.entry("Bright").is_some());

    let mut o2 = open_dialog();
    let before = o2.f.names();
    let field = o2.field();
    key_down(&mut o2.f.tree, field, Key::Enter);
    assert!(
        o2.f.tree.drain_pending_modal_dismissal(),
        "Iced closes it too"
    );
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(o2.f.names(), before);
    o.f.finish();
    o2.f.finish();
}

#[test]
fn cancelar_and_a_press_outside_close_without_saving() {
    let mut o = open_dialog();
    o.type_name("Nope");
    let cancel = o.named("Cancelar");
    access_click(&mut o.f.tree, cancel);
    assert!(o.f.tree.drain_pending_modal_dismissal());
    let (x, y, w, _) = rect(&o.f.tree, o.dialog);
    tap_at(&mut o.f.tree, Point::new(x + w + 30.0, y + 5.0));
    assert!(o.f.tree.drain_pending_modal_dismissal());
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(o.f.services.eq_presets.entry("Nope").is_none());
    o.f.finish();
}
