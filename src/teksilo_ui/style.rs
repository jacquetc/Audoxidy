//! The Audoxidy look: the dark theme, the brand colours and the text styles.
//!
//! The Iced UI paints with seven colours (`src/gui/theme.rs`). Here they are
//! mapped onto Teksilo's colour tokens, over the IntUI dark preset, so widget
//! code names a **role** (`SurfaceRole::Raised`, `TextRole::Secondary`) and
//! never a colour:
//!
//! | Iced                  | Value     | Token                                   |
//! | --------------------- | --------- | --------------------------------------- |
//! | `COLOR_BG`            | `#0F0F0F` | `surface_main`, `surface_content`       |
//! | `COLOR_CONTRAST`      | `#1A1A1A` | `surface_raised` (bars, menus, fields)  |
//! | `COLOR_TEXT_PRIMARY`  | `#AFAFAF` | `text_primary`                          |
//! | `COLOR_TEXT_SECONDARY`| `#5B5B5B` | `text_secondary`                        |
//! | `COLOR_ACCENT`        | `#E50039` | `accent` (and the error colours: Iced's palette uses it as `danger`) |
//! | `COLOR_SUCCESS`       | `#10B981` | `text_success`, `status_success_fg`     |
//! | `COLOR_WARNING`       | `#FBBF24` | `text_warning`, `status_warning_fg`     |
//!
//! Icons: the SVGs in `assets/icons/` are white; the Iced UI shows them in the
//! secondary text colour unless a view styles them (see `assets/screenshot.png`),
//! so an icon's default role is [`ICON_ROLE`].
//!
//! Text: the Iced UI sets every size explicitly, in Inter (regular or medium);
//! the logo is set in Stage Wander. [`inter`] and [`logo`] build those styles.

use teksilo::core::styles::Theme;
use teksilo::prelude::{TextRole, intui};
use teksilo::tokens::{Color, ColorTokens, FontWeight, TextStyle, TypographyTokens};

/// Builds a colour from 8-bit sRGB components.
const fn rgb8(r: u8, g: u8, b: u8) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

/// `COLOR_ACCENT`: the brand red.
pub const ACCENT: Color = rgb8(0xE5, 0x00, 0x39);
/// `COLOR_BG`: the window and panel background.
pub const BACKGROUND: Color = rgb8(0x0F, 0x0F, 0x0F);
/// `COLOR_CONTRAST`: bars, menus, dialogs and input fields.
pub const CONTRAST: Color = rgb8(0x1A, 0x1A, 0x1A);
/// `COLOR_TEXT_PRIMARY`.
pub const TEXT_PRIMARY: Color = rgb8(0xAF, 0xAF, 0xAF);
/// `COLOR_TEXT_SECONDARY`.
pub const TEXT_SECONDARY: Color = rgb8(0x5B, 0x5B, 0x5B);
/// `COLOR_SUCCESS`.
pub const SUCCESS: Color = rgb8(0x10, 0xB9, 0x81);
/// `COLOR_WARNING`.
pub const WARNING: Color = rgb8(0xFB, 0xBF, 0x24);

/// The role an icon is drawn in when its Iced view gives it no colour.
pub const ICON_ROLE: TextRole = TextRole::Secondary;

/// The family name of the logo face (`assets/fonts/Stage-Wanders.ttf`).
pub const LOGO_FAMILY: &str = "Stage Wander";
/// The family name of the UI face.
pub const UI_FAMILY: &str = "Inter";

/// The brand colours of the library source pills (`src/gui/library.rs:1316`).
pub mod source_colors {
    use super::{ACCENT, Color, rgb8};

    /// The local library ("Biblioteca").
    pub const LOCAL: Color = ACCENT;
    /// Spotify.
    pub const SPOTIFY: Color = rgb8(29, 185, 84);
    /// YouTube.
    pub const YOUTUBE: Color = rgb8(221, 36, 37);
    /// Qobuz.
    pub const QOBUZ: Color = rgb8(48, 48, 48);
    /// Deezer.
    pub const DEEZER: Color = rgb8(162, 56, 255);
    /// Tidal.
    pub const TIDAL: Color = rgb8(68, 68, 68);
}

/// Inter at `size` px and `weight`: the Iced UI's `FONT_INTER_SANS_NORMAL`
/// (`FontWeight::REGULAR`) and `FONT_INTER_SANS_MEDIUM` (`FontWeight::MEDIUM`).
pub fn inter(size: f32, weight: FontWeight) -> TextStyle {
    TextStyle {
        family: UI_FAMILY.to_string(),
        size,
        weight,
        ..TextStyle::default()
    }
}

/// The logo face (Stage Wander) at `size` px.
pub fn logo(size: f32) -> TextStyle {
    TextStyle {
        family: LOGO_FAMILY.to_string(),
        size,
        weight: FontWeight::REGULAR,
        ..TextStyle::default()
    }
}

