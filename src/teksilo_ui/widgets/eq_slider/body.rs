//! The chrome of the Iced `CustomSlider` (`src/gui/widgets.rs:3139-4531`) as a
//! Teksilo [`SliderStyle`], shared by [`EqSlider`](super::EqSlider) and
//! [`EffectSlider`](crate::teksilo_ui::widgets::EffectSlider).
//!
//! The stock `Slider` keeps the input (drag, keys, accessibility); this style
//! paints what Iced's `draw_vertical_slider` / `draw_horizontal_slider`
//! (`widgets.rs:4100-4397`) paint:
//!
//! - an 8 px track across the whole length, with round ends (radius 4);
//! - optionally the "coloured track" from the minimum to the value;
//! - a 16 px square handle with radius 2, centred on the value and clamped
//!   inside the bounds, in an idle, a hover and a selected colour, each with
//!   an optional inset border.
//!
//! The style's `thumb_diameter` is 0: Iced maps a position to a value
//! over the whole length (`widgets.rs:3807-3814`) and lets the handle overhang
//! the end positions, and the stock slider maps over the length minus the
//! thumb diameter. With 0 the two mappings agree.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::canvas::{Canvas, Point, Rect, Size};
use teksilo::core::binding::BindingLevel;
use teksilo::core::environment::LayoutDirection;
use teksilo::core::styles::{SliderOrientation, SliderStyle, SliderStyleConfig};
use teksilo::prelude::*;
use teksilo::tokens::CornerRadius;

use super::chrome::Tone;

/// The track thickness (`widgets.rs:3302`).
pub const TRACK_WIDTH: f32 = 8.0;
/// The handle's side (`widgets.rs:3303`).
pub const HANDLE_SIZE: f32 = 16.0;
/// The handle's corner radius (`widgets.rs:3235`).
pub const HANDLE_RADIUS: f32 = 2.0;

/// The handle in one state: its colour and its inset border.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandleLook {
    /// The handle colour.
    pub tone: Tone,
    /// The border colour.
    pub border: Tone,
    /// The border width; 0 draws no border.
    pub border_width: f32,
}

impl HandleLook {
    /// A handle of `tone` with no border.
    pub const fn plain(tone: Tone) -> Self {
        Self {
            tone,
            border: Tone::Transparent,
            border_width: 0.0,
        }
    }
}

/// Every colour of a slider: the Iced `CustomSliderOptions` a call site sets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderLook {
    /// The track (`track_color`, default `COLOR_CONTRAST`).
    pub track: Tone,
    /// The coloured track, when `enable_colored_track` is set
    /// (`active_track_color`, default `COLOR_ACCENT`).
    pub fill: Option<Tone>,
    /// The handle at rest (`handle_color`, default `COLOR_ACCENT`).
    pub idle: HandleLook,
    /// The handle under the pointer (`handle_hover_color`, default
    /// `COLOR_TEXT_PRIMARY`).
    pub hover: HandleLook,
    /// The handle of the selected slider (`handle_selected_color`, default
    /// `COLOR_TEXT_PRIMARY`).
    pub selected: HandleLook,
}

impl SliderLook {
    /// An equalizer band while the equalizer is on: the Iced defaults
    /// (`audio_center.rs:2892`, `widgets.rs:3215-3252`).
    pub const EQ_ON: SliderLook = SliderLook {
        track: Tone::Contrast,
        fill: None,
        idle: HandleLook::plain(Tone::Accent),
        hover: HandleLook::plain(Tone::TextPrimary),
        selected: HandleLook::plain(Tone::TextPrimary),
    };

    /// An equalizer band while the equalizer is off (`audio_center.rs:2902-2908`):
    /// a dark handle, red under the pointer, outlined when selected.
    pub const EQ_OFF: SliderLook = SliderLook {
        track: Tone::Contrast,
        fill: None,
        idle: HandleLook {
            tone: Tone::Contrast,
            border: Tone::Accent,
            border_width: 0.0,
        },
        hover: HandleLook::plain(Tone::Accent),
        selected: HandleLook {
            tone: Tone::Contrast,
            border: Tone::TextSecondary,
            border_width: 1.0,
        },
    };

