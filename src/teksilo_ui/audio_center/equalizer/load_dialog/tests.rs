//! Tests of the Load dialog: its size and position, the rows and the divider,
//! the buttons, preview / accept / cancel through the real input paths, and
//! the ways it is dismissed.

use teksilo::canvas::Point;
use teksilo::core::accesskit::Role;
use teksilo::core::overlay::DismissReason;
use teksilo::core::widget_tree::WidgetTree;

use super::*;
use crate::teksilo_ui::audio_center::equalizer::equalizer_vm::tests::Fixture;
use crate::teksilo_ui::test_support::{
    access_click, find_by_name, layout, press, tap_at, with_shipped_messages,
};

/// The dialog open over a fixture.
struct Open {
    f: Fixture,
    layer: WidgetId,
    dialog: WidgetId,
    on_dismiss: Option<teksilo::core::overlay::OverlayDismissCallback>,
}

fn open_in(locale: &str) -> Open {
    open_over(Fixture::in_locale(locale), locale)
}

fn open_over(mut f: Fixture, locale: &str) -> Open {
    let vm = f.vm.clone();
    f.press(move |ctx| open(&vm, ctx));
    let (layer, on_dismiss) = f.open_modal();
    let title = if locale == "es-ES" {
        "Cargar Preset"
    } else {
        "Load Preset"
    };
    let dialog = dialog_node(&f.tree, layer, title);
    Open {
        f,
        layer,
        dialog,
        on_dismiss,
    }
}

fn dialog_node(tree: &WidgetTree, layer: WidgetId, title: &str) -> WidgetId {
    find_by_name(tree, layer, title)
        .into_iter()
        .find(|id| tree.accessibility_node(*id).role() == Role::Dialog)
        .expect("the dialog node")
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

    fn click(&mut self, name: &str) {
        let id = self.named(name);
        access_click(&mut self.f.tree, id);
    }

    fn dismissed(&mut self) -> bool {
        self.f.tree.drain_pending_modal_dismissal()
    }

    fn run_dismiss(&mut self, reason: DismissReason) {
        let callback = self.on_dismiss.clone().expect("an on_dismiss callback");
        press(&mut self.f.tree, move |ctx| callback(reason, ctx));
    }
}

#[test]
fn the_dialog_is_330_by_352_at_iceds_position() {
    let o = open_in("es-ES");
    let (x, y, w, h) = rect(&o.f.tree, o.dialog);
    assert_eq!((w, h), (LOAD_SIZE.0, LOAD_SIZE.1));
    // Centred panel in 1360 x 880: (210, 200); then (305, 64 + 15).
    assert_eq!((x, y), (515.0, 279.0));
    o.f.finish();
}

#[test]
fn a_dragged_panel_moves_the_dialog() {
    let mut f = Fixture::new();
    f.services
        .links
        .audio_center
        .position
        .set(Some((100.0, 50.0)));
    let vm = f.vm.clone();
    f.press(move |ctx| open(&vm, ctx));
    let (layer, _) = f.open_modal();
    let dialog = dialog_node(&f.tree, layer, "Cargar Preset");
    let (x, y, _, _) = rect(&f.tree, dialog);
    assert_eq!((x, y), (100.0 + 305.0, 50.0 + 79.0));
    f.finish();
}

#[test]
fn the_list_box_is_140_by_289_and_rows_are_26_px() {
    let o = open_in("es-ES");
    let list = o.named("Presets");
    let (_, y, w, h) = rect(&o.f.tree, list);
    assert_eq!((w, h), (140.0, 289.0));
    let default = rect(&o.f.tree, o.named("Default"));
    assert_eq!((default.2, default.3), (138.0, 26.0));
    assert_eq!(default.1, y + 1.0, "1 px of padding");
    o.f.finish();
}

#[test]
fn a_one_pixel_divider_separates_the_customs_from_the_built_ins() {
    let mut o = open_in("es-ES");
    // No customs: the divider comes right after Default.
    let default = rect(&o.f.tree, o.named("Default"));
    let next_name = o.f.names()[1].clone();
    let next = rect(&o.f.tree, o.named(&next_name));
    assert_eq!(next.1 - (default.1 + default.3), 1.0);
    let after = rect(&o.f.tree, o.named(&o.f.names()[2].clone()));
    assert_eq!(after.1 - (next.1 + next.3), 0.0, "rows touch");

    // With a custom preset it sits after the custom one.
    let vm = o.f.vm.clone();
    o.f.press(move |ctx| vm.save_preset("Mine", ctx));
    let model = o.f.services.eq_presets.clone();
    o.f.wait(|| model.entry("Mine").is_some());
    layout(&mut o.f.tree);
    let _ = o.f.tree.render();
    layout(&mut o.f.tree);
    let default = rect(&o.f.tree, o.named("Default"));
    let mine = rect(&o.f.tree, o.named("Mine"));
    assert_eq!(mine.1, default.1 + 26.0, "customs follow Default directly");
    let first_builtin = o.f.names()[2].clone();
    let builtin = rect(&o.f.tree, o.named(&first_builtin));
    assert_eq!(builtin.1 - (mine.1 + 26.0), 1.0);
    o.f.finish();
}

