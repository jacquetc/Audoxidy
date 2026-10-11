//! Headless tests of [`VuMeter`]: the Iced sizes, the frame-tick subscription
//! following `active`, what the meter paints, and its accessible node.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use teksilo::canvas::{DrawCommand, MockTextBackend, RenderFrame};
use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::prelude::*;
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{HStack, Spacer, VStack};

use super::paint::{db_to_x, zone};
use super::*;
use crate::teksilo_ui::style;
use crate::teksilo_ui::test_support::{a11y_node, layout, with_shipped_messages};

struct Fixture {
    tree: WidgetTree,
    meter: WidgetId,
    frame: Signal<VuMeterFrame>,
    active: Signal<bool>,
    ticks: Rc<Cell<u32>>,
}

fn playing() -> VuMeterFrame {
    VuMeterFrame {
        peak_db: [-6.0, -2.0],
        level_db: [-12.0, -2.4],
        hold_db: [-6.0, -2.0],
        correlation: 0.8,
        crest_db: [6.0, 0.4],
        ..VuMeterFrame::default()
    }
}

/// A meter at the top-left corner of the window.
fn fixture(frame: VuMeterFrame, active: bool) -> Fixture {
    // A real text backend is not available headlessly; the mock lays out
    // every character 8 px wide, so the labels are drawn but not at their
    // real widths.
    let mut tree =
        WidgetTree::new().with_text_backend(Rc::new(RefCell::new(MockTextBackend::new())));
    tree.set_theme(style::audoxidy_dark());
    let frame = Signal::new(frame);
    let active = Signal::new(active);
    let ticks = Rc::new(Cell::new(0));
    let count = ticks.clone();
    let meter = tree.add(VuMeter::new(frame.clone(), active.clone(), move |_| {
        count.set(count.get() + 1)
    }));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Top)
                    .child(meter)
                    .child(Spacer::new()),
            )
            .child(Spacer::new()),
    );
    layout(&mut tree);
    Fixture {
        tree,
        meter,
        frame,
        active,
        ticks,
    }
}

/// Renders, then lets the tree settle the frames it asked for.
fn render(tree: &mut WidgetTree) -> Rc<RenderFrame> {
    let frame = tree.render();
    layout(tree);
    frame
}

/// Runs the frame-tick effects once, as a frame the event loop pumps does.
fn frame_tick(tree: &mut WidgetTree) {
    tree.frame_tick().set(0.033);
}

fn has_fill(frame: &RenderFrame, rect: [f32; 4], color: [f32; 4]) -> bool {
    frame
        .decorations
        .iter()
        .any(|d| d.rect == rect && d.color == color)
        || frame
            .shapes
            .iter()
            .any(|s| s.screen == rect && s.color == color)
}

#[test]
fn the_meter_is_four_hundred_by_forty_eight_with_the_readout() {
    let f = fixture(VuMeterFrame::default(), false);
    let bounds = f.tree.bounds(f.meter);
    assert_eq!((bounds.width, bounds.height), (400.0, 48.0));
}

#[test]
fn the_meter_is_twenty_four_high_with_nothing_but_the_bars_to_show() {
    let compact = VuMeterFrame {
        show_numeric: false,
        ..VuMeterFrame::default()
    };
    assert!(!compact.is_tall());
    let mut f = fixture(compact, false);
    assert_eq!(f.tree.bounds(f.meter).height, 24.0);
    f.frame.set(VuMeterFrame {
        show_numeric: false,
        correlation: 0.3,
        ..VuMeterFrame::default()
    });
    layout(&mut f.tree);
    assert_eq!(f.tree.bounds(f.meter).height, 48.0);
}

#[test]
fn an_active_meter_ticks_on_every_frame_with_the_time() {
    let mut f = fixture(playing(), true);
    let before = f.ticks.get();
    frame_tick(&mut f.tree);
    frame_tick(&mut f.tree);
    assert_eq!(f.ticks.get(), before + 2);
}

#[test]
fn an_idle_meter_never_ticks() {
    let mut f = fixture(playing(), false);
    frame_tick(&mut f.tree);
    frame_tick(&mut f.tree);
    assert_eq!(f.ticks.get(), 0);
}

