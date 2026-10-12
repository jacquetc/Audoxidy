//! Headless tests of [`MeterSlot`] and [`MeterPopup`]: the slot's geometry
//! with and without a meter, the right-click, the popup's anchoring, its
//! controls and their persistence, Escape and click-outside, and the names in
//! both languages.

use std::time::Duration;

use teksilo::canvas::Point;
use teksilo::core::accesskit::Role;
use teksilo::core::{Key, Modifiers, PointerButton, WidgetEvent, WidgetTree};
use teksilo::prelude::*;
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{FixedSize, HStack, Spacer, VStack};

use super::*;
use crate::frontend::AppContext;
use crate::frontend::fakes::InMemoryFakes;
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::keys;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::library::bottom_bar::{self, METER_SLOT_WIDTH};
use crate::teksilo_ui::library::test_util as library_util;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{
    VIEWPORT, access_click, find_by_name, first_of_type, key_down, layout, tap_at, tree_with_async,
    with_shipped_messages,
};
use crate::teksilo_ui::widgets::vu_meter;

/// Where the slot sits: the bottom bar's meter slot, at the window's foot.
const SLOT_X: f32 = 700.0;

struct Fixture {
    services: AppServices,
    fakes: InMemoryFakes,
    tree: WidgetTree,
    vm: MeterViewModel,
    slot: WidgetId,
    layer: std::rc::Rc<std::cell::Cell<Option<WidgetId>>>,
}

impl Fixture {
    fn new(enabled: bool) -> Self {
        Self::of_width(enabled, SLOT_WIDTH)
    }

    /// The slot in a box `width` px wide.
    fn of_width(enabled: bool, width: f32) -> Self {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        fakes.player.state().write().meter_enabled = enabled;
        let (mut tree, runtime) = tree_with_async(&ctx);
        let services = AppServices::new(&AppHandles::new(ctx), runtime);
        let vm = MeterViewModel::new(&services);
        let slot = MeterSlot::new(vm.clone());
        let layer = slot.layer_handle();
        let slot = tree.add(slot);
        let sized = tree.add(
            FixedSize::new()
                .width(width)
                .height(SLOT_HEIGHT)
                .child(slot),
        );
        tree.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(Spacer::new())
                .child(
                    HStack::new()
                        .spacing(0.0)
                        .alignment(VAlignment::Top)
                        .child(
                            FixedSize::new()
                                .width(SLOT_X)
                                .height(SLOT_HEIGHT)
                                .child(Spacer::new()),
                        )
                        .child(sized)
                        .child(Spacer::new()),
                ),
        );
        layout(&mut tree);
        layout(&mut tree);
        Self {
            services,
            fakes,
            tree,
            vm,
            slot,
            layer,
        }
    }

    fn rect(&self, id: WidgetId) -> (f32, f32, f32, f32) {
        let b = self.tree.bounds(id);
        (b.x, b.y, b.width, b.height)
    }

    fn slot_center(&self) -> Point {
        let (x, y, w, h) = self.rect(self.slot);
        Point::new(x + w / 2.0, y + h / 2.0)
    }

    fn meter(&self) -> Option<WidgetId> {
        first_of_type(&self.tree, self.slot, "VuMeter")
    }

    fn secondary_tap(&mut self, at: Point) {
        self.tree.dispatch_event(WidgetEvent::pointer_down(
            at,
            PointerButton::Secondary,
            Modifiers::NONE,
        ));
        self.tree.dispatch_event(WidgetEvent::pointer_up(
            at,
            PointerButton::Secondary,
            Modifiers::NONE,
        ));
        layout(&mut self.tree);
        let _ = self.tree.render();
    }

    fn open_popup(&mut self) {
        let at = self.slot_center();
        self.secondary_tap(at);
        assert_eq!(self.tree.active_overlays().len(), 1, "the popup opened");
    }

    fn popup_open(&self) -> bool {
        !self.tree.active_overlays().is_empty()
    }

    /// The settings panel's bounds (the overlay covers the window).
    fn popup_bounds(&self) -> teksilo::canvas::Rect {
        let layer = self.layer.get().expect("the popup's layer");
        let panel = first_of_type(&self.tree, layer, "MeterPopup").expect("the panel");
        self.tree.bounds(panel)
    }

    /// The control named `name` in the open popup.
    fn control(&self, name: &str) -> WidgetId {
        self.tree
            .find_by_label(name)
            .unwrap_or_else(|| panic!("no control named {name:?}"))
    }

    fn click(&mut self, name: &str) {
        let id = self.control(name);
        access_click(&mut self.tree, id);
    }

    fn key(&mut self, name: &str, key: Key) {
        let id = self.control(name);
        key_down(&mut self.tree, id, key);
    }

    fn stored(&self, key: &'static str) -> Option<String> {
        self.services
            .ctx
            .db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.services.ctx.shutdown();
    }
}