    /// An effect parameter (`audio_center.rs:3130-3134`): a dark track filled
    /// in the accent colour up to the value.
    pub const EFFECT: SliderLook = SliderLook {
        track: Tone::Background,
        fill: Some(Tone::Accent),
        idle: HandleLook::plain(Tone::Accent),
        hover: HandleLook::plain(Tone::TextPrimary),
        selected: HandleLook::plain(Tone::TextPrimary),
    };

    /// The handle for a state: selected wins over hovered (`widgets.rs:4168-4195`).
    pub fn handle(&self, hovered: bool, selected: bool) -> HandleLook {
        if selected {
            self.selected
        } else if hovered {
            self.hover
        } else {
            self.idle
        }
    }
}

/// Where the parts of a slider go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderGeometry {
    /// The whole track.
    pub track: Rect,
    /// The coloured part of the track (empty at the minimum).
    pub fill: Rect,
    /// The handle.
    pub handle: Rect,
}

/// The geometry of a slider in `bounds` at `percent` (0 at the minimum, 1 at
/// the maximum) (`widgets.rs:4114-4202`, `4267-4348`). A horizontal slider in
/// a right-to-left layout has its minimum on the right.
pub fn geometry(
    bounds: Rect,
    orientation: SliderOrientation,
    percent: f32,
    rtl: bool,
) -> SliderGeometry {
    let percent = percent.clamp(0.0, 1.0);
    match orientation {
        SliderOrientation::Vertical => {
            let track_x = bounds.x + (bounds.width - TRACK_WIDTH) / 2.0;
            let filled = bounds.height * percent;
            let handle_y = bounds.y + bounds.height - percent * bounds.height - HANDLE_SIZE / 2.0;
            // Not `f32::clamp`: it panics when the slider is shorter than the
            // handle (the range is then empty). The handle sits at the start.
            let handle_y = handle_y
                .min(bounds.y + bounds.height - HANDLE_SIZE)
                .max(bounds.y);
            SliderGeometry {
                track: Rect::new(track_x, bounds.y, TRACK_WIDTH, bounds.height),
                fill: Rect::new(
                    track_x,
                    bounds.y + bounds.height - filled,
                    TRACK_WIDTH,
                    filled,
                ),
                handle: Rect::new(
                    bounds.x + (bounds.width - HANDLE_SIZE) / 2.0,
                    handle_y,
                    HANDLE_SIZE,
                    HANDLE_SIZE,
                ),
            }
        }
        _ => {
            let track_y = bounds.y + (bounds.height - TRACK_WIDTH) / 2.0;
            let filled = bounds.width * percent;
            let handle_x = bounds.x + percent * bounds.width - HANDLE_SIZE / 2.0;
            // Not `f32::clamp`, which panics on a slider narrower than the
            // handle; the handle then sits at the start.
            let handle_x = handle_x
                .min(bounds.x + bounds.width - HANDLE_SIZE)
                .max(bounds.x);
            let (fill_x, handle_x) = if rtl {
                (
                    bounds.x + bounds.width - filled,
                    2.0 * bounds.x + bounds.width - handle_x - HANDLE_SIZE,
                )
            } else {
                (bounds.x, handle_x)
            };
            SliderGeometry {
                track: Rect::new(bounds.x, track_y, bounds.width, TRACK_WIDTH),
                fill: Rect::new(fill_x, track_y, filled, TRACK_WIDTH),
                handle: Rect::new(
                    handle_x,
                    bounds.y + (bounds.height - HANDLE_SIZE) / 2.0,
                    HANDLE_SIZE,
                    HANDLE_SIZE,
                ),
            }
        }
    }
}

/// The quads that draw a handle, in order, as `(rect, corner radius, tone)`:
/// with a border, the border colour over the whole handle and the handle
/// colour inset by the border width, with Iced's inner radius
/// `max(2 - width / 2, 0)` (`widgets.rs:4204-4250`).
pub fn handle_quads(handle: Rect, look: HandleLook) -> Vec<(Rect, f32, Tone)> {
    if look.border_width > 0.0 {
        let inset = look.border_width;
        vec![
            (handle, HANDLE_RADIUS, look.border),
            (
                Rect::new(
                    handle.x + inset,
                    handle.y + inset,
                    handle.width - 2.0 * inset,
                    handle.height - 2.0 * inset,
                ),
                (2.0 - inset * 0.5).max(0.0),
                look.tone,
            ),
        ]
    } else {
        vec![(handle, HANDLE_RADIUS, look.tone)]
    }
}

