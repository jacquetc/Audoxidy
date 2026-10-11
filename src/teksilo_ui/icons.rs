//! `res!()` SVG factories for the icons in `assets/icons/`, grouped by where
//! they appear. Pure asset lookups: no state, no logic.
//!
//! `res!` embeds each file at compile time (the Iced UI loads them from the
//! working directory at run time) and needs a literal path per call site, so
//! there is one function per icon. The SVGs are white single-colour Material
//! icons; Teksilo tints them, so a factory sets no colour: the caller picks a
//! role, by default [`style::ICON_ROLE`](super::style::ICON_ROLE). Factories
//! set no size either: the Iced sizes differ per call site.

use teksilo::res;
use teksilo::widgets::IconWidget;

/// The window controls in the title bar (`src/gui/library.rs:1361`).
pub mod window {
    use super::{IconWidget, res};

    /// Minimize (`minimize.svg`).
    pub fn minimize() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/minimize.svg"))
    }

    /// Maximize (`maximize.svg`). The Iced UI shows it in both window states.
    pub fn maximize() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/maximize.svg"))
    }

    /// Close (`close.svg`).
    pub fn close() -> IconWidget {
        IconWidget::from_svg_icon(res!("assets/icons/close.svg"))
    }
}