// --- the slot -----------------------------------------------------------------------------

#[test]
fn the_enabled_slot_is_400_by_40_with_the_meter_centred_in_it() {
    let f = Fixture::new(true);
    assert_eq!(
        f.rect(f.slot),
        (SLOT_X, VIEWPORT.1 - SLOT_HEIGHT, SLOT_WIDTH, SLOT_HEIGHT)
    );
    let meter = f.rect(f.meter().expect("a meter"));
    assert_eq!((meter.0, meter.2), (SLOT_X, 400.0));
    // The tall meter (48 px, with its readout) is centred on the 40 px bar.
    let slot = f.rect(f.slot);
    assert!(
        ((meter.1 + meter.3 / 2.0) - (slot.1 + slot.3 / 2.0)).abs() < 0.5,
        "{meter:?} {slot:?}"
    );
}

#[test]
fn the_disabled_slot_keeps_its_400_by_40_and_shows_no_meter() {
    // Quirk fix 10: Iced drops the slot with the meter.
    let f = Fixture::new(false);
    assert_eq!(
        f.rect(f.slot),
        (SLOT_X, VIEWPORT.1 - SLOT_HEIGHT, SLOT_WIDTH, SLOT_HEIGHT)
    );
    let meter = f.meter().expect("the widget exists, hidden");
    assert!(!f.tree.is_visible(meter), "a disabled meter is not shown");
}

#[test]
fn a_wider_slot_draws_the_meter_in_its_leading_400_and_takes_the_click_everywhere() {
    let mut f = Fixture::of_width(true, 600.0);
    assert_eq!(f.rect(f.slot).2, 600.0);
    let meter = f.rect(f.meter().unwrap());
    assert_eq!((meter.0, meter.2), (SLOT_X, 400.0));
    f.secondary_tap(Point::new(SLOT_X + 550.0, f.rect(f.slot).1 + 20.0));
    assert!(f.popup_open());
}

#[test]
fn a_narrower_slot_fills_what_it_is_given() {
    let f = Fixture::of_width(true, 300.0);
    assert_eq!(f.rect(f.slot).2, 300.0);
}

#[test]
fn the_natural_width_is_400_with_the_meter_and_0_without() {
    for (enabled, expected) in [(true, 400.0), (false, 0.0)] {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        fakes.player.state().write().meter_enabled = enabled;
        let (mut tree, runtime) = tree_with_async(&ctx);
        let services = AppServices::new(&AppHandles::new(ctx), runtime);
        let vm = MeterViewModel::new(&services);
        let id = tree.add(MeterSlot::new(vm));
        tree.layout(SizeProposal::unspecified());
        assert_eq!(tree.bounds(id).width, expected, "enabled: {enabled}");
        services.ctx.shutdown();
    }
}

#[test]
fn showing_the_meter_from_the_vm_shows_the_widget() {
    let mut f = Fixture::new(false);
    f.vm.enabled().set(true);
    layout(&mut f.tree);
    assert!(f.tree.is_visible(f.meter().unwrap()));
}