#[test]
fn the_subscription_lives_only_while_the_meter_is_active() {
    let mut f = fixture(playing(), true);
    let _ = f.tree.render();
    assert!(
        f.tree.frame_requested(),
        "an active meter keeps the window waking"
    );
    assert!(f.tree.frame_tick_deadline().is_some());

    f.active.set(false);
    layout(&mut f.tree);
    render(&mut f.tree);
    let _ = f.tree.render();
    assert!(!f.tree.frame_requested(), "an idle meter asks for no frame");
    assert_eq!(f.tree.frame_tick_deadline(), None);
    let before = f.ticks.get();
    frame_tick(&mut f.tree);
    assert_eq!(f.ticks.get(), before, "an idle meter has no tick effect");

    f.active.set(true);
    layout(&mut f.tree);
    let _ = f.tree.render();
    assert!(f.tree.frame_requested());
    let before = f.ticks.get();
    frame_tick(&mut f.tree);
    assert_eq!(f.ticks.get(), before + 1);
}

#[test]
fn a_stopped_meter_keeps_drawing_its_last_frame() {
    let mut f = fixture(playing(), true);
    f.active.set(false);
    layout(&mut f.tree);
    let frame = render(&mut f.tree);
    let left = db_to_x(-12.0, 21.0, 264.0) - 21.0;
    assert!(has_fill(
        &frame,
        [21.0, 1.0, left, 6.0],
        zone::GREEN.to_array()
    ));
}

#[test]
fn the_meter_paints_its_bars_holds_and_led() {
    let mut f = fixture(playing(), false);
    let frame = render(&mut f.tree);
    assert!(has_fill(
        &frame,
        [0.0, 0.0, 400.0, 48.0],
        style::BACKGROUND.to_array()
    ));
    let right = db_to_x(-2.4, 21.0, 264.0) - 21.0;
    assert!(has_fill(
        &frame,
        [21.0, 9.0, right, 6.0],
        zone::ORANGE.to_array()
    ));
    let hold = db_to_x(-6.0, 21.0, 264.0);
    assert!(has_fill(
        &frame,
        [hold, 1.0, 2.0, 6.0],
        zone::YELLOW.to_array()
    ));
    assert!(has_fill(
        &frame,
        [21.0, 17.0, 264.0, 3.0],
        style::CONTRAST.to_array()
    ));
}

#[test]
fn a_new_frame_repaints_without_a_rebuild() {
    let mut f = fixture(playing(), false);
    render(&mut f.tree);
    f.frame.set(VuMeterFrame {
        level_db: [1.0, -60.0],
        ..playing()
    });
    layout(&mut f.tree);
    let frame = render(&mut f.tree);
    let left = db_to_x(1.0, 21.0, 264.0) - 21.0;
    assert!(has_fill(
        &frame,
        [21.0, 1.0, left, 6.0],
        zone::RED.to_array()
    ));
}

#[test]
fn the_meter_reads_its_levels_in_whole_decibels() {
    with_shipped_messages("en-US", || {
        let mut f = fixture(playing(), false);
        let meter = a11y_node(&mut f.tree, f.meter);
        assert_eq!(meter.role(), Role::Meter);
        assert_eq!(meter.label(), Some("VU meter"));
        assert_eq!(meter.value(), Some("L -12 dB, R -2 dB"));
        assert!(meter.live().is_none());
    });
}

#[test]
fn the_spanish_meter_names_mid_side_and_silence() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture(
            VuMeterFrame {
                mid_side: true,
                level_db: [-3.4, -60.0],
                ..playing()
            },
            false,
        );
        let meter = a11y_node(&mut f.tree, f.meter);
        assert_eq!(meter.label(), Some("Medidor VU"));
        assert_eq!(meter.value(), Some("M -3 dB, S silencio"));
    });
}

