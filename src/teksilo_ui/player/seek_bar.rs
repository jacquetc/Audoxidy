//! The seek bar: a 5 px rail with no handle, white up to the position, or a
//! flat rail when the duration is unknown.
//!
//! Port of the bottom strip of `src/gui/player.rs:226-288`. The Iced strip is
//! 40 px high with a 25 % black background; its slider sits 13 px in from the
//! sides and 15 px from the top and bottom (a 374 x 10 px area, its rail
//! centred), and is Iced's default slider with the handle made invisible and
//! the rail set to 5 px, white to the position and `COLOR_CONTRAST` after it.
//! A press or drag anywhere on it seeks to that fraction of the track.
//!
//! The slider keeps Iced's step of 0.001 (`player.rs:242`): it is the grid a
//! drag snaps to and what an arrow key over the hovered Iced slider moves
//! (`iced_widget` `slider.rs:289-320`, `393-405`). Its accessible value is the
//! position as a time, not the fraction (Iced has no accessibility).
//!
//! When the duration is unknown ([`fine_seek_enabled`]) there is nothing to
//! seek to, so the strip shows a flat, inert rail instead of the slider:
//! `COLOR_CONTRAST` at 50 %, with no handle, no jump and nothing to drag.
//!
//! [`fine_seek_enabled`]: crate::logic::transport::fine_seek_enabled

use teksilo::canvas::{Canvas, Rect};
use teksilo::core::binding::BindingLevel;
use teksilo::core::environment::LayoutDirection;
use teksilo::core::styles::{SliderStyle, SliderStyleConfig};
use teksilo::prelude::*;
use teksilo::tokens::CornerRadius;
use teksilo::widgets::{FixedSize, RectWidget, Slider, ZStack};

use super::geometry::{
    CONTRAST_ALPHA, SEEK_BAR_ALPHA, SEEK_HEIGHT, SEEK_PADDING_X, SEEK_RAIL, SEEK_RAIL_RADIUS, SIZE,
};
use super::player_vm::PlayerViewModel;
use crate::teksilo_ui::style;

/// The slider's area: the strip less its padding (`player.rs:271`).
pub const SLIDER_WIDTH: f32 = SIZE - 2.0 * SEEK_PADDING_X;
/// The slider's height after the 15 px padding above and below.
pub const SLIDER_HEIGHT: f32 = 10.0;

/// The rail colours Iced sets: white to the position (`player.rs:244`).
pub const RAIL_PLAYED: Color = Color::WHITE;

/// The slider's rail geometry for a slider in `bounds` at `fraction`
/// (0..=1): the played part and the rest, each `SEEK_RAIL` high and centred
/// (`iced_widget` `slider.rs:476-504` with a zero-sized handle). A horizontal
/// slider in a right-to-left layout has its minimum on the right.
pub fn rails(bounds: Rect, fraction: f32, rtl: bool) -> (Rect, Rect) {
    let fraction = fraction.clamp(0.0, 1.0);
    let played = bounds.width * fraction;
    let y = bounds.y + (bounds.height - SEEK_RAIL) / 2.0;
    if rtl {
        (
            Rect::new(bounds.x + bounds.width - played, y, played, SEEK_RAIL),
            Rect::new(bounds.x, y, bounds.width - played, SEEK_RAIL),
        )
    } else {
        (
            Rect::new(bounds.x, y, played, SEEK_RAIL),
            Rect::new(bounds.x + played, y, bounds.width - played, SEEK_RAIL),
        )
    }
}

/// The slider's look: the two rails, no handle.
struct SeekStyle;

impl SliderStyle for SeekStyle {
    fn make_body(&self, cfg: &SliderStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let body = ctx.add(SeekRail {
            fraction: cfg.value_normalized.clone(),
        });
        cfg.value_normalized
            .bind_to(body, ctx.binding_registry(), BindingLevel::RepaintOnly);
        body
    }

    /// Iced maps a position to a value over the whole width: no handle.
    fn thumb_diameter(&self, _cfg: &SliderStyleConfig) -> f32 {
        0.0
    }
}

/// The leaf that paints the rails over the slider's bounds.
#[derive(Debug)]
struct SeekRail {
    fraction: Signal<f32>,
}

