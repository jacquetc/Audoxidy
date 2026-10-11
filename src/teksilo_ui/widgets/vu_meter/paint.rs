//! What the VU meter paints: Iced's `VuMeterWidget::draw`
//! (`src/gui/widgets.rs:4737-5171`), as a list of [`MeterPaint`] items in
//! Iced's drawing order, so the geometry can be tested without a renderer.
//!
//! The layout, at the meter's top-left corner, 400 px wide:
//!
//! - the clip LED, 8 × 8 at (2, 1), red when clipping;
//! - the bar labels ("L" / "R", or "M" / "S"), 8 px;
//! - two 6 px bars from x = 21, at y = 1 and y = 9, filled up to the level
//!   in the colour of its zone (green to -9 dB, yellow to -3, orange to 0,
//!   red above), each with a 2 px peak-hold marker;
//! - the numeric readout ("Pk:-12.0 Rms:-18.0") right of the bars, when shown;
//!   the bars leave 115 px for it whenever it or the M/S mode is on;
//! - the phase-correlation gauge, 3 px at y = 17, filled from its centre
//!   towards the correlation, with its value;
//! - the crest factor ("Crest: 12.0 dB") at y = 24 with a small bar;
//! - the dB scale at y = 33: 1 × 4 px ticks and 7 px labels.
//!
//! Sizes, colours and positions are Iced's, with two differences (quirk
//! fix 34), both pinned by the tests below:
//!
//! - **Every label is drawn whole**, centred in the box Iced gives it: the
//!   bar labels in their 10 × 6 box beside the bar, the numeric readout and
//!   the correlation value on the middle of their row, each scale label on
//!   its tick. Iced anchors these on the box's top or left edge and clips them
//!   to the box, so only part of each shows.
//! - **The compact 24 px meter draws only what fits in it**: the crest row and
//!   the scale, which Iced draws below it at y = 24 and y = 33, are left out;
//!   the 48 px meter shows them.

use teksilo::canvas::{Point, Rect};
use teksilo::tokens::Color;

use super::VuMeterFrame;
use crate::logic::meter::METER_MIN_DB;
use crate::teksilo_ui::widgets::eq_slider::chrome::{AnchorX, AnchorY, IcedText, Tone};

/// The meter's width (`widgets.rs:4713`).
pub const WIDTH: f32 = 400.0;
/// The compact height: bars and gauge (`widgets.rs:4548`).
pub const HEIGHT: f32 = 24.0;
/// The height with the numeric readout, correlation or crest factor
/// (`widgets.rs:4550`).
pub const HEIGHT_TALL: f32 = 48.0;
/// The top of the scale (`widgets.rs:4545`).
pub const MAX_DB: f32 = 6.0;
/// The span of the scale, -60…+6 dB (`widgets.rs:4546`).
pub const TOTAL_RANGE: f32 = 66.0;
/// A bar's height (`widgets.rs:4551`).
pub const BAR_HEIGHT: f32 = 6.0;
/// The space between the two bars (`widgets.rs:4552`).
pub const BAR_GAP: f32 = 2.0;
/// The clip LED's side (`widgets.rs:4554`).
pub const LED_SIZE: f32 = 8.0;
/// The width of the numeric readout (`widgets.rs:4557`).
pub const NUMERIC_WIDTH: f32 = 110.0;
/// The scale's ticks (`widgets.rs:4566-4578`).
pub const TICKS: [f32; 11] = [
    -60.0, -40.0, -20.0, -15.0, -12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0,
];

/// The level zones' colours (`widgets.rs:4560-4563`). They are fixed signal
/// colours of the meter, not part of the Audoxidy palette, so they are not
/// theme roles.
pub mod zone {
    use super::Color;

    /// Up to -9 dB.
    pub const GREEN: Color = Color::new(
        0x4C as f32 / 255.0,
        0xAF as f32 / 255.0,
        0x50 as f32 / 255.0,
        1.0,
    );
    /// Up to -3 dB.
    pub const YELLOW: Color = Color::new(1.0, 0xC1 as f32 / 255.0, 0x07 as f32 / 255.0, 1.0);
    /// Up to 0 dB.
    pub const ORANGE: Color = Color::new(1.0, 0x98 as f32 / 255.0, 0.0, 1.0);
    /// Above 0 dB.
    pub const RED: Color = Color::new(
        0xF4 as f32 / 255.0,
        0x43 as f32 / 255.0,
        0x36 as f32 / 255.0,
        1.0,
    );
}