#[test]
fn the_meter_ticks_only_while_active() {
    let mut f = Fixture::new(true);
    let _ = f.tree.render();
    assert!(f.tree.frame_tick_deadline().is_none(), "idle: zero frames");
    f.services.now_playing.is_playing().set(true);
    layout(&mut f.tree);
    let _ = f.tree.render();
    assert!(f.vm.active().get());
    assert!(f.tree.frame_tick_deadline().is_some(), "playing: ticking");
    f.services.now_playing.is_playing().set(false);
    f.vm.tick(std::time::Instant::now() + Duration::from_secs(5));
    f.vm.tick(std::time::Instant::now() + Duration::from_secs(10));
    layout(&mut f.tree);
    let _ = f.tree.render();
    assert!(!f.vm.active().get());
    assert!(f.tree.frame_tick_deadline().is_none(), "idle again");
}

#[test]
fn the_slot_is_one_keyboard_stop_named_for_assistive_technology() {
    let f = Fixture::new(true);
    let node = f.tree.find_by_label("VU meter settings").expect("named");
    assert!(f.tree.tab_stops_within(f.slot).contains(&node));
    assert_eq!(f.tree.accessibility_node(node).role(), Role::Button);
}

// --- opening ------------------------------------------------------------------------------

#[test]
fn a_right_click_on_the_meter_opens_the_popup_above_the_slot() {
    // Quirk fix 7: anchored to the slot (Iced: always at the window's top-left).
    let mut f = Fixture::new(true);
    f.open_popup();
    let popup = f.popup_bounds();
    let slot = f.rect(f.slot);
    assert_eq!(popup.width, POPUP_WIDTH);
    assert!(popup.y + popup.height <= slot.1 + 0.5, "{popup:?} {slot:?}");
    assert!(
        slot.1 - (popup.y + popup.height) <= 12.0,
        "{popup:?} {slot:?}"
    );
    assert!((popup.x - slot.0).abs() <= 8.0, "{popup:?} {slot:?}");
}

#[test]
fn a_right_click_on_the_empty_slot_opens_the_popup_too() {
    // Quirk fix 10: the meter can be enabled on a fresh install.
    let mut f = Fixture::new(false);
    f.open_popup();
    let popup = f.popup_bounds();
    let slot = f.rect(f.slot);
    assert!(popup.y + popup.height <= slot.1 + 0.5);
    assert!((popup.x - slot.0).abs() <= 8.0);
}

#[test]
fn the_popup_follows_the_slot_wherever_it_is() {
    let mut f = Fixture::new(true);
    f.open_popup();
    let first = f.popup_bounds();
    let slot = f.rect(f.slot);
    // Any point of the slot opens it at the same place.
    f.tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut f.tree);
    std::thread::sleep(Duration::from_millis(150));
    f.secondary_tap(Point::new(slot.0 + 5.0, slot.1 + 5.0));
    let second = f.popup_bounds();
    assert_eq!((first.x, first.y), (second.x, second.y));
}

#[test]
fn a_right_click_on_the_slot_while_the_popup_is_open_closes_it() {
    // Iced: the popup's blocker closes it on a right press (`app.rs:5857`).
    let mut f = Fixture::new(true);
    f.open_popup();
    let at = f.slot_center();
    f.secondary_tap(at);
    assert!(!f.popup_open());
}

#[test]
fn a_left_click_does_nothing() {
    let mut f = Fixture::new(true);
    let at = f.slot_center();
    tap_at(&mut f.tree, at);
    let _ = f.tree.render();
    assert!(!f.popup_open());
}

#[test]
fn the_keyboard_opens_the_popup() {
    let mut f = Fixture::new(true);
    let node = f.tree.find_by_label("VU meter settings").unwrap();
    key_down(&mut f.tree, node, Key::Enter);
    let _ = f.tree.render();
    assert!(f.popup_open());
}

