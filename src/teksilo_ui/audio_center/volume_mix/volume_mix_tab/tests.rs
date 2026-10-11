use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::core::{Key, PointerButton};
use teksilo::tokens::Alignment;
use teksilo::widgets::ZStack;

use super::*;
use crate::frontend::commands::mix;
use crate::frontend::settings;
use crate::teksilo_ui::app::services::{AppServices, audio_center_tab};
use crate::teksilo_ui::audio_center::shell::testing::{
    all_of_role, find_by_name, services_with_fakes, tree_for,
};
use crate::teksilo_ui::audio_center::shell::{AudioCenterPanel, AudioCenterViewModel};
use crate::teksilo_ui::test_support::{
    a11y_node, access_click, key_down, key_down_with, layout, pointer_click, tap_at, type_text,
    with_shipped_messages,
};

/// The body the panel gives a tab (see the configuration tab's tests).
const BODY: (f32, f32) = (906.0, 391.0);

struct Fixture {
    services: AppServices,
    vm: VolumeMixViewModel,
    tree: WidgetTree,
    tab: WidgetId,
}

fn fixture() -> Fixture {
    let (services, _fakes) = services_with_fakes();
    let vm = VolumeMixViewModel::new(&services);
    let (mut tree, _toasts) = tree_for(&services);
    let tab = tree.add(VolumeMixTab::new(vm.clone()));
    tree.add(
        ZStack::new().alignment(Alignment::TOP_LEADING).child(
            FixedSize::new()
                .width(BODY.0)
                .height(BODY.1)
                .child(ZStack::new().alignment(Alignment::TOP_LEADING).child(tab)),
        ),
    );
    layout(&mut tree);
    Fixture {
        services,
        vm,
        tree,
        tab,
    }
}

impl Fixture {
    fn find(&mut self, name: &str) -> Vec<WidgetId> {
        find_by_name(&mut self.tree, self.tab, name)
    }

    fn named(&mut self, name: &str) -> WidgetId {
        let found = self.find(name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    }

    fn bounds(&mut self, name: &str) -> Rect {
        let id = self.named(name);
        self.tree.bounds(id)
    }

    fn of_role(&mut self, role: Role) -> Vec<WidgetId> {
        all_of_role(&mut self.tree, self.tab, role)
    }

    fn disabled(&mut self, id: WidgetId) -> bool {
        a11y_node(&mut self.tree, id).is_disabled()
    }
}

fn centre(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

// ---- layout ----

#[test]
fn there_are_fourteen_check_boxes_and_ten_steppers() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        assert_eq!(f.of_role(Role::CheckBox).len(), 14);
        assert_eq!(f.of_role(Role::SpinButton).len(), 10);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_check_boxes_are_13_pixels_and_the_steppers_96_by_13() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        for id in f.of_role(Role::CheckBox) {
            let b = f.tree.bounds(id);
            assert_eq!((b.width, b.height), (CHECKBOX_SIZE, CHECKBOX_SIZE));
        }
        for id in f.of_role(Role::SpinButton) {
            let b = f.tree.bounds(id);
            assert_eq!((b.width, b.height), (96.0, 13.0));
        }
        assert!(f.services.ctx.shutdown());
    });
}

/// The bounds of every text widget of the tab, top to bottom then left to
/// right. The labels are hidden from the accessibility tree (the check boxes
/// carry the names), so geometry finds them by type.
fn texts(f: &Fixture) -> Vec<Rect> {
    let mut found: Vec<Rect> = descendants(&f.tree, f.tab)
        .into_iter()
        .filter(|id| {
            f.tree
                .widget_type_name(*id)
                .is_some_and(|name| name.ends_with("TextWidget"))
        })
        .map(|id| f.tree.bounds(id))
        .filter(|bounds| bounds.width > 0.0)
        .collect();
    found.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    found
}