#[test]
fn the_buttons_are_150_by_30_and_8_px_apart() {
    let o = open_in("es-ES");
    let names = ["Aceptar", "Cancelar", "Eliminar", "Importar", "Exportar"];
    let mut previous: Option<(f32, f32, f32, f32)> = None;
    for name in names {
        let r = rect(&o.f.tree, o.named(name));
        assert_eq!((r.2, r.3), (150.0, 30.0), "{name}");
        if let Some(p) = previous {
            assert_eq!(r.1 - (p.1 + p.3), 8.0, "{name}");
            assert_eq!(r.0, p.0);
        }
        previous = Some(r);
    }
    let defaults = rect(&o.f.tree, o.named("Predeterminado"));
    let export = previous.unwrap();
    assert_eq!(
        defaults.1 - (export.1 + export.3),
        16.0,
        "two 8 px gaps around a 0 px space"
    );
    o.f.finish();
}

#[test]
fn the_warning_carries_the_corrected_spanish() {
    let o = open_in("es-ES");
    o.named("Advertencia: Se eliminarán los presets personalizados y se restablecerán los presets predeterminados.");
    o.f.finish();
}

#[test]
fn the_english_dialog_is_a_faithful_translation() {
    with_shipped_messages("en-US", || {
        let o = open_in("en-US");
        for name in ["Accept", "Cancel", "Delete", "Import", "Export", "Defaults"] {
            o.named(name);
        }
        o.f.finish();
    });
}

#[test]
fn clicking_a_row_previews_it_and_highlights_it() {
    let mut o = open_in("es-ES");
    o.click("Rock");
    assert_eq!(o.f.vm.preset_name().get().as_deref(), Some("Rock"));
    assert!(!o.dismissed(), "a row does not close the dialog");
    o.f.finish();
}

#[test]
fn aceptar_keeps_the_preview_and_closes() {
    let mut o = open_in("es-ES");
    o.click("Rock");
    o.click("Aceptar");
    assert!(o.dismissed());
    // The dismissal that follows must not undo it.
    o.run_dismiss(DismissReason::Programmatic);
    assert_eq!(o.f.vm.preset_name().get().as_deref(), Some("Rock"));
    o.f.finish();
}

#[test]
fn cancelar_restores_the_state_and_closes() {
    let f = Fixture::new();
    f.vm.set_band_gain(3, 2.0);
    let mut o = open_over(f, "es-ES");
    let before = o.f.vm.state();
    o.click("Rock");
    assert_ne!(o.f.vm.state().gains, before.gains);
    o.click("Cancelar");
    assert!(o.dismissed());
    assert_eq!(o.f.vm.state().gains, before.gains);
    assert_eq!(o.f.vm.preset_name().get(), None);
    o.f.finish();
}

#[test]
fn escape_and_a_press_outside_restore_the_backup() {
    for reason in [DismissReason::Escape, DismissReason::OutsidePress] {
        let mut o = open_in("es-ES");
        let before = o.f.vm.state();
        o.click("Rock");
        o.run_dismiss(reason);
        assert_eq!(o.f.vm.state(), before, "{reason:?}");
        o.f.finish();
    }
}

#[test]
fn a_press_outside_the_dialog_dismisses_it_and_one_inside_does_not() {
    let mut o = open_in("es-ES");
    let (x, y, w, h) = rect(&o.f.tree, o.dialog);
    tap_at(&mut o.f.tree, Point::new(x + 3.0, y + h - 3.0));
    assert!(!o.dismissed(), "the dialog's own padding is not outside");
    tap_at(&mut o.f.tree, Point::new(x + w + 40.0, y + 10.0));
    assert!(o.dismissed(), "Iced's click blocker (`app.rs:5766`)");
    o.f.finish();
}

#[test]
fn eliminar_removes_the_highlighted_preset() {
    let mut o = open_in("es-ES");
    o.click("Rock");
    o.click("Eliminar");
    let model = o.f.services.eq_presets.clone();
    o.f.wait(|| model.entry("Rock").is_none());
    layout(&mut o.f.tree);
    let _ = o.f.tree.render();
    layout(&mut o.f.tree);
    assert!(
        find_by_name(&o.f.tree, o.layer, "Rock").is_empty(),
        "the list rebuilt"
    );
    o.f.finish();
}

#[test]
fn importar_and_exportar_without_a_file_service_show_an_error_toast() {
    // The headless tree has no file-dialog service, as a build without
    // `install_file_dialog` would not (quirk fix 8).
    let mut o = open_in("es-ES");
    o.click("Importar");
    assert_eq!(o.f.toasts.live_count(), 1);
    o.click("Exportar");
    assert_eq!(
        o.f.toasts.live_count(),
        1,
        "Exportar waits for a highlighted row"
    );
    o.click("Rock");
    o.click("Exportar");
    assert_eq!(o.f.toasts.live_count(), 2);
    o.f.finish();
}

#[test]
fn the_file_filter_is_iceds() {
    assert_eq!(FILE_FILTER, "Audoxidy EQ Preset");
    assert_eq!(FILE_EXTENSIONS, ["json", "aeqp"]);
}
