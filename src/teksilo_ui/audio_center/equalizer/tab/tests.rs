//! Tests of the tab body: the geometry of the controls and the slider rows at
//! Iced's sizes, the interaction through the real input paths, the dialogs the
//! icons open, the accessible names and the Spanish strings.

use teksilo::canvas::Point;
use teksilo::core::PointerButton;
use teksilo::core::accesskit::Role;
use teksilo::widgets::{FixedSize, Spacer, VStack};

use super::*;
use crate::teksilo_ui::audio_center::equalizer::equalizer_vm::tests::Fixture;
use crate::teksilo_ui::test_support::{
    access_click, drain_dialog_titles, find_by_name, first_of_type, layout, pointer_click,
    with_shipped_messages,
};

/// The tab body's size in the Audio Center: 940 less its 2 px frame and 15 px
/// side padding (`audio_center.rs:2007`, `1999`).
const BODY: (f32, f32) = (940.0 - 4.0 - 30.0, 410.0);

struct Mounted {
    f: Fixture,
    tab: WidgetId,
}

fn mounted() -> Mounted {
    mounted_in("es-ES")
}

fn mounted_in(locale: &str) -> Mounted {
    let mut f = Fixture::in_locale(locale);
    let tab = f.tree.add(EqualizerTab::new(f.vm.clone()));
    f.tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(FixedSize::new().width(BODY.0).height(BODY.1).child(tab))
            .child(Spacer::new()),
    );
    layout(&mut f.tree);
    let _ = f.tree.render();
    Mounted { f, tab }
}

fn all_of_type(
    tree: &teksilo::core::widget_tree::WidgetTree,
    root: WidgetId,
    suffix: &str,
) -> Vec<WidgetId> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if tree
            .widget_type_name(id)
            .is_some_and(|name| name.ends_with(suffix))
        {
            found.push(id);
        }
        let mut children = tree.children(id);
        children.reverse();
        stack.extend(children);
    }
    found
}

fn rect(m: &Mounted, id: WidgetId) -> (f32, f32, f32, f32) {
    let b = m.f.tree.bounds(id);
    (b.x, b.y, b.width, b.height)
}

/// The band columns (the children of the bands row).
fn columns(m: &Mounted) -> Vec<WidgetId> {
    let bands = first_of_type(&m.f.tree, m.tab, "Bands").unwrap();
    let row = m.f.tree.children(bands)[0];
    m.f.tree.children(row)
}

fn named(m: &Mounted, name: &str) -> WidgetId {
    match find_by_name(&m.f.tree, m.tab, name).first() {
        Some(id) => *id,
        None => panic!("no widget named {name:?}"),
    }
}

fn rebuild(m: &mut Mounted) {
    for _ in 0..3 {
        layout(&mut m.f.tree);
        let _ = m.f.tree.render();
    }
}

// --- Geometry ------------------------------------------------------------------------------

#[test]
fn twenty_bands_and_the_preamp_are_laid_out_20_18_px_apart() {
    let m = mounted();
    let sliders = all_of_type(&m.f.tree, m.tab, "EqSlider");
    assert_eq!(sliders.len(), 21, "the preamp and 20 bands");
    let cols = columns(&m);
    assert_eq!(cols.len(), 20);
    for pair in cols.windows(2) {
        let (a, b) = (m.f.tree.bounds(pair[0]), m.f.tree.bounds(pair[1]));
        assert!(
            (b.x - (a.x + a.width) - SPACING_20).abs() < 0.02,
            "{} {}",
            a.x + a.width,
            b.x
        );
    }
    for slider in &sliders {
        let (_, _, w, h) = rect(&m, *slider);
        assert_eq!((w, h), (24.0, 240.0));
    }
    m.f.finish();
}

