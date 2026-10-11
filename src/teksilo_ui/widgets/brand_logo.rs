//! [`BrandLogo`]: the "AuDoxiDY" mark.
//!
//! The Iced UI writes the brand in the Stage Wander face in four places, at
//! four sizes, in one of two colourings:
//!
//! - one colour: the title bar (19 px, primary, `library_filters.rs:164`),
//!   the large album placeholder (11 px, secondary, `widgets.rs:1487`) and the
//!   player placeholder (40 px, primary, `widgets.rs:1528`);
//! - two tones: the Audio Center header (18 px, `audio_center.rs:1817`), a row
//!   of five runs where "A", "o" and "Y" are in the accent and "uD" and "xiD"
//!   in the primary text colour (see `assets/screenshot.png`).
//!
//! A one-colour logo is a single text, kerned as one word; the two-tone logo is
//! a row of five texts with no spacing, as the Iced row of `text`s is. Either
//! way it is one accessibility node, an image named "Audoxidy".

use teksilo::core::accesskit::Role;
use teksilo::prelude::*;
use teksilo::tokens::VAlignment;
use teksilo::widgets::{HStack, TextWidget};

use crate::teksilo_ui::style;

/// The logo's text: the brand as the Iced UI writes it.
pub const LOGO_TEXT: &str = "AuDoxiDY";

/// The two-tone runs of the Audio Center logo (`audio_center.rs:1817-1838`):
/// each run's text and whether it is in the mark (accent) colour.
pub const TWO_TONE_RUNS: [(&str, bool); 5] = [
    ("A", true),
    ("uD", false),
    ("o", true),
    ("xiD", false),
    ("Y", true),
];

/// The "AuDoxiDY" logo; see the module documentation.
pub struct BrandLogo {
    size: f32,
    color: ColorProp,
    mark: Option<ColorProp>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for BrandLogo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrandLogo")
            .field("size", &self.size)
            .field("two_tone", &self.mark.is_some())
            .finish_non_exhaustive()
    }
}

impl BrandLogo {
    /// The logo at `size` px, in the primary text colour.
    pub fn new(size: f32) -> Self {
        Self {
            size,
            color: TextRole::Primary.into(),
            mark: None,
            root: None,
        }
    }

    /// The colour of the whole logo, or of its body letters when two-toned
    /// (default primary).
    pub fn color(mut self, color: impl Into<ColorProp>) -> Self {
        self.color = color.into();
        self
    }

    /// Draws "A", "o" and "Y" in `mark` (the Audio Center logo uses the
    /// accent, `TextRole::Accent`).
    pub fn two_tone(mut self, mark: impl Into<ColorProp>) -> Self {
        self.mark = Some(mark.into());
        self
    }

    fn run(&self, text: &'static str, color: ColorProp) -> TextWidget {
        TextWidget::new(lit!(text))
            .style(style::logo(self.size))
            .color(color)
            .single_line()
            .no_shrink()
    }
}

impl Widget for BrandLogo {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let content = match &self.mark {
            None => HStack::new()
                .spacing(0.0)
                .child(self.run(LOGO_TEXT, self.color.clone())),
            Some(mark) => HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Center)
                .children(TWO_TONE_RUNS.into_iter().map(|(text, is_mark)| {
                    let color = if is_mark {
                        mark.clone()
                    } else {
                        self.color.clone()
                    };
                    self.run(text, color)
                })),
        };
        let root = ctx.add(
            content
                .access_role(Role::Image)
                .access_label(lit!("Audoxidy"))
                .access_exclude_subtree(),
        );
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
mod tests;
