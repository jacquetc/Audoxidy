//! [`VuMeter`]: the stereo level meter of the library's bottom bar.
//!
//! The Iced `VuMeterWidget` (`src/gui/widgets.rs:4539-5178`) is a display-only
//! leaf, 400 px wide and 24 or 48 px tall, that paints what it is given; the
//! app advances `logic::meter::MeterState` every 30 ms while playing or fading
//! (`src/gui/app.rs:5941-5946`, `6056-6072`) and rebuilds it from that state
//! (`src/gui/library.rs:1887-1900`).
//!
//! Here the meter paints one [`VuMeterFrame`], a plain value the meter
//! feature's view-model produces; [`paint`] is the Iced drawing, item by item,
//! except that every label is drawn whole and a compact 24 px meter draws only
//! what fits in it, where Iced clips labels and overflows (quirk fix 34).
//! The view-model's maths are not this widget's concern: it draws whatever
//! frame it is handed.
//!
//! **Timing.** While `active` is true (playing or fading) the meter holds a
//! frame-tick subscription throttled to [`TICK`], and calls `on_tick` with
//! the current time on every frame tick; the view-model advances its state
//! from that time (the effect also runs on frames something else asked for,
//! so it must measure elapsed time, never count calls). `active` is bound at
//! rebuild level: when it turns false the subscription is dropped, nothing
//! wakes the window for the meter, and the last frame stays drawn. The
//! framework also stops the subscription while the meter is hidden.
//!
//! **Accessibility.** A meter named "VU meter" whose value reads the two bar
//! levels in whole decibels ("L -12 dB, R -11 dB"). It is not a live region,
//! so nothing is announced as the levels move; the tree is re-read only when
//! a rounded level changes.

pub mod paint;
#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::time::{Duration, Instant};

use teksilo::canvas::{Canvas, Rect, Size};
use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::core::frame_tick_scheduler::FrameTickSubscription;
use teksilo::prelude::*;
use teksilo::tokens::CornerRadius;

use self::paint::{HEIGHT, HEIGHT_TALL, MeterPaint, MeterWords, WIDTH, meter_paint};
use crate::logic::meter::METER_MIN_DB;

/// The meter's refresh interval while active (Iced ticks every 30 ms).
pub const TICK: Duration = Duration::from_millis(33);

/// What the meter draws at one moment. Index 0 of each pair is the top bar
/// (left, or mid in M/S mode), index 1 the bottom bar (right, or side).
#[derive(Debug, Clone, PartialEq)]
pub struct VuMeterFrame {
    /// The peak reading of the numeric readout ("Pk"), in dBFS.
    pub peak_db: [f32; 2],
    /// The bar level, also the readout's "Rms", in dBFS.
    pub level_db: [f32; 2],
    /// The peak-hold marker, in dBFS; at or below -60 it is not drawn.
    pub hold_db: [f32; 2],
    /// Whether the clip LED is lit.
    pub clipping: bool,
    /// Whether the bars show mid and side ("M" / "S") instead of left and
    /// right.
    pub mid_side: bool,
    /// The phase correlation, -1 to +1 (0 for silence).
    pub correlation: f32,
    /// Whether the numeric readout is shown.
    pub show_numeric: bool,
    /// The crest factor of each bar (peak minus RMS), in dB.
    pub crest_db: [f32; 2],
}

impl Default for VuMeterFrame {
    /// Silence, with the readout shown (the Iced default, `logic::meter`).
    fn default() -> Self {
        Self {
            peak_db: [METER_MIN_DB; 2],
            level_db: [METER_MIN_DB; 2],
            hold_db: [METER_MIN_DB; 2],
            clipping: false,
            mid_side: false,
            correlation: 0.0,
            show_numeric: true,
            crest_db: [0.0; 2],
        }
    }
}

impl VuMeterFrame {
    /// Whether the meter takes its tall form: with the readout, or any
    /// correlation or crest factor to show (`widgets.rs:4651-4661`).
    pub fn is_tall(&self) -> bool {
        self.show_numeric
            || self.correlation.abs() > 0.001
            || self.crest_db[0].abs() > 0.001
            || self.crest_db[1].abs() > 0.001
    }

    /// The meter's height for this frame: 48 px tall, 24 px compact.
    pub fn height(&self) -> f32 {
        if self.is_tall() { HEIGHT_TALL } else { HEIGHT }
    }

    /// The bar levels in whole decibels, `None` for a silent bar: what the
    /// accessible value reads.
    pub fn spoken_levels(&self) -> [Option<i32>; 2] {
        self.level_db
            .map(|db| (db > METER_MIN_DB).then(|| db.round() as i32))
    }
}