#[test]
fn thirty_one_bands_are_laid_out_3_98_px_apart() {
    let mut m = mounted();
    m.f.vm.set_bands_31(true);
    rebuild(&mut m);
    let cols = columns(&m);
    assert_eq!(cols.len(), 31);
    for pair in cols.windows(2) {
        let (a, b) = (m.f.tree.bounds(pair[0]), m.f.tree.bounds(pair[1]));
        assert!((b.x - (a.x + a.width) - SPACING_31).abs() < 0.02);
    }
    assert_eq!(all_of_type(&m.f.tree, m.tab, "EqSlider").len(), 32);
    m.f.finish();
}

#[test]
fn the_slider_row_starts_19_plus_the_top_row_plus_25_px_down() {
    let m = mounted();
    let tab = rect(&m, m.tab);
    let preamp = all_of_type(&m.f.tree, m.tab, "EqSlider")[0];
    let top = rect(&m, preamp).1 - tab.1;
    // 19 (space) + top row + 25 (space) + the label (10.4 px text) + 10.
    assert!(
        top > 19.0 + 25.0 + 10.0 + 10.0 && top < 19.0 + 25.0 + 10.0 + 10.0 + 60.0,
        "{top}"
    );
    m.f.finish();
}

#[test]
fn the_hint_sits_at_the_bottom_of_the_tab() {
    let m = mounted();
    let hint = named(
        &m,
        "*Puede restablecer al valor por defecto haciendo clic secundario sobre un deslizador.",
    );
    let (_, y, _, h) = rect(&m, hint);
    let tab = rect(&m, m.tab);
    assert!(
        (tab.1 + tab.3 - (y + h)).abs() < 1.0,
        "the hint ends at the tab's bottom"
    );
    m.f.finish();
}

#[test]
fn the_top_row_is_three_equal_parts() {
    let m = mounted();
    let tab = rect(&m, m.tab);
    let toggle = rect(&m, first_of_type(&m.f.tree, m.tab, "Toggle").unwrap());
    let load = rect(&m, named(&m, "Cargar preset"));
    let reset = rect(&m, named(&m, "Restablecer ecualizador"));
    // The label (about 140 px) leads, then 10 px, then the toggle.
    assert!(
        toggle.0 - tab.0 > 100.0 && toggle.0 - tab.0 < 200.0,
        "the toggle part is at the leading edge: {}",
        toggle.0 - tab.0
    );
    assert!(
        (tab.0 + tab.2 - (reset.0 + reset.2)).abs() < 1.0,
        "the preset part ends at the trailing edge"
    );
    assert!(
        load.0 > tab.0 + tab.2 * 2.0 / 3.0 - 20.0,
        "the icons are in the last third"
    );
    // 14 px icons in 20 px squares, 7 px apart.
    assert_eq!((load.2, load.3), (20.0, 20.0));
    let save = rect(&m, named(&m, "Guardar preset"));
    assert!((save.0 - (load.0 + load.2) - 7.0).abs() < 0.5);
    m.f.finish();
}

// --- Interaction ---------------------------------------------------------------------------

#[test]
fn the_toggle_switches_the_equalizer() {
    let mut m = mounted();
    let toggle = first_of_type(&m.f.tree, m.tab, "Toggle").unwrap();
    access_click(&mut m.f.tree, toggle);
    assert!(m.f.vm.state().enabled);
    access_click(&mut m.f.tree, toggle);
    assert!(!m.f.vm.state().enabled);
    m.f.finish();
}

#[test]
fn the_radios_switch_the_band_set_and_rebuild_the_sliders() {
    let mut m = mounted();
    let r31 = named(&m, "31");
    access_click(&mut m.f.tree, r31);
    assert!(m.f.vm.state().bands_31);
    rebuild(&mut m);
    assert_eq!(columns(&m).len(), 31);
    let r20 = named(&m, "20");
    access_click(&mut m.f.tree, r20);
    assert!(!m.f.vm.state().bands_31);
    rebuild(&mut m);
    assert_eq!(columns(&m).len(), 20);
    m.f.finish();
}