#[test]
fn the_five_groups_have_their_titles() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        for title in [
            "Volume Fading",
            "Remove silence",
            "Pre-Amplifier",
            "Replay Gain",
            "Crossfade",
        ] {
            // The check box carries the name; the title text is hidden.
            assert_eq!(f.find(title).len(), 1, "{title}");
        }
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn rows_are_5_pixels_apart_and_groups_18() {
    with_shipped_messages("en-US", || {
        let f = fixture();
        // The left column's labels, in order (the right column's are at the
        // same heights, further right).
        let left: Vec<Rect> = texts(&f).into_iter().filter(|t| t.x < 300.0).collect();
        // Fades: title, smooth, fade in, fade out.
        let (title, smooth, fade_in, fade_out) = (left[0], left[1], left[2], left[3]);
        assert!(
            (title.y - TOP).abs() < 0.5,
            "20 px under the tab's top: {title:?}"
        );
        assert!((smooth.y - (title.y + title.height + ROW_SPACING)).abs() < 0.5);
        assert!((fade_in.y - (smooth.y + smooth.height + ROW_SPACING)).abs() < 0.5);
        assert!((fade_out.y - (fade_in.y + fade_in.height + ROW_SPACING)).abs() < 0.5);
        // Silence removal starts 18 px after the last row of the fades.
        let silence = left[4];
        assert!(
            (silence.y - (fade_out.y + fade_out.height + GROUP_SPACING)).abs() < 0.5,
            "{silence:?} after {fade_out:?}"
        );
        // Everything of the left column fits the tab.
        let last = left[left.len() - 1];
        assert!(
            last.y + last.height <= BODY.1,
            "the four groups fit the tab: {last:?} in {BODY:?}"
        );
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_row_is_a_check_box_5_pixels_a_label_and_the_stepper_at_the_trailing_edge() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        let boxes = f.of_role(Role::CheckBox);
        let fade_in_box = f.tree.bounds(boxes[2]);
        let label = texts(&f)
            .into_iter()
            .find(|t| {
                t.x > fade_in_box.x
                    && (t.y + t.height / 2.0 - (fade_in_box.y + fade_in_box.height / 2.0)).abs()
                        < 1.0
            })
            .expect("the fade-in label is on the check box's row");
        // The label starts 5 px after the 13 px box (the stock box has a
        // 24 px target around its drawing; it is held to 13 px here).
        assert_eq!(label.x, fade_in_box.x + CHECKBOX_SIZE + CHECK_SPACING);
        let stepper = f.bounds("Fade-in length");
        let left_width = (BODY.0 - DIVIDER_CELL) * 5.0 / 9.0 - LEFT_TRAILING;
        assert!(
            (stepper.x + stepper.width - left_width).abs() < 0.5,
            "at the trailing edge of the left column: {stepper:?}"
        );
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_divider_is_a_2_by_220_line_between_the_columns_lifted_24_pixels() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        let line = f
            .tree
            .children(f.tab)
            .into_iter()
            .flat_map(|id| descendants(&f.tree, id))
            .map(|id| f.tree.bounds(id))
            .find(|b| b.width == LINE_WIDTH && b.height == LINE_HEIGHT)
            .expect("the divider");
        let crossfade = f.bounds("Crossfade");
        assert!(line.x + line.width <= crossfade.x, "{line:?} {crossfade:?}");
        // Centred in the columns' height, 24 px higher.
        let columns = BODY.1 - TOP;
        let expected = TOP + (columns - LINE_HEIGHT) / 2.0 - 24.0;
        assert!((line.y - expected).abs() < 0.5, "{} vs {expected}", line.y);
        let left = (BODY.0 - DIVIDER_CELL) * 5.0 / 9.0;
        assert!(
            (line.x - (left + (DIVIDER_CELL - LINE_WIDTH) / 2.0 - DIVIDER_SHIFT / 2.0)).abs() < 0.5,
            "{line:?}"
        );
        assert!(f.services.ctx.shutdown());
    });
}

fn descendants(tree: &WidgetTree, root: WidgetId) -> Vec<WidgetId> {
    let mut found = vec![root];
    for child in tree.children(root) {
        found.extend(descendants(tree, child));
    }
    found
}

// ---- the locks ----

