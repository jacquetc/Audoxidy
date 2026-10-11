//! What the stepper paints (`src/gui/widgets.rs:322-380`, `711-822`): three
//! zones side by side, 13 px high.
//!
//! | Zone  | Width | Background                               | Content                     |
//! | ----- | ----- | ---------------------------------------- | --------------------------- |
//! | minus | 15 px | contrast; accent under the pointer       | "-", Inter Medium 17 px     |
//! | value | 66 px | background                               | the value, Inter Medium 13 px |
//! | plus  | 15 px | contrast; accent under the pointer       | "+", Inter Medium 17 px     |
//!
//! The glyphs sit 1 px above the zone's centre (Iced lays them out with a
//! -2 px top padding). Disabled, the glyphs and the value are in the
//! secondary text colour and the zones never light up. While the value is
//! being edited the text field covers the value zone, so the value is not
//! painted under it.

use teksilo::canvas::{Canvas, Point, Rect};
use teksilo::prelude::*;
use teksilo::tokens::FontWeight;

use crate::logic::stepper::{StepperUnit, format_stepper_value};
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::eq_slider::chrome::{Tone, draw_centered};

/// The stepper's width (`widgets.rs:433`).
pub const WIDTH: f32 = 96.0;
/// The stepper's height (`widgets.rs:434`).
pub const HEIGHT: f32 = 13.0;
/// The width of each arrow zone (`widgets.rs:338`, `359`).
pub const ARROW_WIDTH: f32 = 15.0;
/// The width of the value zone (`widgets.rs:373`).
pub const VALUE_WIDTH: f32 = 66.0;
/// The arrow glyphs' size (`widgets.rs:330`).
pub const GLYPH_SIZE: f32 = 17.0;
/// How far the glyphs sit above the zone's centre: half of the -2 px top
/// padding of their 13 px container (`widgets.rs:340-345`).
pub const GLYPH_LIFT: f32 = 1.0;
/// The value's size (`widgets.rs:810`).
pub const VALUE_SIZE: f32 = 13.0;

/// A part of the stepper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Outside every zone.
    None,
    /// The "-" arrow.
    Minus,
    /// The value.
    Value,
    /// The "+" arrow.
    Plus,
}

impl Zone {
    /// The zone under `point`, local to the stepper (`widgets.rs:492-513`).
    pub fn at(point: Point) -> Zone {
        if !(0.0..HEIGHT).contains(&point.y) {
            return Zone::None;
        }
        match point.x {
            x if (0.0..ARROW_WIDTH).contains(&x) => Zone::Minus,
            x if (ARROW_WIDTH..ARROW_WIDTH + VALUE_WIDTH).contains(&x) => Zone::Value,
            x if (ARROW_WIDTH + VALUE_WIDTH..WIDTH).contains(&x) => Zone::Plus,
            _ => Zone::None,
        }
    }

    /// The step direction of an arrow zone.
    pub fn direction(self) -> Option<i8> {
        match self {
            Zone::Minus => Some(-1),
            Zone::Plus => Some(1),
            _ => None,
        }
    }

    /// The zone's rectangle inside a stepper at `bounds`.
    pub fn rect(self, bounds: Rect) -> Option<Rect> {
        let x = match self {
            Zone::Minus => bounds.x,
            Zone::Value => bounds.x + ARROW_WIDTH,
            Zone::Plus => bounds.x + ARROW_WIDTH + VALUE_WIDTH,
            Zone::None => return None,
        };
        let width = if self == Zone::Value {
            VALUE_WIDTH
        } else {
            ARROW_WIDTH
        };
        Some(Rect::new(x, bounds.y, width, HEIGHT))
    }
}

/// The background of an arrow zone (`widgets.rs:747-756`).
pub fn arrow_tone(zone: Zone, hovered: Zone, enabled: bool) -> Tone {
    if enabled && zone == hovered {
        Tone::Accent
    } else {
        Tone::Contrast
    }
}

