//! Paint helpers shared by the value controls: the sliders, the stepper and
//! the VU meter draw their own chrome on a [`Canvas`], as the Iced widgets do
//! with `fill_quad` and `fill_text`.
//!
//! - [`Tone`] names a colour by its role in the Audoxidy palette, resolved
//!   against the theme at paint time, so the controls follow the theme like
//!   every other widget (`style.rs` maps the seven Iced colours onto it).
//! - [`IcedText`] places a line of text the way Iced's renderer does
//!   (`iced_wgpu` 0.14, `text.rs`, the `Text::Cached` arm): the anchor point is
//!   the text's left edge or its centre, its top or its middle, on a line box
//!   as tall as the font size; the text is then clipped to a rectangle. The
//!   Iced meter relies on all three, clipping included.

use teksilo::canvas::{Canvas, Point, Rect};
use teksilo::prelude::PaintContext;
use teksilo::tokens::{Color, ColorTokens, FontWeight, TextStyle};

use crate::teksilo_ui::style;

/// A palette colour, by role.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tone {
    /// `COLOR_ACCENT` (`accent`).
    Accent,
    /// `COLOR_CONTRAST` (`surface_raised`).
    Contrast,
    /// `COLOR_BG` (`surface_main`).
    Background,
    /// `COLOR_TEXT_PRIMARY` (`text_primary`).
    TextPrimary,
    /// `COLOR_TEXT_SECONDARY` (`text_secondary`).
    TextSecondary,
    /// `COLOR_TEXT_SECONDARY` with its alpha scaled by the factor (Iced's
    /// `scale_alpha`).
    TextSecondaryAlpha(f32),
    /// Nothing.
    Transparent,
    /// A colour that is not a theme role (the meter's level zones).
    Fixed(Color),
}

impl Tone {
    /// The colour this tone has in `colors`.
    pub fn resolve(self, colors: &ColorTokens) -> Color {
        match self {
            Tone::Accent => colors.accent,
            Tone::Contrast => colors.surface_raised,
            Tone::Background => colors.surface_main,
            Tone::TextPrimary => colors.text_primary,
            Tone::TextSecondary => colors.text_secondary,
            Tone::TextSecondaryAlpha(factor) => {
                let base = colors.text_secondary;
                base.with_alpha(base.a() * factor)
            }
            Tone::Transparent => Color::TRANSPARENT,
            Tone::Fixed(color) => color,
        }
    }
}

/// Horizontal anchoring of an [`IcedText`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorX {
    /// The anchor is the text's left edge.
    Left,
    /// The anchor is the text's horizontal centre.
    Center,
}

/// Vertical anchoring of an [`IcedText`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorY {
    /// The anchor is the top of the line box.
    Top,
    /// The anchor is the middle of the line box.
    Center,
}

/// One line of Inter Medium text placed as Iced places it; see the module
/// documentation.
#[derive(Debug, Clone, PartialEq)]
pub struct IcedText {
    /// The text.
    pub content: String,
    /// The font size, which is also the line box height (Iced's
    /// `LineHeight::Relative(1.0)`).
    pub size: f32,
    /// The anchor point.
    pub anchor: Point,
    /// What the anchor's x names.
    pub anchor_x: AnchorX,
    /// What the anchor's y names.
    pub anchor_y: AnchorY,
    /// The rectangle the text is clipped to.
    pub clip: Rect,
    /// The colour.
    pub tone: Tone,
}

impl IcedText {
    /// The top-left corner of a `width` × `height` layout of this text, so
    /// that its middle lands where the middle of Iced's line box does.
    pub fn origin(&self, width: f32, height: f32) -> Point {
        let left = match self.anchor_x {
            AnchorX::Left => self.anchor.x,
            AnchorX::Center => self.anchor.x - width / 2.0,
        };
        let line_top = match self.anchor_y {
            AnchorY::Top => self.anchor.y,
            AnchorY::Center => self.anchor.y - self.size / 2.0,
        };
        Point::new(left, line_top + self.size / 2.0 - height / 2.0)
    }