/// The Audoxidy colour tokens, over IntUI dark.
pub fn colors() -> ColorTokens {
    let base = ColorTokens::dark_default();
    ColorTokens {
        surface_main: BACKGROUND,
        surface_content: BACKGROUND,
        surface_raised: CONTRAST,
        surface_sunken: BACKGROUND,
        surface_container: CONTRAST,
        surface_container_raised: CONTRAST,
        surface_container_sunken: BACKGROUND,
        text_primary: TEXT_PRIMARY,
        text_secondary: TEXT_SECONDARY,
        text_disabled: TEXT_SECONDARY,
        text_on_accent: Color::WHITE,
        text_error: ACCENT,
        text_warning: WARNING,
        text_success: SUCCESS,
        accent: ACCENT,
        accent_hover: ACCENT.lighten(0.08),
        accent_pressed: ACCENT.darken(0.08),
        accent_subtle_bg: ACCENT.with_alpha(0.15),
        border_focused: ACCENT,
        border_error: ACCENT,
        status_success_fg: SUCCESS,
        status_warning_fg: WARNING,
        status_error_fg: ACCENT,
        selection_bg_active: ACCENT.with_alpha(0.35),
        focus_ring: ACCENT,
        focus_ring_error: ACCENT,
        tooltip_bg: CONTRAST,
        tooltip_text: TEXT_PRIMARY,
        ..base
    }
}

/// The Audoxidy theme: IntUI dark with the Audoxidy colours, and Inter as the
/// UI face of every typography role.
pub fn audoxidy_dark() -> Theme {
    let base = intui::dark();
    let typography = with_family(&base.typography, UI_FAMILY);
    Theme {
        colors: colors(),
        typography,
        ..base
    }
    .with_id("audoxidy.dark")
}

/// `typography` with every proportional role set in `family` (the monospace
/// role is left alone).
fn with_family(typography: &TypographyTokens, family: &str) -> TypographyTokens {
    let set = |style: &TextStyle| TextStyle {
        family: family.to_string(),
        ..style.clone()
    };
    TypographyTokens {
        body: set(&typography.body),
        body_bold: set(&typography.body_bold),
        small: set(&typography.small),
        small_bold: set(&typography.small_bold),
        tiny: set(&typography.tiny),
        mono: typography.mono.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(color: Color) -> String {
        color.to_hex_upper(false)
    }

    #[test]
    fn the_palette_is_the_iced_palette() {
        assert_eq!(hex(ACCENT), hex(Color::from_hex("#E50039")));
        assert_eq!(hex(BACKGROUND), hex(Color::from_hex("#0F0F0F")));
        assert_eq!(hex(CONTRAST), hex(Color::from_hex("#1A1A1A")));
        assert_eq!(hex(TEXT_PRIMARY), hex(Color::from_hex("#AFAFAF")));
        assert_eq!(hex(TEXT_SECONDARY), hex(Color::from_hex("#5B5B5B")));
        assert_eq!(hex(SUCCESS), hex(Color::from_hex("#10B981")));
        assert_eq!(hex(WARNING), hex(Color::from_hex("#FBBF24")));
    }

    #[test]
    fn the_theme_maps_the_roles_widgets_use() {
        let theme = audoxidy_dark();
        assert!(theme.is_dark());
        assert_eq!(hex(theme.colors.surface_main), hex(BACKGROUND));
        assert_eq!(hex(theme.colors.surface_raised), hex(CONTRAST));
        assert_eq!(hex(theme.colors.text_primary), hex(TEXT_PRIMARY));
        assert_eq!(hex(theme.colors.text_secondary), hex(TEXT_SECONDARY));
        assert_eq!(hex(theme.colors.accent), hex(ACCENT));
        assert_eq!(theme.id.as_str(), "audoxidy.dark");
    }

    #[test]
    fn every_proportional_role_is_set_in_inter() {
        let theme = audoxidy_dark();
        let t = &theme.typography;
        for style in [&t.body, &t.body_bold, &t.small, &t.small_bold, &t.tiny] {
            assert_eq!(style.family, UI_FAMILY);
        }
        assert_ne!(t.mono.family, UI_FAMILY);
    }

    #[test]
    fn the_text_helpers_carry_family_size_and_weight() {
        let pill = inter(15.0, FontWeight::MEDIUM);
        assert_eq!(pill.family, UI_FAMILY);
        assert_eq!(pill.size, 15.0);
        assert_eq!(pill.weight, FontWeight::MEDIUM);
        let mark = logo(19.0);
        assert_eq!(mark.family, LOGO_FAMILY);
        assert_eq!(mark.size, 19.0);
    }
}