impl Widget for SeekRail {
    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let rtl = ctx.layout_direction == LayoutDirection::RightToLeft;
        let (played, rest) = rails(bounds, self.fraction.get(), rtl);
        let radius = CornerRadius::uniform(SEEK_RAIL_RADIUS);
        if played.width > 0.0 {
            canvas.fill_rounded_rect(played, radius, RAIL_PLAYED);
        }
        if rest.width > 0.0 {
            canvas.fill_rounded_rect(rest, radius, ctx.theme.colors.surface_raised);
        }
    }
}

/// The seek strip; see the module documentation.
pub struct SeekBar {
    vm: PlayerViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for SeekBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeekBar").finish_non_exhaustive()
    }
}

impl SeekBar {
    /// The strip for `vm`.
    pub fn new(vm: PlayerViewModel) -> Self {
        Self { vm, root: None }
    }

    /// The slider, or the flat rail when the duration is unknown.
    fn control(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        if self.vm.duration_known().get() {
            let vm = self.vm.clone();
            // The slider's value is a fraction of the track; assistive
            // technologies are told the position as a time ("1:23 of 4:05"),
            // in the current locale.
            let times = self.vm.elapsed().zip(&self.vm.total_time());
            let value_text = ctx
                .locale_signal()
                .zip(&times)
                .map(|(_, (elapsed, total))| {
                    tr!(player_seek_value(
                        elapsed = elapsed.clone(),
                        total = total.clone()
                    ))
                    .resolve_now()
                });
            Box::new(
                FixedSize::new()
                    .width(SLIDER_WIDTH)
                    .height(SLIDER_HEIGHT)
                    .child(
                        Slider::new(self.vm.seek_value(), 0.0, 1.0)
                            .step(0.001)
                            .style(SeekStyle)
                            .enabled(self.vm.transport_enabled())
                            .label(tr!(player_seek()))
                            .on_change(move |fraction, _ctx| {
                                vm.seek_fraction(fraction);
                            })
                            .access_value(value_text),
                    ),
            )
        } else {
            // `COLOR_CONTRAST` at 50 % (`player.rs:256-265`): a literal alpha
            // over the theme's raised surface.
            Box::new(
                FixedSize::new()
                    .width(SLIDER_WIDTH)
                    .height(SEEK_RAIL)
                    .child(
                        RectWidget::new().background(style::CONTRAST.with_alpha(CONTRAST_ALPHA)),
                    ),
            )
        }
    }
}

impl Widget for SeekBar {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.duration_known().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Rebuild,
        );
        let strip = FixedSize::new().width(SIZE).height(SEEK_HEIGHT).child(
            ZStack::new()
                .child(RectWidget::new().background(Color::new(0.0, 0.0, 0.0, SEEK_BAR_ALPHA)))
                .child(self.control(ctx)),
        );
        let root = ctx.add(strip);
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rect {
        Rect::new(13.0, 365.0, 374.0, 10.0)
    }

    #[test]
    fn the_rails_are_five_pixels_high_and_centred_in_the_slider() {
        let (played, rest) = rails(bounds(), 0.5, false);
        assert_eq!((played.y, played.height), (367.5, 5.0));
        assert_eq!((rest.y, rest.height), (367.5, 5.0));
    }

    #[test]
    fn the_played_rail_covers_the_fraction_and_the_rest_covers_the_remainder() {
        let (played, rest) = rails(bounds(), 0.25, false);
        assert_eq!((played.x, played.width), (13.0, 93.5));
        assert_eq!((rest.x, rest.width), (106.5, 280.5));
        assert_eq!(played.width + rest.width, 374.0);
    }

    #[test]
    fn the_ends_have_one_empty_rail() {
        let (played, rest) = rails(bounds(), 0.0, false);
        assert_eq!(played.width, 0.0);
        assert_eq!(rest.width, 374.0);
        let (played, rest) = rails(bounds(), 1.0, false);
        assert_eq!(played.width, 374.0);
        assert_eq!(rest.width, 0.0);
    }

    #[test]
    fn a_right_to_left_slider_plays_from_the_right() {
        let (played, rest) = rails(bounds(), 0.25, true);
        assert_eq!((played.x, played.width), (13.0 + 374.0 - 93.5, 93.5));
        assert_eq!((rest.x, rest.width), (13.0, 280.5));
    }

    #[test]
    fn the_strip_geometry_is_iceds() {
        assert_eq!(SLIDER_WIDTH, 374.0);
        assert_eq!(SEEK_HEIGHT, 40.0);
    }
}