#[test]
fn what_is_in_a_group_is_disabled_while_its_master_is_off() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        // Volume fading starts off in the engine, silence removal on.
        let smooth_box = f.of_role(Role::CheckBox)[1];
        assert!(f.disabled(smooth_box));
        let fade_in = f.named("Fade-in length");
        assert!(f.disabled(fade_in));
        let silence_length = f.named("Shortest silence removed");
        assert!(!f.disabled(silence_length), "silence removal starts on");
        let master = f.of_role(Role::CheckBox)[0];
        assert!(!f.disabled(master), "a master is never locked");
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_master_check_box_unlocks_its_group_and_stores_the_change() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        let master = f.of_role(Role::CheckBox)[0];
        access_click(&mut f.tree, master);
        assert!(f.vm.is_on(MixToggle::Fades));
        let smooth_box = f.of_role(Role::CheckBox)[1];
        assert!(!f.disabled(smooth_box));
        let stored = settings::load_all(&f.services.ctx.db)
            .recv_blocking()
            .unwrap();
        assert!(stored.mix.fades_enabled);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_switch_check_box_toggles_and_stores() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.vm.set_toggle(MixToggle::Fades, true);
        layout(&mut f.tree);
        let smooth_box = f.of_role(Role::CheckBox)[1];
        access_click(&mut f.tree, smooth_box);
        assert!(f.vm.is_on(MixToggle::SmoothVolume));
        let stored = settings::load_all(&f.services.ctx.db)
            .recv_blocking()
            .unwrap();
        assert!(stored.mix.smooth_volume_enabled);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_click_on_a_label_flips_its_switch() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.vm.set_toggle(MixToggle::Fades, true);
        layout(&mut f.tree);
        let label = f.bounds("Smooth volume changes");
        tap_at(&mut f.tree, centre(label));
        assert!(f.vm.is_on(MixToggle::SmoothVolume));
        tap_at(&mut f.tree, centre(label));
        assert!(!f.vm.is_on(MixToggle::SmoothVolume));
        // A group title flips its master.
        let title = f.bounds("Volume Fading");
        tap_at(&mut f.tree, centre(title));
        assert!(!f.vm.is_on(MixToggle::Fades));
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_click_on_a_locked_label_does_nothing() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        assert!(!f.vm.is_on(MixToggle::Fades));
        let label = f.bounds("Smooth volume changes");
        tap_at(&mut f.tree, centre(label));
        assert!(!f.vm.is_on(MixToggle::SmoothVolume));
        let stored = settings::load_all(&f.services.ctx.db)
            .recv_blocking()
            .unwrap();
        assert!(!stored.mix.smooth_volume_enabled);
        assert!(f.services.ctx.shutdown());
    });
}

// ---- the steppers ----

fn enable_fade_in(f: &mut Fixture) {
    f.vm.set_toggle(MixToggle::Fades, true);
    f.vm.set_toggle(MixToggle::FadeIn, true);
    layout(&mut f.tree);
}