    /// Draws the text, clipped to its rectangle (within whatever clip the
    /// ancestors set). Draws nothing when the canvas has no text backend.
    pub fn paint(&self, canvas: &mut Canvas, ctx: &PaintContext) {
        let style = style::inter(self.size, FontWeight::MEDIUM);
        let Some(layout) = canvas.text_backend().map(|backend| {
            backend
                .borrow_mut()
                .layout_single_line(&self.content, &style, None)
        }) else {
            return;
        };
        let clip = match ctx.clip_bounds {
            Some(outer) => intersection(outer, self.clip),
            None => Some(self.clip),
        };
        let Some(clip) = clip else {
            return;
        };
        canvas.set_clip(clip);
        let origin = self.origin(layout.width, layout.height);
        canvas.draw_text_layout(&layout, origin, self.tone.resolve(&ctx.theme.colors));
        match ctx.clip_bounds {
            Some(outer) => canvas.set_clip(outer),
            None => canvas.clear_clip(),
        }
    }
}

/// Draws `text` in `style`, centred in `rect` (both ways). Draws nothing
/// when the canvas has no text backend.
pub fn draw_centered(canvas: &mut Canvas, text: &str, style: &TextStyle, rect: Rect, color: Color) {
    let Some(layout) = canvas
        .text_backend()
        .map(|backend| backend.borrow_mut().layout_single_line(text, style, None))
    else {
        return;
    };
    let origin = Point::new(
        rect.x + (rect.width - layout.width) / 2.0,
        rect.y + (rect.height - layout.height) / 2.0,
    );
    canvas.draw_text_layout(&layout, origin, color);
}

/// The overlap of two rectangles, if they overlap.
pub fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    (right > left && bottom > top).then(|| Rect::new(left, top, right - left, bottom - top))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(anchor_x: AnchorX, anchor_y: AnchorY) -> IcedText {
        IcedText {
            content: "L".into(),
            size: 8.0,
            anchor: Point::new(11.0, 1.0),
            anchor_x,
            anchor_y,
            clip: Rect::new(11.0, 1.0, 10.0, 6.0),
            tone: Tone::TextSecondary,
        }
    }

    #[test]
    fn a_centred_anchor_is_the_middle_of_the_text_and_of_its_line_box() {
        // A 4 × 10 layout of an 8 px line: centred on (11, 1).
        let origin = text(AnchorX::Center, AnchorY::Center).origin(4.0, 10.0);
        assert_eq!(origin, Point::new(9.0, -4.0));
    }

    #[test]
    fn a_left_top_anchor_is_the_left_edge_and_the_top_of_the_line_box() {
        // The 10 px layout is centred on the 8 px line box below (11, 1).
        let origin = text(AnchorX::Left, AnchorY::Top).origin(4.0, 10.0);
        assert_eq!(origin, Point::new(11.0, 0.0));
    }

    #[test]
    fn rectangles_intersect_only_where_they_overlap() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            intersection(a, Rect::new(5.0, 5.0, 10.0, 10.0)),
            Some(Rect::new(5.0, 5.0, 5.0, 5.0))
        );
        assert_eq!(intersection(a, Rect::new(10.0, 0.0, 5.0, 5.0)), None);
    }

    #[test]
    fn tones_resolve_to_the_audoxidy_palette() {
        let colors = style::colors();
        assert_eq!(Tone::Accent.resolve(&colors), style::ACCENT);
        assert_eq!(Tone::Contrast.resolve(&colors), style::CONTRAST);
        assert_eq!(Tone::Background.resolve(&colors), style::BACKGROUND);
        assert_eq!(Tone::TextPrimary.resolve(&colors), style::TEXT_PRIMARY);
        assert_eq!(Tone::TextSecondary.resolve(&colors), style::TEXT_SECONDARY);
        let faded = Tone::TextSecondaryAlpha(0.3).resolve(&colors);
        assert!((faded.a() - 0.3).abs() < 1e-6);
        assert_eq!(Tone::Transparent.resolve(&colors).a(), 0.0);
    }
}
