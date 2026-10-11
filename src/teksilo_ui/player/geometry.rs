//! The player's fixed geometry and the pure rules that read it, from the Iced
//! view (`src/gui/player.rs`) and its handlers.
//!
//! Everything is in the panel's own coordinates: the panel is 400 x 400 at the
//! window's top-leading corner, as the Iced player is, so Iced's window
//! coordinates and these are the same numbers.

use crate::utils::format_duration;

/// The panel's side (`player.rs:34`, `bounds`).
pub const SIZE: f32 = 400.0;
/// The top row's height (`player.rs:123`).
pub const TOP_ROW_HEIGHT: f32 = 40.0;
/// The menu button's icon size (`player.rs:125`).
pub const MENU_ICON_SIZE: f32 = 31.0;
/// The top row's horizontal padding (`player.rs:129`).
pub const TOP_ROW_PADDING: f32 = 8.0;
/// The middle band's height: the info column and the three control zones
/// (`player.rs:225`, `387`).
pub const BAND_HEIGHT: f32 = 320.0;
/// The seek bar's height (`player.rs:250`).
pub const SEEK_HEIGHT: f32 = 40.0;
/// The space above the title inside the info column (`player.rs:194`).
pub const TITLE_TOP: f32 = 262.0;
/// The artist row's height (`player.rs:212`).
pub const ARTIST_ROW_HEIGHT: f32 = 36.0;
/// The title's height: what the info column leaves after the space above and
/// the artist row. Iced asks for 40 but `center_y(Fill)` turns the height
/// into a fill (`player.rs:196-197`), so the title gets the remaining 22.
pub const TITLE_HEIGHT: f32 = BAND_HEIGHT - TITLE_TOP - ARTIST_ROW_HEIGHT;
/// The title and artist rows' horizontal padding (`player.rs:198`, `214`).
pub const INFO_PADDING: f32 = 10.0;
/// The text cells' horizontal padding (`player.rs:150`, `169`, `210`).
pub const TEXT_PADDING: f32 = 2.0;
/// The title's font size (`player.rs:144`).
pub const TITLE_SIZE: f32 = 18.0;
/// The artist's font size (`player.rs:173`).
pub const ARTIST_SIZE: f32 = 15.0;
/// The elapsed time's font size (`player.rs:207`).
pub const ELAPSED_SIZE: f32 = 14.0;
/// A title or artist is announced by a tooltip when it is longer than this
/// many **bytes** (`player.rs:156`, `185`).
pub const TOOLTIP_MIN_BYTES: usize = 35;
/// The tooltip text's size (`player.rs:159`).
pub const TOOLTIP_SIZE: f32 = 13.0;
/// The seek bar's horizontal padding (`player.rs:271`).
pub const SEEK_PADDING_X: f32 = 13.0;
/// The seek rail's thickness (`player.rs:236`).
pub const SEEK_RAIL: f32 = 5.0;
/// The rail's corner radius: Iced's default slider style (`iced_widget`
/// `slider::default`, 2 px).
pub const SEEK_RAIL_RADIUS: f32 = 2.0;
/// The black overlay's opacity (`player.rs:99`).
pub const BLACKOUT_ALPHA: f32 = 0.30;
/// The seek bar's background opacity (`player.rs:285`).
pub const SEEK_BAR_ALPHA: f32 = 0.25;
/// The opacity of `COLOR_CONTRAST` in the zone circles, the volume bubble and
/// the flat rail (`player.rs:230`, `318`, `371`).
pub const CONTRAST_ALPHA: f32 = 0.5;
/// The seek tooltip's text size (`player.rs:628`).
pub const SEEK_TIP_SIZE: f32 = 12.0;
/// The seek tooltip's padding (`player.rs:629`).
pub const SEEK_TIP_PADDING: f32 = 4.0;