/// What the meter calls on every frame tick while active.
pub type TickFn = Rc<dyn Fn(Instant)>;

/// The VU meter; see the module documentation.
pub struct VuMeter {
    frame: Signal<VuMeterFrame>,
    active: Signal<bool>,
    on_tick: TickFn,
    tall: Signal<bool>,
    spoken: Signal<[Option<i32>; 2]>,
    tick: Option<FrameTickSubscription>,
}

impl std::fmt::Debug for VuMeter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VuMeter")
            .field("active", &self.active.get())
            .field("ticking", &self.tick.is_some())
            .finish_non_exhaustive()
    }
}

impl VuMeter {
    /// A meter drawing `frame`, ticking `on_tick` while `active`.
    pub fn new(
        frame: Signal<VuMeterFrame>,
        active: Signal<bool>,
        on_tick: impl Fn(Instant) + 'static,
    ) -> Self {
        let current = frame.get();
        Self {
            tall: Signal::new(current.is_tall()),
            spoken: Signal::new(current.spoken_levels()),
            frame,
            active,
            on_tick: Rc::new(on_tick),
            tick: None,
        }
    }

    /// The words drawn on the meter, in the current language.
    fn words() -> MeterWords {
        MeterWords {
            left: tr!(controls_meter_left()).resolve_now(),
            right: tr!(controls_meter_right()).resolve_now(),
            mid: tr!(controls_meter_mid()).resolve_now(),
            side: tr!(controls_meter_side()).resolve_now(),
            peak: tr!(controls_meter_peak()).resolve_now(),
            rms: tr!(controls_meter_rms()).resolve_now(),
            crest: tr!(controls_meter_crest()).resolve_now(),
        }
    }

    /// The accessible value: each bar's name and level.
    fn spoken_value(frame: &VuMeterFrame) -> String {
        let words = Self::words();
        let names = if frame.mid_side {
            [words.mid, words.side]
        } else {
            [words.left, words.right]
        };
        let levels = frame.spoken_levels().map(|level| match level {
            Some(db) => tr!(controls_meter_db(value = db.to_string())).resolve_now(),
            None => tr!(controls_meter_silent()).resolve_now(),
        });
        let [first, second] = names;
        let [first_level, second_level] = levels;
        format!("{first} {first_level}, {second} {second_level}")
    }
}

impl Widget for VuMeter {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let self_id = ctx.self_id();
        let registry = ctx.binding_registry();
        self.active
            .bind_to(self_id, registry, BindingLevel::Rebuild);
        self.frame
            .bind_to(self_id, registry, BindingLevel::RepaintOnly);
        self.tall.bind_to(self_id, registry, BindingLevel::Relayout);
        self.spoken
            .bind_to(self_id, registry, BindingLevel::AccessibilityOnly);

        let current = self.frame.get();
        self.tall.set_if_changed(current.is_tall());
        self.spoken.set_if_changed(current.spoken_levels());
        let (tall, spoken) = (self.tall.clone(), self.spoken.clone());
        ctx.effect(&self.frame, move |frame| {
            tall.set_if_changed(frame.is_tall());
            spoken.set_if_changed(frame.spoken_levels());
        });

        self.tick = None;
        if self.active.get() {
            self.tick = Some(ctx.subscribe_frame_tick_throttled(TICK));
            let on_tick = self.on_tick.clone();
            ctx.effect(&ctx.frame_tick(), move |_| on_tick(Instant::now()));
        }
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        let height = if self.tall.get() { HEIGHT_TALL } else { HEIGHT };
        Size::new(WIDTH, height).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let colors = &ctx.theme.colors;
        for item in meter_paint(&self.frame.get(), bounds, &Self::words()) {
            match item {
                MeterPaint::Fill { rect, tone } => canvas.fill_rect(rect, tone.resolve(colors)),
                MeterPaint::Led { rect, fill, border } => {
                    let radius = CornerRadius::uniform(1.0);
                    canvas.fill_rounded_rect(rect, radius, fill.resolve(colors));
                    canvas.stroke_rounded_rect(
                        Rect::new(
                            rect.x + 0.5,
                            rect.y + 0.5,
                            rect.width - 1.0,
                            rect.height - 1.0,
                        ),
                        radius,
                        border.resolve(colors),
                        1.0,
                    );
                }
                MeterPaint::Text(text) => text.paint(canvas, ctx),
            }
        }
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::Meter);
        builder.set_name(tr!(controls_meter_name()).resolve_now());
        builder.set_value(Self::spoken_value(&self.frame.get()));
    }
}