#[test]
fn a_secondary_click_on_a_band_resets_it_and_a_primary_press_sets_it() {
    let mut m = mounted();
    let slider = all_of_type(&m.f.tree, m.tab, "EqSlider")[1];
    let (x, y, w, h) = rect(&m, slider);
    // A press near the top is near +9 dB.
    pointer_click(
        &mut m.f.tree,
        Point::new(x + w / 2.0, y + 8.0),
        PointerButton::Primary,
        100,
    );
    assert!(
        m.f.vm.band_gain(0).get() > 6.0,
        "{}",
        m.f.vm.band_gain(0).get()
    );
    assert_eq!(m.f.vm.state().gains[0], m.f.vm.band_gain(0).get());
    pointer_click(
        &mut m.f.tree,
        Point::new(x + w / 2.0, y + h / 2.0),
        PointerButton::Secondary,
        600,
    );
    assert_eq!(m.f.vm.band_gain(0).get(), 0.0);
    assert_eq!(m.f.vm.state().gains[0], 0.0);
    m.f.finish();
}

#[test]
fn the_reset_icon_flattens_everything() {
    let mut m = mounted();
    m.f.vm.set_band_gain(2, 4.0);
    m.f.vm.set_preamp(3.0);
    let reset = named(&m, "Restablecer ecualizador");
    access_click(&mut m.f.tree, reset);
    assert_eq!(m.f.vm.preamp().get(), 0.0);
    assert_eq!(m.f.vm.band_gain(2).get(), 0.0);
    m.f.finish();
}

#[test]
fn the_preset_label_exists_only_while_a_preset_other_than_default_is_selected() {
    let mut m = mounted();
    assert!(find_by_name(&m.f.tree, m.tab, "Preset: Rock").is_empty());
    let load = m.f.vm.begin_load();
    load.select(m.f.row("Rock"));
    load.accept();
    rebuild(&mut m);
    assert_eq!(find_by_name(&m.f.tree, m.tab, "Preset: Rock").len(), 1);
    let load = m.f.vm.begin_load();
    load.select(m.f.row("Default"));
    rebuild(&mut m);
    assert!(
        find_by_name(&m.f.tree, m.tab, "Preset: Default").is_empty(),
        "Default shows no label (`audio_center.rs:2813`)"
    );
    m.f.finish();
}

#[test]
fn the_load_and_save_icons_open_their_dialogs() {
    let mut m = mounted();
    let load = named(&m, "Cargar preset");
    access_click(&mut m.f.tree, load);
    assert_eq!(drain_dialog_titles(&mut m.f.tree), vec!["Cargar Preset"]);
    let save = named(&m, "Guardar preset");
    access_click(&mut m.f.tree, save);
    assert_eq!(drain_dialog_titles(&mut m.f.tree), vec!["Guardar Preset"]);
    m.f.finish();
}

// --- Names and Spanish ---------------------------------------------------------------------

#[test]
fn the_controls_have_accessible_names_in_spanish() {
    with_shipped_messages("es-ES", || {
        let m = mounted_in("es-ES");
        let toggle = first_of_type(&m.f.tree, m.tab, "Toggle").unwrap();
        assert_eq!(
            m.f.tree.accessibility_node(toggle).name(),
            Some("Activar Ecualizador")
        );
        for name in [
            "20",
            "31",
            "Cargar preset",
            "Guardar preset",
            "Restablecer ecualizador",
        ] {
            named(&m, name);
        }
        named(&m, "Preamplificación");
        let first = m.f.vm.band_label(0);
        named(&m, &format!("Banda de {first} Hz"));
        assert_eq!(
            m.f.tree
                .accessibility_node(named(&m, "Cargar preset"))
                .role(),
            Role::Button
        );
        m.f.finish();
    });
}

#[test]
fn the_english_strings_are_a_faithful_translation() {
    with_shipped_messages("en-US", || {
        let m = mounted_in("en-US");
        named(&m, "Enable Equalizer");
        named(&m, "Load preset");
        named(
            &m,
            "*You can restore the default value by right-clicking a slider.",
        );
        m.f.finish();
    });
}