#[test]
fn a_stepper_changes_its_value_and_stores_it() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        enable_fade_in(&mut f);
        let before = f.vm.value(MixValue::FadeInMs).get();
        let stepper = f.named("Fade-in length");
        key_down(&mut f.tree, stepper, Key::ArrowUp);
        assert_eq!(f.vm.value(MixValue::FadeInMs).get(), before + 50.0);
        let stored = settings::load_all(&f.services.ctx.db)
            .recv_blocking()
            .unwrap();
        assert_eq!(f64::from(stored.mix.fade_in_ms), before + 50.0);
        assert_eq!(
            MixValue::FadeInMs.get(&f.services.ctx.player.snapshot()),
            before + 50.0,
            "the engine has it too"
        );
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_secondary_click_on_a_stepper_resets_it() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        enable_fade_in(&mut f);
        f.vm.value_changed(MixValue::FadeInMs, 4000.0);
        layout(&mut f.tree);
        let stepper = f.bounds("Fade-in length");
        pointer_click(&mut f.tree, centre(stepper), PointerButton::Secondary, 0);
        assert_eq!(f.vm.value(MixValue::FadeInMs).get(), 1000.0);
        let stored = settings::load_all(&f.services.ctx.db)
            .recv_blocking()
            .unwrap();
        assert_eq!(stored.mix.fade_in_ms, 1000.0);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_disabled_stepper_ignores_the_keyboard() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        let before = f.vm.value(MixValue::FadeInMs).get();
        let stepper = f.named("Fade-in length");
        key_down(&mut f.tree, stepper, Key::ArrowUp);
        assert_eq!(f.vm.value(MixValue::FadeInMs).get(), before);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_steppers_have_iceds_ranges_and_units() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.vm.set_toggle(MixToggle::RgFixed, true);
        layout(&mut f.tree);
        let node = |f: &mut Fixture, name: &str| {
            let id = f.named(name);
            a11y_node(&mut f.tree, id)
        };
        let threshold = node(&mut f, "Silence threshold");
        assert_eq!(threshold.min_numeric_value(), Some(-80.0));
        assert_eq!(threshold.max_numeric_value(), Some(0.0));
        assert_eq!(threshold.numeric_value_step(), Some(0.25));
        let preamp = node(&mut f, "Pre-Amplifier gain");
        assert_eq!(preamp.min_numeric_value(), Some(-30.0));
        assert_eq!(preamp.max_numeric_value(), Some(12.0));
        let fade = node(&mut f, "Fade-in length");
        assert_eq!(fade.min_numeric_value(), Some(0.0));
        assert_eq!(fade.max_numeric_value(), Some(10_000.0));
        assert_eq!(fade.numeric_value_step(), Some(50.0));
        let shortest = node(&mut f, "Shortest silence removed");
        assert_eq!(shortest.min_numeric_value(), Some(100.0));
        let album = node(&mut f, "Album Replay Gain offset");
        assert_eq!(album.min_numeric_value(), Some(-12.0));
        assert_eq!(album.max_numeric_value(), Some(12.0));
        assert!(f.services.ctx.shutdown());
    });
}

// ---- Spanish ----

#[test]
fn the_spanish_strings_are_the_iced_ones_with_the_typos_corrected() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture();
        for name in [
            "Desvanecimiento de Volumen",
            "Suavizar el cambio de volumen",
            "Desvanecimiento de volumen al iniciar la canción",
            "Desvanecimiento de volumen al terminar la canción",
            "Eliminar silencio",
            "Eliminar los silencios mayores a:",
            "Umbral de eliminación de silencios menores a:",
            "Eliminar silencios de inicio y fin de las canciones",
            "Pre-Amplificador",
            "Nivel del Pre-Amplificador",
            "Replay Gain",
            "Usar valor de la etiqueta incrustada 'Album'",
            "Usar valor de la etiqueta incrustada 'Canción'",
            "Replay Gain fijo para canciones sin etiquetas",
            "Mezcla Cruzada",
            "Cambio manual:",
            "Cambio automático:",
        ] {
            assert!(!f.find(name).is_empty(), "{name}");
        }
        assert_eq!(f.find("Mezclar canción actual con la siguiente").len(), 2);
        assert!(f.services.ctx.shutdown());
    });
}

// ---- inside the panel ----

struct PanelFixture {
    services: AppServices,
    vm: VolumeMixViewModel,
    shell: AudioCenterViewModel,
    tree: WidgetTree,
}

fn in_panel() -> PanelFixture {
    let (services, _fakes) = services_with_fakes();
    let vm = VolumeMixViewModel::new(&services);
    let shell = AudioCenterViewModel::new(&services);
    let (mut tree, _toasts) = tree_for(&services);
    let spare = || -> Box<dyn Widget> { Box::new(Spacer::new()) };
    let panel = AudioCenterPanel::new(
        shell.clone(),
        spare(),
        spare(),
        spare(),
        Box::new(VolumeMixTab::new(vm.clone())),
    );
    tree.add(ZStack::new().alignment(Alignment::TOP_LEADING).child(panel));
    layout(&mut tree);
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::VOLUME_MIX));
    layout(&mut tree);
    PanelFixture {
        services,
        vm,
        shell,
        tree,
    }
}