#[test]
fn escape_closes_the_popup() {
    // Quirk fix 4.
    let mut f = Fixture::new(true);
    f.open_popup();
    f.tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut f.tree);
    assert!(!f.popup_open());
}

#[test]
fn a_click_outside_closes_the_popup() {
    // Quirk fix 4.
    let mut f = Fixture::new(true);
    f.open_popup();
    tap_at(&mut f.tree, Point::new(100.0, 100.0));
    assert!(!f.popup_open());
}

// --- the popup ----------------------------------------------------------------------------

#[test]
fn the_popup_is_280_px_wide_with_iceds_rows_in_order() {
    let mut f = Fixture::new(true);
    f.open_popup();
    let popup = f.popup_bounds();
    assert_eq!(popup.width, 280.0);
    let ys: Vec<f32> = [
        "Show meter",
        "Peak hold time",
        "RMS window",
        "M/S mode",
        "Infinite hold",
        "Reset peak",
    ]
    .iter()
    .map(|name| f.tree.bounds(f.control(name)).y)
    .collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
    // Padding: 10 above, 12 on the sides.
    let show = f.tree.bounds(f.control("Show meter"));
    assert!(
        show.x >= popup.x + 12.0 && show.x < popup.x + 20.0,
        "{show:?}"
    );
}

#[test]
fn the_checkboxes_show_the_options_and_flip_them_persistently() {
    let mut f = Fixture::new(false);
    f.open_popup();
    assert!(!f.vm.enabled().get());
    f.click("Show meter");
    assert!(f.vm.enabled().get());
    assert!(f.fakes.player.snapshot().meter_enabled);
    assert_eq!(f.stored(keys::METER_ENABLED).as_deref(), Some("1"));
    f.click("M/S mode");
    assert!(f.vm.ms_mode().get());
    assert_eq!(f.stored(keys::METER_MS_MODE).as_deref(), Some("1"));
    f.click("Infinite hold");
    assert!(f.vm.infinite_hold().get());
    assert_eq!(f.stored(keys::METER_INFINITE_HOLD).as_deref(), Some("1"));
    f.click("Infinite hold");
    assert!(!f.vm.infinite_hold().get());
    assert_eq!(f.stored(keys::METER_INFINITE_HOLD).as_deref(), Some("0"));
}

#[test]
fn the_popup_stays_open_while_its_values_change() {
    // Quirk fix 10: Iced closes it after every change (`app.rs:4909-4976`).
    let mut f = Fixture::new(false);
    f.open_popup();
    f.click("Show meter");
    assert!(f.popup_open(), "showing the meter keeps it open");
    f.key("Peak hold time", Key::ArrowUp);
    assert!(f.popup_open(), "a stepper change keeps it open");
    f.key("RMS window", Key::ArrowDown);
    assert!(f.popup_open());
    f.click("M/S mode");
    f.click("Infinite hold");
    assert!(f.popup_open());
    assert!(f.tree.is_visible(f.meter().unwrap()), "the meter appeared");
}

#[test]
fn the_steppers_persist_their_values_within_their_ranges() {
    let mut f = Fixture::new(true);
    f.open_popup();
    assert_eq!(f.vm.hold_time_ms().get(), 1500.0);
    assert_eq!(f.vm.rms_window_ms().get(), 300.0);
    f.key("Peak hold time", Key::ArrowUp);
    f.key("RMS window", Key::ArrowDown);
    assert_eq!(f.vm.hold_time_ms().get(), 1550.0);
    assert_eq!(f.vm.rms_window_ms().get(), 250.0);
    assert_eq!(f.stored(keys::METER_HOLD_TIME_MS).as_deref(), Some("1550"));
    assert_eq!(f.stored(keys::METER_RMS_WINDOW_MS).as_deref(), Some("250"));
    // The Iced stubs: the meter itself keeps its fixed hold.
    assert_eq!(f.fakes.player.snapshot().meter_hold_time_ms, 1550.0);
}