/// The x where the second zone starts: a third of the width, as Iced writes
/// it (`player.rs:392`).
pub const ZONE_PLAY_FROM: f32 = 133.3;
/// The x where the third zone starts (`player.rs:394`).
pub const ZONE_NEXT_FROM: f32 = 266.6;

/// The side of the round hover feedback in a control zone: the icon plus its
/// padding on both sides, 42 + 3 + 3 and 34 + 7 + 7 (`player.rs:299-312`).
pub const ZONE_BUBBLE: f32 = 48.0;
/// The size of the previous and next icons (`player.rs:292`).
pub const ZONE_ICON: f32 = 42.0;
/// The size of the pause icon (`player.rs:291`).
pub const ZONE_PAUSE_ICON: f32 = 34.0;
/// The volume bubble's icon size (`player.rs:411`).
pub const VOLUME_ICON: f32 = 44.0;
/// The volume bubble's text size (`player.rs:414`).
pub const VOLUME_TEXT: f32 = 20.0;

/// The one of the three control zones the pointer is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverZone {
    /// Outside the zones, or the controls are not showing feedback.
    None,
    /// The left third: Previous.
    Previous,
    /// The middle third: Play / Pause.
    PlayPause,
    /// The right third: Next.
    Next,
}

impl HoverZone {
    /// The zone at `(x, y)` when the controls show their feedback: inside the
    /// middle band (`40 <= y <= 360`, `x <= 400`) and split at Iced's
    /// thresholds (`player.rs:325-342`).
    pub fn at(x: f32, y: f32) -> Self {
        let inside = x <= SIZE && (TOP_ROW_HEIGHT..=TOP_ROW_HEIGHT + BAND_HEIGHT).contains(&y);
        if !inside {
            Self::None
        } else if x < ZONE_PLAY_FROM {
            Self::Previous
        } else if x < ZONE_NEXT_FROM {
            Self::PlayPause
        } else {
            Self::Next
        }
    }
}

/// Which speaker icon the volume bubble shows for `percent`: muted at 0, low
/// under 50, loud from 50 (`player.rs:397-403`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeIcon {
    /// `volume-off-straight-fill.svg`.
    Off,
    /// `volume-down-straight-fill.svg`.
    Down,
    /// `volume-up-straight-fill.svg`.
    Up,
}

impl VolumeIcon {
    /// The icon for a volume in percent.
    pub fn for_percent(percent: f32) -> Self {
        if percent == 0.0 {
            Self::Off
        } else if percent < 50.0 {
            Self::Down
        } else {
            Self::Up
        }
    }
}

/// The volume bubble's text: the percentage with no decimals (`{:.0}`).
pub fn volume_text(percent: f32) -> String {
    format!("{percent:.0}")
}

/// Whether `text` gets a tooltip: longer than [`TOOLTIP_MIN_BYTES`] bytes (not
/// characters, as in Iced).
pub fn wants_tooltip(text: &str) -> bool {
    text.len() > TOOLTIP_MIN_BYTES
}

/// The time tooltip over the seek bar: where it is and what it says.
#[derive(Debug, Clone, PartialEq)]
pub struct SeekHover {
    /// The tooltip's left edge (`pos.x - 15`, `player.rs:640`).
    pub left: f32,
    /// The tooltip's top edge (`pos.y - 30`, `player.rs:639`).
    pub top: f32,
    /// The time under the pointer.
    pub text: String,
}