/// The words the meter draws, in the user's language (Iced draws them in
/// English in both locales).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterWords {
    /// "L".
    pub left: String,
    /// "R".
    pub right: String,
    /// "M".
    pub mid: String,
    /// "S".
    pub side: String,
    /// "Pk".
    pub peak: String,
    /// "Rms".
    pub rms: String,
    /// "Crest".
    pub crest: String,
}

impl MeterWords {
    /// The Iced words.
    pub fn english() -> Self {
        Self {
            left: "L".into(),
            right: "R".into(),
            mid: "M".into(),
            side: "S".into(),
            peak: "Pk".into(),
            rms: "Rms".into(),
            crest: "Crest".into(),
        }
    }
}

/// One thing the meter draws.
#[derive(Debug, Clone, PartialEq)]
pub enum MeterPaint {
    /// A filled rectangle.
    Fill {
        /// Where.
        rect: Rect,
        /// In what colour.
        tone: Tone,
    },
    /// The clip LED: a rectangle with radius 1 and a 1 px border.
    Led {
        /// Where.
        rect: Rect,
        /// The fill colour.
        fill: Tone,
        /// The border colour.
        border: Tone,
    },
    /// A line of text.
    Text(IcedText),
}

/// The x of `db` on a bar starting at `bar_left`, `bar_width` long, rounded
/// to the pixel (`widgets.rs:4644-4648`).
pub fn db_to_x(db: f32, bar_left: f32, bar_width: f32) -> f32 {
    let clamped = db.clamp(METER_MIN_DB, MAX_DB);
    (bar_left + (clamped - METER_MIN_DB) / TOTAL_RANGE * bar_width).round()
}

/// The x of a correlation (-1…+1) on the gauge (`widgets.rs:4673-4676`).
pub fn corr_to_x(corr: f32, left: f32, width: f32) -> f32 {
    (left + (corr + 1.0) / 2.0 * width).round()
}

/// The colour of a level (`widgets.rs:4679-4689`).
pub fn zone_color(db: f32) -> Color {
    if db <= -9.0 {
        zone::GREEN
    } else if db <= -3.0 {
        zone::YELLOW
    } else if db <= 0.0 {
        zone::ORANGE
    } else {
        zone::RED
    }
}

/// A reading for the numeric readout: "-inf" at the floor, else one decimal
/// with its sign (`widgets.rs:4664-4670`).
pub fn format_db(db: f32) -> String {
    if db <= METER_MIN_DB {
        "-inf".to_string()
    } else {
        format!("{db:+.1}")
    }
}

/// The label of a scale tick (`widgets.rs:4692-4707`).
pub fn tick_label(db: f32) -> String {
    let whole = db as i32;
    if whole > 0 {
        format!("+{whole}")
    } else {
        whole.to_string()
    }
}

fn text(
    content: String,
    size: f32,
    anchor: Point,
    x: AnchorX,
    y: AnchorY,
    tone: Tone,
) -> MeterPaint {
    MeterPaint::Text(IcedText {
        content,
        size,
        anchor,
        anchor_x: x,
        anchor_y: y,
        tone,
    })
}