#[test]
fn reset_peak_clears_the_held_peaks() {
    let mut f = Fixture::new(true);
    f.services.now_playing.path().set("/a.flac".into());
    f.services.now_playing.is_playing().set(true);
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    let t0 = std::time::Instant::now();
    for tick in 0..5 {
        f.vm.tick(t0 + Duration::from_millis(30 * tick));
    }
    assert_eq!(f.vm.frame().get().hold_db[0], -6.0);
    f.open_popup();
    // The signal went quiet: the marker would fall back to it at once.
    f.fakes.player.set_meter(-60.0, -60.0, -60.0, -60.0);
    f.click("Reset peak");
    assert_eq!(f.vm.frame().get().hold_db, [-60.0; 2]);
    assert!(f.popup_open());
}

#[test]
fn a_label_press_flips_its_option_and_its_colour_follows() {
    let mut f = Fixture::new(false);
    f.open_popup();
    let show = f.control("Show meter");
    let row = f.tree.bounds(show);
    // The label sits 5 px right of the 16 px checkbox.
    tap_at(
        &mut f.tree,
        Point::new(row.x + row.width + 5.0 + 20.0, row.y + row.height / 2.0),
    );
    assert!(f.vm.enabled().get());
    assert!(f.popup_open());
}

// --- the library's slot -------------------------------------------------------------------

#[test]
fn the_library_bar_hosts_the_slot_between_the_figures_and_the_actions() {
    // The library of a 1600 px window. The slot is the free space between the
    // figures and the actions, 10 px from each, and a shown meter gets its
    // 400 px (quirk fix 10).
    for shown in [false, true] {
        let (services, vm) = library_util::loaded();
        let meter_vm = MeterViewModel::new(&services);
        meter_vm.enabled().set(shown);
        let mut tree = library_util::tree_for(&services);
        let id = library_util::mount_at(
            &mut tree,
            crate::teksilo_ui::library::LibraryPanel::new(vm).meter_slot(MeterSlot::new(meter_vm)),
            998.0,
            library_util::REGION.1,
        );
        library_util::settle_layout(&mut tree);
        let slot = tree.bounds(library_util::all_of_type(&tree, id, "MeterSlot")[0]);
        let stats = tree.bounds(library_util::all_of_type(&tree, id, "Stats")[0]);
        let play = tree.bounds(find_by_name(&tree, id, "Play the selection")[0]);
        assert_eq!(slot.height, bottom_bar::HEIGHT, "shown: {shown}");
        assert_eq!(slot.x - (stats.x + stats.width), 10.0, "shown: {shown}");
        assert_eq!(play.x - (slot.x + slot.width), 10.0, "shown: {shown}");
        if shown {
            assert!(slot.width > METER_SLOT_WIDTH - 0.1, "{}", slot.width);
        }
        services.ctx.shutdown();
    }
}

// --- strings ------------------------------------------------------------------------------