#[test]
fn the_spoken_levels_change_only_with_a_whole_decibel() {
    let quiet = VuMeterFrame::default();
    assert_eq!(quiet.spoken_levels(), [None, None]);
    let a = VuMeterFrame {
        level_db: [-12.2, -3.0],
        ..quiet.clone()
    };
    let b = VuMeterFrame {
        level_db: [-11.8, -3.4],
        ..quiet
    };
    assert_eq!(a.spoken_levels(), b.spoken_levels());
    assert_eq!(a.spoken_levels(), [Some(-12), Some(-3)]);
}

/// Every fill and shape the frame paints, as `[x, y, width, height]`.
fn painted_rects(frame: &RenderFrame) -> Vec<[f32; 4]> {
    frame
        .decorations
        .iter()
        .map(|d| d.rect)
        .chain(frame.shapes.iter().map(|s| s.screen))
        .collect()
}

fn within(rect: [f32; 4], width: f32, height: f32) -> bool {
    let [x, y, w, h] = rect;
    x >= 0.0 && y >= 0.0 && x + w <= width && y + h <= height
}

/// The centre of a glyph quad.
fn centre(glyph: [f32; 4]) -> (f32, f32) {
    (glyph[0] + glyph[2] / 2.0, glyph[1] + glyph[3] / 2.0)
}

fn clips(frame: &RenderFrame) -> bool {
    frame
        .draw_order
        .iter()
        .any(|cmd| matches!(cmd, DrawCommand::SetClip(_) | DrawCommand::ClearClip))
}

/// The characters of every label the meter draws for `frame`, `height` px
/// tall.
fn label_chars(frame: &VuMeterFrame, height: f32) -> usize {
    paint::meter_paint(
        frame,
        teksilo::canvas::Rect::new(0.0, 0.0, 400.0, height),
        &paint::MeterWords::english(),
    )
    .iter()
    .map(|item| match item {
        paint::MeterPaint::Text(text) => text.content.chars().count(),
        _ => 0,
    })
    .sum()
}

#[test]
fn every_label_of_the_tall_meter_is_drawn_whole_and_unclipped() {
    let frame = VuMeterFrame {
        mid_side: true,
        correlation: -0.6,
        ..playing()
    };
    with_shipped_messages("en-US", || {
        let mut f = fixture(frame.clone(), false);
        let rendered = render(&mut f.tree);
        assert!(!clips(&rendered), "no label is clipped");
        assert_eq!(
            rendered.glyphs.len(),
            label_chars(&frame, 48.0),
            "every character is drawn"
        );
        // The mock lays each character out 8 × 16, so a label's first glyph
        // shows where it is anchored: "M" centred in its box beside the top
        // bar, the readout from x = 289 on the middle of that bar's row.
        assert_eq!(centre(rendered.glyphs[0].screen), (16.0, 4.0));
        assert_eq!(centre(rendered.glyphs[1].screen), (16.0, 12.0));
        assert_eq!(rendered.glyphs[2].screen[0], 289.0);
        assert_eq!(centre(rendered.glyphs[2].screen).1, 4.0);
        for rect in painted_rects(&rendered) {
            assert!(within(rect, 400.0, 48.0), "{rect:?} leaves the meter");
        }
    });
}

#[test]
fn the_compact_meter_draws_nothing_below_its_twenty_four_pixels() {
    let compact = VuMeterFrame {
        show_numeric: false,
        correlation: 0.0,
        crest_db: [0.0, 0.0],
        ..playing()
    };
    with_shipped_messages("en-US", || {
        let mut f = fixture(compact.clone(), false);
        assert_eq!(f.tree.bounds(f.meter).height, 24.0);
        let rendered = render(&mut f.tree);
        assert!(!clips(&rendered));
        // "L", "R" and "0.00": no crest row, no scale.
        assert_eq!(rendered.glyphs.len(), 6);
        assert_eq!(label_chars(&compact, 24.0), 6);
        for glyph in &rendered.glyphs {
            let (_, y) = centre(glyph.screen);
            assert!(
                (0.0..=24.0).contains(&y),
                "{:?} is centred outside",
                glyph.screen
            );
        }
        for rect in painted_rects(&rendered) {
            assert!(
                within(rect, 400.0, 24.0),
                "{rect:?} leaves the compact meter"
            );
        }
    });
}