/// The seek tooltip for a pointer at `(x, y)` over a track of `duration`
/// seconds, or `None` when Iced shows none: the pointer must be in the seek
/// bar (`360 <= y <= 400`, `13 <= x <= 387`) and the duration known
/// (`player.rs:608-644`). The fraction is taken over 376 px, as Iced does.
pub fn seek_hover(x: f32, y: f32, duration: f64) -> Option<SeekHover> {
    let in_bar = (TOP_ROW_HEIGHT + BAND_HEIGHT..=SIZE).contains(&y)
        && (SEEK_PADDING_X..=SIZE - SEEK_PADDING_X).contains(&x);
    if !in_bar || duration <= 0.0 {
        return None;
    }
    let fraction = ((x - SEEK_PADDING_X) / 376.0).clamp(0.0, 1.0);
    Some(SeekHover {
        left: x - 15.0,
        top: y - 30.0,
        text: format_duration(f64::from(fraction) * duration),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_gets_what_the_info_column_leaves() {
        assert_eq!(TITLE_HEIGHT, 22.0);
        assert_eq!(TOP_ROW_HEIGHT + BAND_HEIGHT + SEEK_HEIGHT, SIZE);
    }

    #[test]
    fn the_zones_split_the_band_at_iceds_thresholds() {
        assert_eq!(HoverZone::at(0.0, 40.0), HoverZone::Previous);
        assert_eq!(HoverZone::at(133.2, 200.0), HoverZone::Previous);
        assert_eq!(HoverZone::at(133.3, 200.0), HoverZone::PlayPause);
        assert_eq!(HoverZone::at(266.5, 200.0), HoverZone::PlayPause);
        assert_eq!(HoverZone::at(266.6, 200.0), HoverZone::Next);
        assert_eq!(HoverZone::at(400.0, 360.0), HoverZone::Next);
    }

    #[test]
    fn the_top_row_and_the_seek_bar_belong_to_no_zone() {
        assert_eq!(HoverZone::at(200.0, 39.9), HoverZone::None);
        assert_eq!(HoverZone::at(200.0, 360.1), HoverZone::None);
        assert_eq!(HoverZone::at(400.1, 200.0), HoverZone::None);
    }

    #[test]
    fn the_volume_icon_follows_the_percentage() {
        assert_eq!(VolumeIcon::for_percent(0.0), VolumeIcon::Off);
        assert_eq!(VolumeIcon::for_percent(5.0), VolumeIcon::Down);
        assert_eq!(VolumeIcon::for_percent(49.9), VolumeIcon::Down);
        assert_eq!(VolumeIcon::for_percent(50.0), VolumeIcon::Up);
        assert_eq!(VolumeIcon::for_percent(100.0), VolumeIcon::Up);
    }

    #[test]
    fn the_volume_text_has_no_decimals() {
        assert_eq!(volume_text(55.0), "55");
        assert_eq!(volume_text(0.0), "0");
        assert_eq!(volume_text(99.6), "100");
    }

    #[test]
    fn a_tooltip_is_for_more_than_35_bytes_not_characters() {
        assert!(!wants_tooltip(&"a".repeat(35)));
        assert!(wants_tooltip(&"a".repeat(36)));
        // 18 two-byte characters are 36 bytes: over the limit though short.
        assert!(wants_tooltip(&"é".repeat(18)));
        assert!(!wants_tooltip(&"é".repeat(17)));
    }

    #[test]
    fn the_seek_tooltip_names_the_time_under_the_pointer() {
        let hover = seek_hover(13.0 + 188.0, 380.0, 200.0).unwrap();
        assert_eq!(hover.text, "01:40");
        assert_eq!((hover.left, hover.top), (186.0, 350.0));
    }

    #[test]
    fn the_seek_tooltip_needs_the_bar_and_a_known_duration() {
        assert!(seek_hover(200.0, 359.0, 200.0).is_none());
        assert!(seek_hover(12.0, 380.0, 200.0).is_none());
        assert!(seek_hover(388.0, 380.0, 200.0).is_none());
        assert!(seek_hover(200.0, 380.0, 0.0).is_none());
        assert!(seek_hover(200.0, 400.0, 200.0).is_some());
    }

    #[test]
    fn the_seek_fraction_clamps_over_376_px() {
        assert_eq!(seek_hover(13.0, 380.0, 100.0).unwrap().text, "00:00");
        assert_eq!(seek_hover(387.0, 380.0, 100.0).unwrap().text, "01:39");
    }
}