/// The library at `width` with a shown meter: the meter's and the slot's
/// bounds, the play button's left edge, and the clip in force for each fill
/// the meter draws in its top 4 px (the part above the 40 px bar).
fn meter_in_the_bar(
    width: f32,
) -> (
    teksilo::canvas::Rect,
    teksilo::canvas::Rect,
    f32,
    Vec<Option<teksilo::canvas::Rect>>,
) {
    use teksilo::canvas::{DrawCommand, Rect};
    let (services, vm) = library_util::loaded();
    let meter_vm = MeterViewModel::new(&services);
    meter_vm.enabled().set(true);
    let mut tree = library_util::tree_for(&services);
    let id = library_util::mount_at(
        &mut tree,
        crate::teksilo_ui::library::LibraryPanel::new(vm).meter_slot(MeterSlot::new(meter_vm)),
        width,
        library_util::REGION.1,
    );
    library_util::settle_layout(&mut tree);
    let meter = tree.bounds(library_util::all_of_type(&tree, id, "VuMeter")[0]);
    let slot = tree.bounds(library_util::all_of_type(&tree, id, "MeterSlot")[0]);
    let play = tree
        .bounds(find_by_name(&tree, id, "Play the selection")[0])
        .x;
    let frame = tree.render();
    let inside = |r: [f32; 4]| {
        r[0] >= meter.x - 0.01
            && r[1] >= meter.y - 0.01
            && r[0] + r[2] <= meter.x + vu_meter::paint::WIDTH + 0.01
            && r[1] + r[3] <= meter.y + meter.height + 0.01
    };
    let mut stack: Vec<Rect> = Vec::new();
    let mut clips = Vec::new();
    for cmd in &frame.draw_order {
        let rect = match cmd {
            DrawCommand::SetClip(clip) => {
                stack.push(*clip);
                continue;
            }
            DrawCommand::ClearClip => {
                stack.pop();
                continue;
            }
            DrawCommand::Decoration(k) => frame.decorations[*k].rect,
            DrawCommand::Shape(k) => frame.shapes[*k].screen,
            _ => continue,
        };
        if inside(rect) && rect[1] < meter.y + 4.0 {
            clips.push(stack.last().copied());
        }
    }
    services.ctx.shutdown();
    (meter, slot, play, clips)
}

#[test]
fn the_library_bar_draws_the_whole_meter_over_its_edges() {
    // Iced centres the 48 px meter on the 40 px bar and draws the 4 px it
    // overflows by, above and below (`library.rs:1943`). In the library of a
    // 1600 px window the meter has its 400 px: nothing clips it.
    let (meter, slot, _, clips) = meter_in_the_bar(998.0);
    assert_eq!((meter.width, meter.height), (400.0, 48.0));
    assert_eq!(meter.y, slot.y - 4.0);
    assert!(
        !clips.is_empty(),
        "the meter draws in the 4 px above the bar"
    );
    for clip in clips.into_iter().flatten() {
        assert!(
            clip.y <= meter.y && clip.y + clip.height >= meter.y + meter.height,
            "{clip:?} cuts the meter {meter:?}"
        );
    }
}

#[test]
fn a_squeezed_meter_shows_its_left_part_and_keeps_its_height() {
    // The library of the default 1360 px window: the slot is narrower than
    // 400 px. The meter fills it, draws the left part of its layout, and its
    // clip cuts it at the slot's right edge only, before the actions.
    let (meter, slot, play, clips) = meter_in_the_bar(library_util::REGION.0);
    assert!(slot.width < 400.0, "{slot:?}");
    assert_eq!(
        (meter.x, meter.width, meter.height),
        (slot.x, slot.width, 48.0)
    );
    assert!(slot.x + slot.width <= play - 10.0 + 0.01);
    assert!(
        !clips.is_empty(),
        "the meter draws in the 4 px above the bar"
    );
    for clip in clips {
        let clip = clip.expect("a squeezed meter clips itself");
        assert_eq!(clip, meter, "the clip is the meter's own bounds");
    }
}

#[test]
fn the_popup_reads_iceds_spanish() {
    with_shipped_messages("es-ES", || {
        let mut f = Fixture::new(true);
        f.open_popup();
        for name in [
            "Mostrar medidor",
            "Tiempo de hold del pico",
            "Ventana RMS",
            "Modo M/S",
            "Hold infinito",
            "Restablecer pico",
        ] {
            f.control(name);
        }
        assert!(f.tree.find_by_label("Ajustes del medidor").is_some());
    });
}

#[test]
fn the_popup_reads_english() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new(true);
        f.open_popup();
        f.control("Show meter");
        f.control("Reset peak");
        assert_eq!(tr!(meter_popup_title()).resolve_now(), "Meter settings");
        assert_eq!(tr!(meter_hold_time()).resolve_now(), "Peak hold time:");
    });
}