/// The value a press at `position` (local to a slider of `size`) gives:
/// Iced's mapping over the whole length, then rounded to a multiple of
/// `step` and clamped to the range (`widgets.rs:3807-3822`).
pub fn value_at(
    position: Point,
    size: Size,
    orientation: SliderOrientation,
    rtl: bool,
    range: (f32, f32),
    step: f32,
) -> f32 {
    let (min, max) = range;
    let percent = match orientation {
        SliderOrientation::Vertical => {
            1.0 - (position.y / size.height.max(f32::EPSILON)).clamp(0.0, 1.0)
        }
        _ => {
            let along = (position.x / size.width.max(f32::EPSILON)).clamp(0.0, 1.0);
            if rtl { 1.0 - along } else { along }
        }
    };
    let raw = (min + percent * (max - min)).clamp(min, max);
    snap(raw, step).clamp(min, max)
}

/// `value` rounded to a multiple of `step` (Iced's `(v / step).round() * step`);
/// unchanged when `step` is not positive.
pub fn snap(value: f32, step: f32) -> f32 {
    if step > 0.0 {
        (value / step).round() * step
    } else {
        value
    }
}

/// The [`SliderStyle`] painting the Iced chrome.
pub struct CustomSliderStyle {
    /// The colours, which may change at run time (the equalizer's on/off).
    pub look: Signal<SliderLook>,
    /// Whether the slider is selected (Iced's `is_selected`): the composite
    /// widget decides; a drag always counts as selected.
    pub selected: Signal<bool>,
    /// Where the body records the size it was last painted at, for the
    /// composite's own press handling.
    pub painted_size: Rc<Cell<Size>>,
}

impl SliderStyle for CustomSliderStyle {
    fn make_body(&self, cfg: &SliderStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let body = ctx.add(SliderBody {
            cfg: cfg.clone(),
            look: self.look.clone(),
            selected: self.selected.clone(),
            painted_size: self.painted_size.clone(),
        });
        let registry = ctx.binding_registry();
        cfg.value_normalized
            .bind_to(body, registry, BindingLevel::RepaintOnly);
        cfg.is_hovered
            .bind_to(body, registry, BindingLevel::RepaintOnly);
        cfg.is_dragging
            .bind_to(body, registry, BindingLevel::RepaintOnly);
        self.look.bind_to(body, registry, BindingLevel::RepaintOnly);
        self.selected
            .bind_to(body, registry, BindingLevel::RepaintOnly);
        body
    }

    fn thumb_diameter(&self, _cfg: &SliderStyleConfig) -> f32 {
        0.0
    }
}

/// The leaf that paints a [`CustomSliderStyle`] slider over the slider's
/// whole bounds.
#[derive(Debug)]
struct SliderBody {
    cfg: SliderStyleConfig,
    look: Signal<SliderLook>,
    selected: Signal<bool>,
    painted_size: Rc<Cell<Size>>,
}

impl Widget for SliderBody {
    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        self.painted_size
            .set(Size::new(bounds.width, bounds.height));
        let colors = &ctx.theme.colors;
        let look = self.look.get();
        let rtl = ctx.layout_direction == LayoutDirection::RightToLeft;
        let parts = geometry(
            bounds,
            self.cfg.orientation,
            self.cfg.value_normalized.get(),
            rtl,
        );
        let round = CornerRadius::uniform(TRACK_WIDTH / 2.0);
        canvas.fill_rounded_rect(parts.track, round, look.track.resolve(colors));
        if let Some(fill) = look.fill
            && parts.fill.width > 0.0
            && parts.fill.height > 0.0
        {
            canvas.fill_rounded_rect(parts.fill, round, fill.resolve(colors));
        }
        let selected = self.selected.get() || self.cfg.is_dragging.get();
        let handle = look.handle(self.cfg.is_hovered.get(), selected);
        for (rect, radius, tone) in handle_quads(parts.handle, handle) {
            canvas.fill_rounded_rect(rect, CornerRadius::uniform(radius), tone.resolve(colors));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EQ: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 24.0,
        height: 240.0,
    };