/// Everything the meter draws for `frame` at `bounds`, in Iced's order. A
/// row that does not fit in `bounds` (the crest factor and the scale of a
/// 24 px meter) is left out.
pub fn meter_paint(frame: &VuMeterFrame, bounds: Rect, words: &MeterWords) -> Vec<MeterPaint> {
    let (bx, by, bw) = (bounds.x, bounds.y, bounds.width);
    let bottom = by + bounds.height;
    let mut out = vec![MeterPaint::Fill {
        rect: bounds,
        tone: Tone::Background,
    }];

    let left_margin = LED_SIZE + 3.0 + 10.0;
    let right_margin = if frame.show_numeric || frame.mid_side {
        NUMERIC_WIDTH + 5.0
    } else {
        5.0
    };
    let bar_left = (bx + left_margin).round();
    let bar_width = (bw - left_margin - right_margin).max(1.0);
    let bar_y = [
        (by + 1.0).round(),
        ((by + 1.0).round() + BAR_HEIGHT + BAR_GAP).round(),
    ];
    let gauge_y = (bar_y[1] + BAR_HEIGHT + 2.0).round();
    let gauge_h = 3.0;
    let gauge_area_h = 6.0;
    let crest_y = (gauge_y + gauge_area_h + 1.0).round();
    let crest_h = 8.0;
    let scale_y = (crest_y + crest_h + 1.0).round();

    // The bars (`widgets.rs:4795-4831`).
    for (level, y) in frame.level_db.iter().zip(bar_y) {
        if *level > METER_MIN_DB {
            let width = (db_to_x(*level, bar_left, bar_width) - bar_left).max(0.0);
            if width > 0.0 {
                out.push(MeterPaint::Fill {
                    rect: Rect::new(bar_left, y, width, BAR_HEIGHT),
                    tone: Tone::Fixed(zone_color(*level)),
                });
            }
        }
    }
    // The peak-hold markers (`widgets.rs:4833-4861`).
    for (hold, y) in frame.hold_db.iter().zip(bar_y) {
        if *hold > METER_MIN_DB {
            out.push(MeterPaint::Fill {
                rect: Rect::new(db_to_x(*hold, bar_left, bar_width), y, 2.0, BAR_HEIGHT),
                tone: Tone::Fixed(zone_color(*hold)),
            });
        }
    }
    // The clip LED (`widgets.rs:4863-4890`).
    let (led_fill, led_border) = if frame.clipping {
        (Tone::Fixed(zone::RED), Tone::Fixed(zone::RED))
    } else {
        (Tone::TextSecondaryAlpha(0.3), Tone::TextSecondaryAlpha(0.5))
    };
    out.push(MeterPaint::Led {
        rect: Rect::new((bx + 2.0).round(), (by + 1.0).round(), LED_SIZE, LED_SIZE),
        fill: led_fill,
        border: led_border,
    });
    // The bar labels (`widgets.rs:4892-4934`), centred in their 10 × 6 box.
    // Quirk fix 34: Iced centres them on the box's top-left corner and clips them to the box (widgets.rs:4897-4934).
    let label_x = (bx + LED_SIZE + 3.0).round();
    let names = if frame.mid_side {
        [&words.mid, &words.side]
    } else {
        [&words.left, &words.right]
    };
    for (name, y) in names.into_iter().zip(bar_y) {
        out.push(text(
            name.clone(),
            8.0,
            Point::new(label_x + 5.0, y + BAR_HEIGHT / 2.0),
            AnchorX::Center,
            AnchorY::Center,
            Tone::TextSecondary,
        ));
    }
    // The numeric readout (`widgets.rs:4936-4996`), on the middle of each bar's
    // row.
    // Quirk fix 34: Iced centres it on the row's top edge and clips it to the row (widgets.rs:4950-4995).
    if frame.show_numeric {
        let num_x = (bar_left + bar_width + 4.0).round();
        let prefixes = if frame.mid_side {
            [format!("{}: ", words.mid), format!("{}: ", words.side)]
        } else {
            [String::new(), String::new()]
        };
        for (channel, (prefix, y)) in prefixes.into_iter().zip(bar_y).enumerate() {
            out.push(text(
                format!(
                    "{prefix}{}:{} {}:{}",
                    words.peak,
                    format_db(frame.peak_db[channel]),
                    words.rms,
                    format_db(frame.level_db[channel]),
                ),
                7.0,
                Point::new(num_x, y + BAR_HEIGHT / 2.0),
                AnchorX::Left,
                AnchorY::Center,
                Tone::TextSecondary,
            ));
        }
    }
    // The phase-correlation gauge (`widgets.rs:4998-5082`).
    let corr = frame.correlation.clamp(-1.0, 1.0);
    out.push(MeterPaint::Fill {
        rect: Rect::new(bar_left, gauge_y, bar_width, gauge_h),
        tone: Tone::Contrast,
    });
    let center_x = corr_to_x(0.0, bar_left, bar_width);
    let corr_x = corr_to_x(corr, bar_left, bar_width);
    let (fill_left, fill_width) = if corr >= 0.0 {
        (center_x, (corr_x - center_x).max(0.0))
    } else {
        (corr_x, (center_x - corr_x).max(0.0))
    };
    if fill_width > 0.0 {
        let tone = if corr > 0.0 { zone::GREEN } else { zone::RED };
        out.push(MeterPaint::Fill {
            rect: Rect::new(fill_left, gauge_y, fill_width, gauge_h),
            tone: Tone::Fixed(tone),
        });
    }
    out.push(MeterPaint::Fill {
        rect: Rect::new(center_x - 0.5, gauge_y, 1.0, gauge_h),
        tone: Tone::TextSecondary,
    });
    let corr_tone = if corr > 0.0 {
        Tone::Fixed(zone::GREEN)
    } else if corr < 0.0 {
        Tone::Fixed(zone::RED)
    } else {
        Tone::TextSecondary
    };
    // The correlation value, on the middle of its 6 px row.
    // Quirk fix 34: Iced centres it on the row's top edge and clips it to the row (widgets.rs:5064-5082).
    out.push(text(
        format!("{corr:.2}"),
        7.0,
        Point::new(bar_left, gauge_y - 1.0 + gauge_area_h / 2.0),
        AnchorX::Left,
        AnchorY::Center,
        corr_tone,
    ));
    // The crest factor of the more dynamic channel (`widgets.rs:5084-5129`),
    // when its row fits.
    // Quirk fix 34: Iced draws this row below a 24 px meter, outside its bounds (widgets.rs:5084-5129).
    if crest_y + crest_h <= bottom {
        paint_crest(&mut out, frame, words, bar_left, crest_y);
    }
    // The scale (`widgets.rs:5131-5170`), when it fits, each label centred on
    // its tick.
    // Quirk fix 34: Iced draws the scale below a 24 px meter, and centres each label 10 px left of its tick, clipped from there (widgets.rs:5131-5170).
    let label_top = scale_y + 4.0;
    if label_top + 7.0 <= bottom {
        for db in TICKS {
            let tx = db_to_x(db, bar_left, bar_width);
            out.push(MeterPaint::Fill {
                rect: Rect::new(tx, scale_y, 1.0, 4.0),
                tone: Tone::TextSecondary,
            });
            out.push(text(
                tick_label(db),
                7.0,
                Point::new(tx, label_top),
                AnchorX::Center,
                AnchorY::Top,
                Tone::TextSecondary,
            ));
        }
    }
    out
}