/// The colour of the glyphs and of the value (`widgets.rs:316-320`, `801-805`).
pub fn text_tone(enabled: bool) -> Tone {
    if enabled {
        Tone::TextPrimary
    } else {
        Tone::TextSecondary
    }
}

/// The leaf that paints the three zones.
#[derive(Debug)]
pub struct StepperFace {
    /// The value shown.
    pub value: Signal<f64>,
    /// Its unit, for the suffix.
    pub unit: StepperUnit,
    /// The zone under the pointer.
    pub hovered: Signal<Zone>,
    /// Whether the text field covers the value.
    pub editing: Signal<bool>,
}

impl Widget for StepperFace {
    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        teksilo::canvas::Size::new(WIDTH, HEIGHT).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let colors = &ctx.theme.colors;
        let enabled = ctx.effective_enabled;
        let hovered = self.hovered.get();
        let glyph = style::inter(GLYPH_SIZE, FontWeight::MEDIUM);
        let text = text_tone(enabled).resolve(colors);
        for (zone, sign) in [(Zone::Minus, "-"), (Zone::Plus, "+")] {
            if let Some(rect) = zone.rect(bounds) {
                canvas.fill_rect(rect, arrow_tone(zone, hovered, enabled).resolve(colors));
                let lifted = Rect::new(rect.x, rect.y - GLYPH_LIFT, rect.width, rect.height);
                draw_centered(canvas, sign, &glyph, lifted, text);
            }
        }
        if let Some(rect) = Zone::Value.rect(bounds) {
            canvas.fill_rect(rect, Tone::Background.resolve(colors));
            if !self.editing.get() {
                let shown = format_stepper_value(self.value.get(), &self.unit);
                let style = style::inter(VALUE_SIZE, FontWeight::MEDIUM);
                draw_centered(canvas, &shown, &style, rect, text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zones_are_fifteen_sixty_six_and_fifteen_pixels_wide() {
        assert_eq!(Zone::at(Point::new(0.0, 5.0)), Zone::Minus);
        assert_eq!(Zone::at(Point::new(14.9, 5.0)), Zone::Minus);
        assert_eq!(Zone::at(Point::new(15.0, 5.0)), Zone::Value);
        assert_eq!(Zone::at(Point::new(80.9, 5.0)), Zone::Value);
        assert_eq!(Zone::at(Point::new(81.0, 5.0)), Zone::Plus);
        assert_eq!(Zone::at(Point::new(95.9, 12.9)), Zone::Plus);
        assert_eq!(Zone::at(Point::new(96.0, 5.0)), Zone::None);
        assert_eq!(Zone::at(Point::new(50.0, 13.0)), Zone::None);
        assert_eq!(ARROW_WIDTH * 2.0 + VALUE_WIDTH, WIDTH);
    }

    #[test]
    fn only_an_enabled_hovered_arrow_lights_up() {
        assert_eq!(arrow_tone(Zone::Minus, Zone::Minus, true), Tone::Accent);
        assert_eq!(arrow_tone(Zone::Minus, Zone::Plus, true), Tone::Contrast);
        assert_eq!(arrow_tone(Zone::Plus, Zone::Plus, false), Tone::Contrast);
        assert_eq!(text_tone(true), Tone::TextPrimary);
        assert_eq!(text_tone(false), Tone::TextSecondary);
    }

    #[test]
    fn each_zone_has_its_rectangle() {
        let bounds = Rect::new(100.0, 10.0, WIDTH, HEIGHT);
        assert_eq!(
            Zone::Minus.rect(bounds),
            Some(Rect::new(100.0, 10.0, 15.0, 13.0))
        );
        assert_eq!(
            Zone::Value.rect(bounds),
            Some(Rect::new(115.0, 10.0, 66.0, 13.0))
        );
        assert_eq!(
            Zone::Plus.rect(bounds),
            Some(Rect::new(181.0, 10.0, 15.0, 13.0))
        );
        assert_eq!(Zone::None.rect(bounds), None);
    }
}