#[test]
fn the_spanish_labels_are_iceds_literals() {
    with_shipped_messages("es-ES", || {
        assert_eq!(
            tr!(meter_popup_title()).resolve_now(),
            "Configuración del Meter"
        );
        assert_eq!(tr!(meter_show()).resolve_now(), "Mostrar medidor");
        assert_eq!(
            tr!(meter_hold_time()).resolve_now(),
            "Tiempo de hold del pico:"
        );
        assert_eq!(tr!(meter_rms_window()).resolve_now(), "Ventana RMS:");
        assert_eq!(tr!(meter_ms_mode()).resolve_now(), "Modo M/S");
        assert_eq!(tr!(meter_infinite_hold()).resolve_now(), "Hold infinito");
    });
}

#[test]
fn a_click_on_a_steppers_value_opens_it_for_typing_and_keeps_the_popup() {
    // Probe F6: the click closed the popup instead.
    let mut f = Fixture::new(true);
    f.open_popup();
    let stepper = f.tree.bounds(f.control("RMS window"));
    let at = Point::new(
        stepper.x + stepper.width / 2.0,
        stepper.y + stepper.height / 2.0,
    );
    tap_at(&mut f.tree, at);
    let _ = f.tree.render();
    layout(&mut f.tree);
    assert!(f.popup_open(), "the popup stays open");
    let field = f.tree.focused().and_then(|id| f.tree.widget_type_name(id));
    assert!(
        field.is_some_and(|name| name.contains("TextInputField")),
        "the value is open for typing: {field:?}"
    );
}

#[test]
fn enter_on_a_stepper_opens_its_value_and_keeps_the_popup() {
    let mut f = Fixture::new(true);
    f.open_popup();
    f.key("RMS window", Key::Enter);
    let _ = f.tree.render();
    layout(&mut f.tree);
    assert!(f.popup_open());
    let field = f.tree.focused().and_then(|id| f.tree.widget_type_name(id));
    assert!(field.is_some_and(|name| name.contains("TextInputField")));
}

#[test]
fn a_click_outside_the_popup_closes_it_and_reaches_nothing_under_it() {
    // Iced's popup sits over a blocker that takes the press
    // (`MeterPopupClose`, `app.rs:5850-5857`): the card under it is not
    // selected.
    let (services, vm) = library_util::loaded();
    let meter_vm = MeterViewModel::new(&services);
    meter_vm.enabled().set(true);
    let mut tree = library_util::tree_for(&services);
    let id = library_util::mount_at(
        &mut tree,
        crate::teksilo_ui::library::LibraryPanel::new(vm.clone())
            .meter_slot(MeterSlot::new(meter_vm)),
        998.0,
        library_util::REGION.1,
    );
    library_util::settle_layout(&mut tree);
    let slot = tree.bounds(library_util::all_of_type(&tree, id, "MeterSlot")[0]);
    let at = Point::new(slot.x + 20.0, slot.y + 20.0);
    tree.dispatch_event(WidgetEvent::pointer_down(
        at,
        PointerButton::Secondary,
        Modifiers::NONE,
    ));
    tree.dispatch_event(WidgetEvent::pointer_up(
        at,
        PointerButton::Secondary,
        Modifiers::NONE,
    ));
    layout(&mut tree);
    assert_eq!(tree.active_overlays().len(), 1, "the popup opened");
    let card = tree.bounds(library_util::all_of_type(&tree, id, "AlbumTile")[0]);
    tap_at(&mut tree, Point::new(card.x + 60.0, card.y + 60.0));
    let _ = tree.render();
    assert!(tree.active_overlays().is_empty(), "the popup closed");
    assert_eq!(vm.grid_selection().count(), 0, "the card was not clicked");
    // The next click reaches the card.
    tap_at(&mut tree, Point::new(card.x + 60.0, card.y + 60.0));
    assert_eq!(vm.grid_selection().count(), 1);
    services.ctx.shutdown();
}
