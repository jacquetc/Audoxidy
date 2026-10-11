//! The fixed-size text button of the preset dialogs.
//!
//! The Load dialog's buttons are 150 × 30 px and the Save dialog's 74 × 30
//! (`app.rs:5428-5453`, `5577-5605`): Inter Medium 14 px centred in the box,
//! `COLOR_CONTRAST` with a secondary label at rest, accent with a primary
//! label while hovered, a 6 px radius, and a label at half strength while
//! disabled. The shared `text_button` sizes itself to its label, so this one
//! fixes the box.

use teksilo::prelude::*;
use teksilo::tokens::FontWeight;
use teksilo::widgets::{Center, FixedSize, RectWidget, TextWidget, ZStack};

use crate::teksilo_ui::style::{self, inter};
use crate::teksilo_ui::widgets::Pressable;
use crate::teksilo_ui::widgets::dialog_frame::{
    BUTTON_RADIUS, TEXT_SIZE, button_fill, button_text,
};

/// The label's alpha while the button is disabled (`app.rs:5436`, `5593`).
const DISABLED_ALPHA: f32 = 0.5;

/// A `width` × `height` button labelled `label`, enabled while `enabled`
/// holds. Chain `on_activate` onto it.
pub fn fixed_button(
    label: LocalizedString,
    width: f32,
    height: f32,
    enabled: impl Into<Prop<bool>>,
) -> Pressable {
    let enabled: Prop<bool> = enabled.into();
    let on = match &enabled {
        Prop::Static(on) => Signal::new(*on),
        Prop::Bound(signal) => signal.clone(),
    };
    let hovered = Signal::new(false);
    let lit_when = on.clone();
    let fill_hovered = hovered.clone();
    let fill_on = on.clone();
    let fill = fill_hovered
        .zip(&fill_on)
        .map(|(hovered, on)| button_fill(*hovered && *on));
    let live = lit_when.clone();
    let text_hovered = hovered.clone();
    let enabled_text = TextWidget::new(label.clone())
        .style(inter(TEXT_SIZE, FontWeight::MEDIUM))
        .color(text_hovered.map(|hovered| button_text(*hovered)))
        .single_line()
        .a11y_hidden()
        .visible_when(live);
    // Disabled: the secondary colour at half strength. No colour role carries
    // an alpha, so this is a literal over the palette's colour.
    let disabled_text = TextWidget::new(label.clone())
        .style(inter(TEXT_SIZE, FontWeight::MEDIUM))
        .color(style::TEXT_SECONDARY.with_alpha(DISABLED_ALPHA))
        .single_line()
        .a11y_hidden()
        .visible_when(on.map(|on| !*on));
    let body = FixedSize::new().width(width).height(height).child(
        ZStack::new()
            .child(
                RectWidget::new()
                    .background(fill)
                    .corner_radius(CornerRadius::uniform(BUTTON_RADIUS)),
            )
            .child(Center::new().child(ZStack::new().child(enabled_text).child(disabled_text))),
    );
    Pressable::new(body, label)
        .hovered(hovered)
        .enabled(enabled)
        .corner_radius(BUTTON_RADIUS)
}