/// The crest-factor text and its small bar (`widgets.rs:5084-5129`).
fn paint_crest(
    out: &mut Vec<MeterPaint>,
    frame: &VuMeterFrame,
    words: &MeterWords,
    bar_left: f32,
    crest_y: f32,
) {
    let crest = frame.crest_db[0].max(frame.crest_db[1]);
    out.push(text(
        format!("{}: {crest:.1} dB", words.crest),
        7.0,
        Point::new(bar_left, crest_y),
        AnchorX::Left,
        AnchorY::Top,
        Tone::TextSecondary,
    ));
    let crest_bar_w = (crest / 20.0 * 60.0).clamp(0.0, 60.0);
    if crest_bar_w > 0.0 {
        let tone = if crest > 10.0 {
            zone::GREEN
        } else if crest > 3.0 {
            zone::YELLOW
        } else {
            zone::RED
        };
        out.push(MeterPaint::Fill {
            rect: Rect::new(bar_left, crest_y + 7.0, crest_bar_w, 2.0),
            tone: Tone::Fixed(tone),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rect {
        Rect::new(0.0, 0.0, WIDTH, HEIGHT_TALL)
    }

    fn fills(paint: &[MeterPaint]) -> Vec<(Rect, Tone)> {
        paint
            .iter()
            .filter_map(|p| match p {
                MeterPaint::Fill { rect, tone } => Some((*rect, *tone)),
                _ => None,
            })
            .collect()
    }

    fn texts(paint: &[MeterPaint]) -> Vec<IcedText> {
        paint
            .iter()
            .filter_map(|p| match p {
                MeterPaint::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn frame() -> VuMeterFrame {
        VuMeterFrame {
            peak_db: [-6.0, -12.0],
            level_db: [-12.0, -1.5],
            hold_db: [-4.0, -60.0],
            clipping: false,
            mid_side: false,
            correlation: 0.5,
            show_numeric: true,
            crest_db: [6.0, 12.0],
        }
    }

    #[test]
    fn the_scale_maps_minus_sixty_to_plus_six_onto_the_bar() {
        assert_eq!(db_to_x(-60.0, 21.0, 264.0), 21.0);
        assert_eq!(db_to_x(6.0, 21.0, 264.0), 285.0);
        assert_eq!(db_to_x(0.0, 21.0, 264.0), 261.0);
        assert_eq!(db_to_x(-100.0, 21.0, 264.0), 21.0);
        assert_eq!(corr_to_x(0.0, 21.0, 264.0), 153.0);
        assert_eq!(corr_to_x(-1.0, 21.0, 264.0), 21.0);
    }

    #[test]
    fn levels_take_the_colour_of_their_zone() {
        assert_eq!(zone_color(-20.0), zone::GREEN);
        assert_eq!(zone_color(-9.0), zone::GREEN);
        assert_eq!(zone_color(-5.0), zone::YELLOW);
        assert_eq!(zone_color(-1.0), zone::ORANGE);
        assert_eq!(zone_color(0.0), zone::ORANGE);
        assert_eq!(zone_color(0.5), zone::RED);
        assert_eq!(
            zone::GREEN.to_hex_upper(false),
            Color::from_hex("#4CAF50").to_hex_upper(false)
        );
        assert_eq!(
            zone::YELLOW.to_hex_upper(false),
            Color::from_hex("#FFC107").to_hex_upper(false)
        );
        assert_eq!(
            zone::ORANGE.to_hex_upper(false),
            Color::from_hex("#FF9800").to_hex_upper(false)
        );
        assert_eq!(
            zone::RED.to_hex_upper(false),
            Color::from_hex("#F44336").to_hex_upper(false)
        );
    }

    #[test]
    fn readings_and_ticks_are_formatted_as_in_iced() {
        assert_eq!(format_db(-60.0), "-inf");
        assert_eq!(format_db(-12.04), "-12.0");
        assert_eq!(format_db(1.25), "+1.2");
        assert_eq!(tick_label(-60.0), "-60");
        assert_eq!(tick_label(0.0), "0");
        assert_eq!(tick_label(3.0), "+3");
    }

    #[test]
    fn the_bars_and_holds_sit_at_the_iced_rectangles() {
        let paint = meter_paint(&frame(), bounds(), &MeterWords::english());
        let fills = fills(&paint);
        assert_eq!(fills[0], (bounds(), Tone::Background));
        // With the readout, the bars run from 21 to 285: 264 px.
        let left = db_to_x(-12.0, 21.0, 264.0);
        assert_eq!(
            fills[1],
            (
                Rect::new(21.0, 1.0, left - 21.0, 6.0),
                Tone::Fixed(zone::GREEN)
            )
        );
        let right = db_to_x(-1.5, 21.0, 264.0);
        assert_eq!(
            fills[2],
            (
                Rect::new(21.0, 9.0, right - 21.0, 6.0),
                Tone::Fixed(zone::ORANGE)
            )
        );
        // One hold marker: the right channel's sits on the floor.
        let hold = db_to_x(-4.0, 21.0, 264.0);
        assert_eq!(
            fills[3],
            (Rect::new(hold, 1.0, 2.0, 6.0), Tone::Fixed(zone::YELLOW))
        );
        assert_eq!(fills[4].0, Rect::new(21.0, 17.0, 264.0, 3.0));
    }

    #[test]
    fn without_the_readout_or_mid_side_the_bars_reach_five_pixels_from_the_edge() {
        let quiet = VuMeterFrame {
            show_numeric: false,
            ..frame()
        };
        let paint = meter_paint(&quiet, bounds(), &MeterWords::english());
        let gauge = fills(&paint)
            .into_iter()
            .find(|(_, tone)| *tone == Tone::Contrast)
            .map(|(rect, _)| rect);
        assert_eq!(gauge, Some(Rect::new(21.0, 17.0, 374.0, 3.0)));
        assert!(!texts(&paint).iter().any(|t| t.content.contains("Pk")));
        let mid_side = VuMeterFrame {
            show_numeric: false,
            mid_side: true,
            ..frame()
        };
        let paint = meter_paint(&mid_side, bounds(), &MeterWords::english());
        assert!(
            fills(&paint)
                .iter()
                .any(|(rect, _)| *rect == Rect::new(21.0, 17.0, 264.0, 3.0))
        );
    }

    #[test]
    fn the_led_lights_red_only_when_clipping() {
        let led = |frame: &VuMeterFrame| {
            meter_paint(frame, bounds(), &MeterWords::english())
                .into_iter()
                .find_map(|p| match p {
                    MeterPaint::Led { rect, fill, border } => Some((rect, fill, border)),
                    _ => None,
                })
        };
        assert_eq!(
            led(&frame()),
            Some((
                Rect::new(2.0, 1.0, 8.0, 8.0),
                Tone::TextSecondaryAlpha(0.3),
                Tone::TextSecondaryAlpha(0.5)
            ))
        );
        let clipping = VuMeterFrame {
            clipping: true,
            ..frame()
        };
        assert_eq!(
            led(&clipping).map(|(_, fill, border)| (fill, border)),
            Some((Tone::Fixed(zone::RED), Tone::Fixed(zone::RED)))
        );
    }

    #[test]
    fn every_label_is_centred_in_the_box_iced_gives_it() {
        let paint = meter_paint(&frame(), bounds(), &MeterWords::english());
        let texts = texts(&paint);
        let at = |t: &IcedText| (t.anchor, t.anchor_x, t.anchor_y);
        // The bar labels, in the middle of their 10 × 6 boxes at (11, 1) and (11, 9).
        assert_eq!(texts[0].content, "L");
        assert_eq!(
            at(&texts[0]),
            (Point::new(16.0, 4.0), AnchorX::Center, AnchorY::Center)
        );
        assert_eq!(texts[1].content, "R");
        assert_eq!(texts[1].anchor, Point::new(16.0, 12.0));
        // The readout, on the middle of each bar's row.
        assert_eq!(texts[2].content, "Pk:-6.0 Rms:-12.0");
        assert_eq!(
            at(&texts[2]),
            (Point::new(289.0, 4.0), AnchorX::Left, AnchorY::Center)
        );
        assert_eq!(texts[3].content, "Pk:-12.0 Rms:-1.5");
        assert_eq!(texts[3].anchor, Point::new(289.0, 12.0));
        // The correlation, on the middle of its row at y = 16.
        assert_eq!(texts[4].content, "0.50");
        assert_eq!(texts[4].tone, Tone::Fixed(zone::GREEN));
        assert_eq!(
            at(&texts[4]),
            (Point::new(21.0, 19.0), AnchorX::Left, AnchorY::Center)
        );
        assert_eq!(texts[5].content, "Crest: 12.0 dB");
        assert_eq!(
            at(&texts[5]),
            (Point::new(21.0, 24.0), AnchorX::Left, AnchorY::Top)
        );
        // Each scale label is centred on its tick.
        assert_eq!(texts[6].content, "-60");
        assert_eq!(
            at(&texts[6]),
            (Point::new(21.0, 37.0), AnchorX::Center, AnchorY::Top)
        );
        assert_eq!(texts[16].content, "+6");
        assert_eq!(texts[16].anchor, Point::new(285.0, 37.0));
        assert_eq!(texts.len(), 6 + TICKS.len());
    }

    /// The frames whose layouts differ: readout on or off, L/R or M/S.
    fn variants() -> Vec<VuMeterFrame> {
        vec![
            frame(),
            VuMeterFrame {
                show_numeric: false,
                ..frame()
            },
            VuMeterFrame {
                mid_side: true,
                ..frame()
            },
            VuMeterFrame {
                show_numeric: false,
                mid_side: true,
                ..frame()
            },
        ]
    }

    /// A generous width for `text` in Inter Medium: 0.7 em per character
    /// (its widest glyphs here, "m" and "M", are about 0.85 em, its digits
    /// about 0.6 em). The real widths are measured at paint time.
    fn wide_estimate(text: &IcedText) -> f32 {
        text.content.chars().count() as f32 * 0.7 * text.size
    }

    /// Whether every rectangle and every text line box of `paint` lies inside
    /// `bounds`, each text's width taken generously.
    fn inside(paint: &[MeterPaint], bounds: Rect) -> bool {
        let (left, top) = (bounds.x, bounds.y);
        let (right, bottom) = (bounds.x + bounds.width, bounds.y + bounds.height);
        let rect_inside = |r: &Rect| {
            r.x >= left && r.y >= top && r.x + r.width <= right && r.y + r.height <= bottom
        };
        paint.iter().all(|item| match item {
            MeterPaint::Fill { rect, .. } | MeterPaint::Led { rect, .. } => rect_inside(rect),
            MeterPaint::Text(text) => {
                let (line_top, line_bottom) = text.line_span();
                let width = wide_estimate(text);
                let (text_left, text_right) = match text.anchor_x {
                    AnchorX::Left => (text.anchor.x, text.anchor.x + width),
                    AnchorX::Center => (text.anchor.x - width / 2.0, text.anchor.x + width / 2.0),
                };
                line_top >= top && line_bottom <= bottom && text_left >= left && text_right <= right
            }
        })
    }

    #[test]
    fn every_label_and_shape_of_the_tall_meter_lies_inside_it() {
        for frame in variants() {
            let paint = meter_paint(&frame, bounds(), &MeterWords::english());
            assert!(inside(&paint, bounds()), "{frame:?}");
        }
    }

    #[test]
    fn the_compact_meter_paints_nothing_outside_its_twenty_four_pixels() {
        let compact_bounds = Rect::new(0.0, 0.0, WIDTH, HEIGHT);
        let compact = VuMeterFrame {
            show_numeric: false,
            correlation: 0.0,
            crest_db: [0.0, 0.0],
            ..frame()
        };
        assert!(!compact.is_tall());
        for frame in [
            compact.clone(),
            VuMeterFrame {
                mid_side: true,
                ..compact
            },
        ] {
            let paint = meter_paint(&frame, compact_bounds, &MeterWords::english());
            assert!(inside(&paint, compact_bounds), "{frame:?}");
            // No crest row and no scale: the 48 px meter is the one that shows them.
            assert!(!texts(&paint).iter().any(|t| t.content.starts_with("Crest")));
            assert!(!fills(&paint).iter().any(|(rect, _)| rect.y >= HEIGHT));
            // The bars, the LED, the labels and the gauge are all there.
            assert_eq!(texts(&paint).len(), 3);
            assert!(paint.iter().any(|p| matches!(p, MeterPaint::Led { .. })));
        }
    }

    #[test]
    fn mid_side_relabels_the_bars_and_prefixes_the_readout() {
        let mid_side = VuMeterFrame {
            mid_side: true,
            ..frame()
        };
        let texts = texts(&meter_paint(&mid_side, bounds(), &MeterWords::english()));
        assert_eq!(texts[0].content, "M");
        assert_eq!(texts[1].content, "S");
        assert_eq!(texts[2].content, "M: Pk:-6.0 Rms:-12.0");
        assert_eq!(texts[3].content, "S: Pk:-12.0 Rms:-1.5");
    }

    #[test]
    fn the_gauge_fills_from_its_centre_towards_the_correlation() {
        let anti = VuMeterFrame {
            correlation: -0.5,
            ..frame()
        };
        let fills = fills(&meter_paint(&anti, bounds(), &MeterWords::english()));
        let gauge = fills
            .iter()
            .find(|(_, tone)| *tone == Tone::Fixed(zone::RED));
        assert_eq!(
            gauge.map(|(rect, _)| *rect),
            Some(Rect::new(87.0, 17.0, 66.0, 3.0))
        );
        assert!(fills.contains(&(Rect::new(152.5, 17.0, 1.0, 3.0), Tone::TextSecondary)));
    }

    #[test]
    fn the_crest_bar_is_sixty_pixels_at_twenty_db() {
        let loud = VuMeterFrame {
            crest_db: [25.0, 2.0],
            ..frame()
        };
        let fills = fills(&meter_paint(&loud, bounds(), &MeterWords::english()));
        assert!(fills.contains(&(Rect::new(21.0, 31.0, 60.0, 2.0), Tone::Fixed(zone::GREEN))));
        let flat = VuMeterFrame {
            crest_db: [0.0, 0.0],
            ..frame()
        };
        let fills = fills_of(&flat);
        assert!(!fills.iter().any(|(rect, _)| rect.y == 31.0));
    }

    fn fills_of(frame: &VuMeterFrame) -> Vec<(Rect, Tone)> {
        fills(&meter_paint(frame, bounds(), &MeterWords::english()))
    }

    #[test]
    fn the_scale_has_eleven_ticks_below_the_crest_row() {
        let ticks: Vec<_> = fills_of(&frame())
            .into_iter()
            .filter(|(rect, _)| rect.y == 33.0 && rect.height == 4.0)
            .collect();
        assert_eq!(ticks.len(), TICKS.len());
        assert_eq!(ticks[0].0, Rect::new(21.0, 33.0, 1.0, 4.0));
        assert_eq!(ticks[10].0, Rect::new(285.0, 33.0, 1.0, 4.0));
    }

    #[test]
    fn a_silent_bar_draws_nothing() {
        let silent = VuMeterFrame {
            level_db: [-60.0, -80.0],
            hold_db: [-60.0, -60.0],
            ..frame()
        };
        let bars = fills_of(&silent)
            .into_iter()
            .filter(|(rect, _)| rect.height == BAR_HEIGHT)
            .count();
        assert_eq!(bars, 0);
    }
}
