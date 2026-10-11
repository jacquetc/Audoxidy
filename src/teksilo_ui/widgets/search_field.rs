//! [`SearchField`]: the search box of the bottom bars.
//!
//! Port of `standard_search_input` (`src/gui/widgets.rs:1852`), used by the
//! playlist, library and filter bars with the placeholder "Buscar..." and a
//! width of 180 px:
//!
//! - a 24 px high strip, the caller's width;
//! - a text field in `COLOR_CONTRAST` with no border and no radius, its text
//!   flush with the leading edge and 25 px kept free at the trailing end for
//!   the clear button (`widgets.rs:1865`);
//! - while the field holds text, an "x" (Inter Medium 14 px, primary) centred
//!   in a 20 × 20 button 5 px from the trailing edge, which empties the field
//!   (`widgets.rs:1898`).
//!
//! The field is the stock `TextInput`, restyled by [`SearchInputStyle`], so it
//! keeps the framework's editing, selection, clipboard and IME. The query is a
//! `Signal<String>` the caller owns; the clear button writes an empty string to
//! it, which is what the Iced `on_clear` message did.
//!
//! Escape clears a field that holds text (quirk fix 35; the Iced field only
//! drops the keyboard focus, `text_input.rs` in iced_widget 0.14). On an empty
//! field Escape is left alone, so it still reaches the overlay or dialog
//! around the field (quirk fix 4).

use teksilo::core::styles::{TextInputStyle, TextInputStyleConfig};
use teksilo::core::{Key, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight};
use teksilo::widgets::{Center, FixedSize, Padding, Panel, TextInput, TextWidget, ZStack};

use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;

/// The strip's height (`widgets.rs:1937`).
pub const HEIGHT: f32 = 24.0;
/// The space kept free after the text for the clear button
/// (`widgets.rs:1865`).
pub const TRAILING_RESERVE: f32 = 25.0;
/// The clear button's side (`widgets.rs:1905`).
pub const CLEAR_SIZE: f32 = 20.0;
/// The clear button's distance from the trailing edge (`widgets.rs:1921`).
pub const CLEAR_TRAILING: f32 = 5.0;
/// The text size of the field and of the "x" (`widgets.rs:1868`).
pub const TEXT_SIZE: f32 = 14.0;
/// The height the stock `TextInput` keeps as its minimum; the strip centres
/// it, as Iced centres its shorter field in the 24 px container.
const FIELD_MIN_HEIGHT: f32 = 28.0;

/// The Iced search field's frame: `COLOR_CONTRAST`, no border, no radius,
/// no padding but the 25 px trailing reserve (`widgets.rs:1872-1894`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchInputStyle;

impl TextInputStyle for SearchInputStyle {
    fn make_body(&self, cfg: &TextInputStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .corner_radius(0.0)
                .background(SurfaceRole::Raised)
                .a11y_presentational()
                .child(Padding::new(0.0, TRAILING_RESERVE, 0.0, 0.0).child(cfg.editor)),
        )
    }
}

/// The search box; see the module documentation.
pub struct SearchField {
    text: Signal<String>,
    placeholder: LocalizedString,
    label: Option<LocalizedString>,
    width: f32,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for SearchField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchField")
            .field("placeholder", &self.placeholder.resolve_now())
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

impl SearchField {
    /// A search box `width` px wide editing `text`, showing `placeholder`
    /// while empty (the call sites' "Buscar...").
    pub fn new(text: Signal<String>, placeholder: impl Into<LocalizedString>, width: f32) -> Self {
        Self {
            text,
            placeholder: placeholder.into(),
            label: None,
            width,
            root: None,
        }
    }

    /// The field's accessible name (default: the placeholder).
    pub fn label(mut self, label: impl Into<LocalizedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The query.
    pub fn text(&self) -> Signal<String> {
        self.text.clone()
    }
}

impl Widget for SearchField {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| self.placeholder.clone());
        let escape_query = self.text.clone();
        let field = FixedSize::new()
            .width(self.width)
            .height(FIELD_MIN_HEIGHT)
            .child(
                TextInput::new(self.text.clone())
                    .style(SearchInputStyle)
                    .placeholder(self.placeholder.clone())
                    .label(label),
            )
            // A preview handler, so Escape is seen before the field consumes it.
            .on_key_preview(move |event, _ctx| match event {
                WidgetEvent::KeyDown {
                    key: Key::Escape, ..
                } if !escape_query.get().is_empty() => {
                    escape_query.set(String::new());
                    EventResponse::Handled
                }
                _ => EventResponse::Ignored,
            });
        let has_text = self.text.map(|text| !text.is_empty());
        let query = self.text.clone();
        let clear = Pressable::new(
            FixedSize::new().width(CLEAR_SIZE).height(CLEAR_SIZE).child(
                Center::new().child(
                    TextWidget::new(lit!("x"))
                        .style(style::inter(TEXT_SIZE, FontWeight::MEDIUM))
                        .color(TextRole::Primary)
                        .single_line()
                        .no_shrink()
                        .a11y_hidden(),
                ),
            ),
            tr!(chrome_search_clear()),
        )
        .on_activate(move |_| query.set(String::new()));
        let root = ctx.add(
            FixedSize::new().width(self.width).height(HEIGHT).child(
                ZStack::new()
                    .alignment(Alignment::CENTER_TRAILING)
                    .child(field)
                    .child(
                        Padding::new(0.0, CLEAR_TRAILING, 0.0, 0.0)
                            .child(clear)
                            .visible_when(has_text),
                    ),
            ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| Size::new(self.width, HEIGHT))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