    #[test]
    fn a_vertical_slider_draws_a_centred_track_and_a_handle_on_the_value() {
        let middle = geometry(EQ, SliderOrientation::Vertical, 0.5, false);
        assert_eq!(middle.track, Rect::new(8.0, 0.0, 8.0, 240.0));
        assert_eq!(middle.fill, Rect::new(8.0, 120.0, 8.0, 120.0));
        assert_eq!(middle.handle, Rect::new(4.0, 112.0, 16.0, 16.0));
    }

    #[test]
    fn the_handle_stays_inside_the_bounds_at_both_ends() {
        let top = geometry(EQ, SliderOrientation::Vertical, 1.0, false);
        assert_eq!(top.handle.y, 0.0);
        let bottom = geometry(EQ, SliderOrientation::Vertical, 0.0, false);
        assert_eq!(bottom.handle.y, 224.0);
        assert_eq!(bottom.fill.height, 0.0);
    }

    #[test]
    fn a_horizontal_slider_fills_from_the_leading_edge() {
        let bounds = Rect::new(10.0, 0.0, 200.0, 18.0);
        let quarter = geometry(bounds, SliderOrientation::Horizontal, 0.25, false);
        assert_eq!(quarter.track, Rect::new(10.0, 5.0, 200.0, 8.0));
        assert_eq!(quarter.fill, Rect::new(10.0, 5.0, 50.0, 8.0));
        assert_eq!(quarter.handle, Rect::new(52.0, 1.0, 16.0, 16.0));
        let mirrored = geometry(bounds, SliderOrientation::Horizontal, 0.25, true);
        assert_eq!(mirrored.fill, Rect::new(160.0, 5.0, 50.0, 8.0));
        assert_eq!(mirrored.handle, Rect::new(152.0, 1.0, 16.0, 16.0));
    }

    #[test]
    fn a_bordered_handle_is_an_outline_with_an_inset_fill() {
        let handle = Rect::new(4.0, 112.0, 16.0, 16.0);
        let selected_off = SliderLook::EQ_OFF.handle(false, true);
        assert_eq!(
            handle_quads(handle, selected_off),
            vec![
                (handle, 2.0, Tone::TextSecondary),
                (Rect::new(5.0, 113.0, 14.0, 14.0), 1.5, Tone::Contrast),
            ]
        );
        let idle_off = SliderLook::EQ_OFF.handle(false, false);
        assert_eq!(
            handle_quads(handle, idle_off),
            vec![(handle, 2.0, Tone::Contrast)]
        );
    }

    #[test]
    fn the_handle_colour_follows_selected_then_hovered_then_idle() {
        let on = SliderLook::EQ_ON;
        assert_eq!(on.handle(false, false).tone, Tone::Accent);
        assert_eq!(on.handle(true, false).tone, Tone::TextPrimary);
        assert_eq!(on.handle(true, true).tone, Tone::TextPrimary);
        let off = SliderLook::EQ_OFF;
        assert_eq!(off.handle(true, false).tone, Tone::Accent);
        assert_eq!(off.handle(true, true).tone, Tone::Contrast);
    }

    #[test]
    fn a_press_maps_over_the_whole_length_and_snaps_to_the_step() {
        let size = Size::new(24.0, 240.0);
        let at = |y| {
            value_at(
                Point::new(12.0, y),
                size,
                SliderOrientation::Vertical,
                false,
                (-9.0, 9.0),
                0.1,
            )
        };
        assert_eq!(at(0.0), 9.0);
        assert_eq!(at(120.0), 0.0);
        assert_eq!(at(240.0), -9.0);
        assert!((at(20.0) - 7.5).abs() < 1e-5);
        assert!((at(21.0) - 7.4).abs() < 1e-5);
        assert_eq!(at(-30.0), 9.0);
        let wide = Size::new(200.0, 18.0);
        let across = |x, rtl| {
            value_at(
                Point::new(x, 9.0),
                wide,
                SliderOrientation::Horizontal,
                rtl,
                (0.0, 100.0),
                1.0,
            )
        };
        assert_eq!(across(50.0, false), 25.0);
        assert_eq!(across(50.0, true), 75.0);
    }

    #[test]
    fn snapping_rounds_to_the_nearest_multiple_of_the_step() {
        assert!((snap(1.26, 0.1) - 1.3).abs() < 1e-6);
        assert!((snap(-0.04, 0.1)).abs() < 1e-6);
        assert_eq!(snap(1.26, 0.0), 1.26);
    }
}
