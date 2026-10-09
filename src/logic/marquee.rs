//! Marquee (scrolling text) windows for text that is too long for its slot.
//!
//! The text scrolls one character per step; once its end is reached a
//! three-space gap is shown and the start is joined back on, and the cycle
//! lasts `len + 10` steps (the last ten show the start of the text at rest).

use std::time::Duration;

/// Visible width, in characters, of the player's title marquee.
pub const TITLE_MARQUEE_WIDTH: usize = 39;

/// Visible width, in characters, of the player's artist marquee.
pub const ARTIST_MARQUEE_WIDTH: usize = 40;

/// Gap shown between the end of the text and its repeated start.
pub const MARQUEE_GAP: &str = "   ";

/// Extra steps in one marquee cycle beyond the text length.
pub const MARQUEE_CYCLE_PADDING: usize = 10;

/// Period of the player's marquee tick (one character per tick).
pub const MARQUEE_TICK_PERIOD: Duration = Duration::from_millis(500);

/// Whether a text of `char_len` characters overflows a `limit`-character slot
/// and must scroll.
pub fn needs_marquee(char_len: usize, limit: usize) -> bool {
    char_len > limit
}

/// The `limit`-character window of `chars` shown at marquee step `step`.
///
/// The offset is `step % (len + 10)`. While the offset is inside the text the
/// window starts there; when the window runs past the end, [`MARQUEE_GAP`] is
/// appended, then as much of the start as still fits after the gap. Offsets
/// past the end show the first `limit` characters.
pub fn marquee_window(chars: &[char], limit: usize, step: u64) -> String {
    let len = chars.len();
    let offset = step as usize % (len + MARQUEE_CYCLE_PADDING);
    if offset < len {
        let end = (offset + limit).min(len);
        let mut s: String = chars[offset..end].iter().collect();
        if offset + limit > len {
            s.push_str(MARQUEE_GAP);
            let needed = (offset + limit) - len;
            if needed > MARQUEE_GAP.len() {
                let rem = needed - MARQUEE_GAP.len();
                s.extend(&chars[0..rem.min(len)]);
            }
        }
        s
    } else {
        chars[0..limit.min(len)].iter().collect()
    }
}

/// The player marquee: `full_text` itself when its `chars` fit in `limit`,
/// otherwise the window at step `tick` (one character per tick).
///
/// `chars` is the character cache of the text; it is passed separately because
/// the player refreshes the cache and the displayed text at different times.
pub fn player_marquee(full_text: &str, chars: &[char], limit: usize, tick: u64) -> String {
    if needs_marquee(chars.len(), limit) {
        marquee_window(chars, limit, tick)
    } else {
        full_text.to_string()
    }
}

/// The slower list marquee: `text` itself when it fits in `limit`
/// characters, otherwise the window at step `tick / 3` (one character every
/// three ticks).
pub fn apply_marquee(text: &str, limit: usize, tick: u64) -> String {
    // Fast path: short ASCII text never scrolls.
    if text.len() <= limit && text.is_ascii() {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text.to_string();
    }
    marquee_window(&chars, limit, tick / 3)
}

/// Marquee tick reached after `elapsed` with one tick every `period`, so a
/// UI can drive the marquee from a clock instead of counting timer events.
/// A zero period yields tick `0`.
pub fn tick_from_elapsed(elapsed: Duration, period: Duration) -> u64 {
    if period.is_zero() {
        return 0;
    }
    u64::try_from(elapsed.as_nanos() / period.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn short_text_does_not_need_a_marquee() {
        assert!(!needs_marquee(39, TITLE_MARQUEE_WIDTH));
        assert!(needs_marquee(40, TITLE_MARQUEE_WIDTH));
        assert!(!needs_marquee(40, ARTIST_MARQUEE_WIDTH));
        assert!(needs_marquee(41, ARTIST_MARQUEE_WIDTH));
    }

    #[test]
    fn the_window_starts_at_the_step_offset() {
        let c = chars("abcdefghij");
        assert_eq!(marquee_window(&c, 4, 0), "abcd");
        assert_eq!(marquee_window(&c, 4, 3), "defg");
        assert_eq!(marquee_window(&c, 4, 6), "ghij");
    }

    #[test]
    fn running_past_the_end_appends_the_gap_then_the_start() {
        let c = chars("abcdefghij");
        // offset 7: "hij" + gap (needed = 1, not more than the gap).
        assert_eq!(marquee_window(&c, 4, 7), "hij   ");
        // offset 8, limit 8: "ij" + gap + start (needed 6, rem 3 → "abc").
        assert_eq!(marquee_window(&c, 8, 8), "ij   abc");
    }

    #[test]
    fn offsets_past_the_end_show_the_start_at_rest() {
        let c = chars("abcdefghij");
        for step in 10..20 {
            assert_eq!(marquee_window(&c, 4, step), "abcd");
        }
    }

    #[test]
    fn the_cycle_is_the_length_plus_ten() {
        let c = chars("abcdefghij");
        assert_eq!(marquee_window(&c, 4, 20), marquee_window(&c, 4, 0));
        assert_eq!(marquee_window(&c, 4, 23), marquee_window(&c, 4, 3));
    }

    #[test]
    fn the_window_counts_characters_not_bytes() {
        let c = chars("ñáéíóú");
        assert_eq!(marquee_window(&c, 3, 1), "áéí");
    }

    #[test]
    fn an_empty_text_yields_an_empty_window() {
        assert_eq!(marquee_window(&[], 4, 7), "");
    }

    #[test]
    fn the_player_marquee_returns_the_full_text_when_it_fits() {
        let text = "Short title";
        assert_eq!(
            player_marquee(text, &chars(text), TITLE_MARQUEE_WIDTH, 17),
            text
        );
    }

    #[test]
    fn the_player_marquee_scrolls_one_character_per_tick() {
        let text = "A title that is far too long to fit in the player slot";
        let c = chars(text);
        assert_eq!(
            player_marquee(text, &c, TITLE_MARQUEE_WIDTH, 2),
            c[2..2 + TITLE_MARQUEE_WIDTH].iter().collect::<String>()
        );
    }

    #[test]
    fn apply_marquee_keeps_text_that_fits() {
        assert_eq!(apply_marquee("abc", 3, 99), "abc");
        assert_eq!(apply_marquee("ñandú", 5, 99), "ñandú");
    }

    #[test]
    fn apply_marquee_moves_one_character_every_three_ticks() {
        assert_eq!(apply_marquee("abcdefghij", 4, 0), "abcd");
        assert_eq!(apply_marquee("abcdefghij", 4, 2), "abcd");
        assert_eq!(apply_marquee("abcdefghij", 4, 3), "bcde");
        assert_eq!(apply_marquee("abcdefghij", 4, 9), "defg");
    }

    #[test]
    fn the_tick_is_derived_from_elapsed_time() {
        assert_eq!(tick_from_elapsed(Duration::ZERO, MARQUEE_TICK_PERIOD), 0);
        assert_eq!(
            tick_from_elapsed(Duration::from_millis(499), MARQUEE_TICK_PERIOD),
            0
        );
        assert_eq!(
            tick_from_elapsed(Duration::from_millis(500), MARQUEE_TICK_PERIOD),
            1
        );
        assert_eq!(
            tick_from_elapsed(Duration::from_millis(5_250), MARQUEE_TICK_PERIOD),
            10
        );
    }

    #[test]
    fn a_zero_tick_period_yields_tick_zero() {
        assert_eq!(tick_from_elapsed(Duration::from_secs(9), Duration::ZERO), 0);
    }
}