impl PanelFixture {
    fn stepper(&mut self, name: &str) -> WidgetId {
        let root = self.tree.find_by_role(Role::Dialog).expect("the panel");
        let found = find_by_name(&mut self.tree, root, name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    }
}

#[test]
fn a_press_on_the_background_commits_an_open_value_field() {
    // Quirk 25 workaround: Iced commits an open value field (and deselects a
    // slider) on any outside click; Teksilo keeps the focus on a click in
    // empty space, so the panel's frame takes it.
    with_shipped_messages("en-US", || {
        let mut f = in_panel();
        f.vm.set_toggle(MixToggle::Fades, true);
        f.vm.set_toggle(MixToggle::FadeIn, true);
        layout(&mut f.tree);
        let stepper = f.stepper("Fade-in length");
        let bounds = f.tree.bounds(stepper);
        // Open the field with a press on the value, and type a number.
        pointer_click(&mut f.tree, centre(bounds), PointerButton::Primary, 0);
        let field = f.tree.focused().expect("the field has the focus");
        assert_ne!(field, stepper, "the field is open");
        key_down_with(
            &mut f.tree,
            field,
            Key::A,
            teksilo::core::Modifiers::COMMAND,
        );
        key_down(&mut f.tree, field, Key::Delete);
        type_text(&mut f.tree, "1500");
        assert_ne!(
            f.vm.value(MixValue::FadeInMs).get(),
            1500.0,
            "not yet committed"
        );

        // A press on the panel's empty space.
        let frame = f.tree.find_by_role(Role::Dialog).expect("the panel");
        let frame = f.tree.bounds(frame);
        pointer_click(
            &mut f.tree,
            Point::new(frame.x + 700.0, frame.y + 440.0),
            PointerButton::Primary,
            400,
        );

        assert_eq!(f.vm.value(MixValue::FadeInMs).get(), 1500.0);
        let stored = settings::load_all(&f.services.ctx.db)
            .recv_blocking()
            .unwrap();
        assert_eq!(stored.mix.fade_in_ms, 1500.0, "committed and stored");
        assert!(f.shell.open().get(), "the press does not close the panel");
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn escape_cancels_an_open_field_before_it_closes_the_panel() {
    with_shipped_messages("en-US", || {
        let mut f = in_panel();
        f.vm.set_toggle(MixToggle::Fades, true);
        f.vm.set_toggle(MixToggle::FadeIn, true);
        layout(&mut f.tree);
        let before = f.vm.value(MixValue::FadeInMs).get();
        let stepper = f.stepper("Fade-in length");
        let bounds = f.tree.bounds(stepper);
        pointer_click(&mut f.tree, centre(bounds), PointerButton::Primary, 0);
        let field = f.tree.focused().expect("the field has the focus");
        type_text(&mut f.tree, "99");
        key_down(&mut f.tree, field, Key::Escape);
        assert!(f.shell.open().get(), "Escape went to the field");
        assert_eq!(f.vm.value(MixValue::FadeInMs).get(), before);
        // With nothing to cancel, the next Escape closes the panel.
        let focused = f.tree.focused().expect("the stepper has the focus again");
        key_down(&mut f.tree, focused, Key::Escape);
        assert!(!f.shell.open().get());
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_values_are_read_again_when_the_tab_is_shown() {
    with_shipped_messages("en-US", || {
        let mut f = in_panel();
        f.shell.select_tab(audio_center_tab::CONFIG);
        layout(&mut f.tree);
        mix::set_value(&f.services.ctx, MixValue::CrossfadeAutoMs, 777.0)
            .recv_blocking()
            .unwrap();
        f.shell.select_tab(audio_center_tab::VOLUME_MIX);
        layout(&mut f.tree);
        assert_eq!(f.vm.value(MixValue::CrossfadeAutoMs).get(), 777.0);
        assert!(f.services.ctx.shutdown());
    });
}
