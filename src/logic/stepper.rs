//! Number-stepper rules: units with their step and suffix, value formatting,
//! suffix stripping, the edit-buffer character filter, and the commit and step
//! rules. Shared by the Iced `NumberStepper` and the Teksilo stepper widget.

use std::ops::RangeInclusive;

/// Maximum length (in bytes) of a stepper or slider edit buffer; typed
/// characters are dropped once it is reached.
pub const EDIT_BUFFER_MAX_LEN: usize = 20;

/// Unit of a stepper value, which fixes its step and display suffix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepperUnit {
    /// Step 0.25, suffix "dB"
    Decibels,
    /// Step 50, suffix "ms"
    Milliseconds,
}

impl StepperUnit {
    /// Amount added or removed by one press of an arrow.
    pub fn step(&self) -> f64 {
        match self {
            StepperUnit::Decibels => 0.25,
            StepperUnit::Milliseconds => 50.0,
        }
    }

    /// Unit suffix shown after the value.
    pub fn suffix(&self) -> &'static str {
        match self {
            StepperUnit::Decibels => "dB",
            StepperUnit::Milliseconds => "ms",
        }
    }
}

/// Formats a value with its unit suffix: two decimals, then trailing zeros and
/// a trailing dot are stripped. Examples: `-50.00` dB → `"-50 dB"`, `-14.25`
/// dB stays `"-14.25 dB"`, `1000.00` ms → `"1000 ms"`.
pub fn format_stepper_value(value: f64, unit: &StepperUnit) -> String {
    let formatted = format!("{:.2}", value);
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    format!(
        "{} {}",
        if trimmed.is_empty() { "0" } else { trimmed },
        unit.suffix()
    )
}

/// Strips surrounding whitespace and one trailing unit suffix (" dB", " ms",
/// " Hz" or " %") from a typed number before it is parsed.
pub fn strip_suffix(s: &str) -> &str {
    s.trim()
        .trim_end_matches(" dB")
        .trim_end_matches(" ms")
        .trim_end_matches(" Hz")
        .trim_end_matches(" %")
        .trim()
}

/// Whether a typed character may enter a numeric edit buffer: ASCII digits,
/// `.`, `-` and whitespace.
pub fn accepts_input_char(ch: char) -> bool {
    ch.is_ascii_digit() || ch == '.' || ch == '-' || ch.is_whitespace()
}

/// Whether `ch` is inserted into `buffer`: the buffer is shorter than
/// [`EDIT_BUFFER_MAX_LEN`] bytes and the character passes
/// [`accepts_input_char`].
pub fn accepts_input(buffer: &str, ch: char) -> bool {
    buffer.len() < EDIT_BUFFER_MAX_LEN && accepts_input_char(ch)
}

/// Text that seeds the edit buffer when editing starts: the bare number, with
/// no suffix (`{:.2}` for dB, `{:.0}` for ms).
pub fn edit_seed(value: f64, unit: &StepperUnit) -> String {
    match unit {
        StepperUnit::Decibels => format!("{:.2}", value),
        StepperUnit::Milliseconds => format!("{:.0}", value),
    }
}

/// Value committed from an edit buffer: the suffix is stripped, the number is
/// parsed and clamped to `range`. It is never rounded to the step. `None` when
/// the text does not parse (the edit is then dropped).
pub fn parse_commit(text: &str, range: &RangeInclusive<f64>) -> Option<f64> {
    strip_suffix(text)
        .parse::<f64>()
        .ok()
        .map(|v| v.clamp(*range.start(), *range.end()))
}

/// Value after one step in direction `dir` (`-1` or `1`) from `base`, clamped
/// to `range`.
pub fn step_value(base: f64, dir: i8, unit: &StepperUnit, range: &RangeInclusive<f64>) -> f64 {
    let new_val = base + dir as f64 * unit.step();
    new_val.clamp(*range.start(), *range.end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_have_their_step_and_suffix() {
        assert_eq!(StepperUnit::Decibels.step(), 0.25);
        assert_eq!(StepperUnit::Milliseconds.step(), 50.0);
        assert_eq!(StepperUnit::Decibels.suffix(), "dB");
        assert_eq!(StepperUnit::Milliseconds.suffix(), "ms");
    }

    #[test]
    fn formatting_strips_trailing_zeros_and_keeps_two_decimals() {
        assert_eq!(
            format_stepper_value(-50.0, &StepperUnit::Decibels),
            "-50 dB"
        );
        assert_eq!(
            format_stepper_value(-14.25, &StepperUnit::Decibels),
            "-14.25 dB"
        );
        assert_eq!(format_stepper_value(1.5, &StepperUnit::Decibels), "1.5 dB");
        assert_eq!(
            format_stepper_value(1000.0, &StepperUnit::Milliseconds),
            "1000 ms"
        );
    }

    #[test]
    fn formatting_zero_shows_a_single_zero() {
        assert_eq!(format_stepper_value(0.0, &StepperUnit::Decibels), "0 dB");
        assert_eq!(
            format_stepper_value(0.0, &StepperUnit::Milliseconds),
            "0 ms"
        );
    }

    #[test]
    fn formatting_rounds_to_two_decimals() {
        assert_eq!(
            format_stepper_value(1.234, &StepperUnit::Decibels),
            "1.23 dB"
        );
    }

    #[test]
    fn suffix_stripping_removes_known_units_and_whitespace() {
        assert_eq!(strip_suffix("-50.25 dB"), "-50.25");
        assert_eq!(strip_suffix("1000 ms"), "1000");
        assert_eq!(strip_suffix("44100 Hz"), "44100");
        assert_eq!(strip_suffix("75 %"), "75");
        assert_eq!(strip_suffix("  -5 dB  "), "-5");
        assert_eq!(strip_suffix("3.14"), "3.14");
    }

    #[test]
    fn suffix_stripping_leaves_unknown_text_alone() {
        assert_eq!(strip_suffix("notanumber"), "notanumber");
        assert_eq!(strip_suffix("5dB"), "5dB");
        assert_eq!(strip_suffix(""), "");
    }

    #[test]
    fn the_character_filter_accepts_digits_dot_minus_and_whitespace() {
        for ch in '0'..='9' {
            assert!(accepts_input_char(ch));
        }
        assert!(accepts_input_char('.'));
        assert!(accepts_input_char('-'));
        assert!(accepts_input_char(' '));
        assert!(accepts_input_char('\t'));
    }

    #[test]
    fn the_character_filter_rejects_letters_symbols_and_non_ascii_digits() {
        for ch in ['a', 'e', 'Z', '+', ',', '%', 'ñ', '٣'] {
            assert!(!accepts_input_char(ch), "{ch:?} must be rejected");
        }
    }

    #[test]
    fn the_edit_buffer_stops_growing_at_twenty_bytes() {
        let full = "1".repeat(EDIT_BUFFER_MAX_LEN);
        let almost = "1".repeat(EDIT_BUFFER_MAX_LEN - 1);
        assert!(!accepts_input(&full, '2'));
        assert!(accepts_input(&almost, '2'));
        assert!(!accepts_input(&almost, 'x'));
    }

    #[test]
    fn the_edit_seed_has_no_suffix() {
        assert_eq!(edit_seed(-50.0, &StepperUnit::Decibels), "-50.00");
        assert_eq!(edit_seed(1.5, &StepperUnit::Decibels), "1.50");
        assert_eq!(edit_seed(1000.0, &StepperUnit::Milliseconds), "1000");
        assert_eq!(edit_seed(342.6, &StepperUnit::Milliseconds), "343");
    }

    #[test]
    fn a_commit_is_parsed_after_stripping_and_never_rounded_to_the_step() {
        let range = -60.0..=0.0;
        assert_eq!(parse_commit("-1.78", &range), Some(-1.78));
        assert_eq!(parse_commit(" -14.3 dB ", &range), Some(-14.3));
        let ms = 0.0..=5000.0;
        assert_eq!(parse_commit("342", &ms), Some(342.0));
    }

    #[test]
    fn a_commit_is_clamped_to_the_range() {
        let range = 500.0..=5000.0;
        assert_eq!(parse_commit("10", &range), Some(500.0));
        assert_eq!(parse_commit("99999 ms", &range), Some(5000.0));
    }

    #[test]
    fn an_unparsable_commit_is_dropped() {
        let range = 0.0..=1.0;
        assert_eq!(parse_commit("", &range), None);
        assert_eq!(parse_commit("abc", &range), None);
        assert_eq!(parse_commit("1 Hz extra", &range), None);
    }

    #[test]
    fn a_step_moves_by_the_unit_step_and_is_clamped() {
        let db = -60.0..=0.0;
        assert!((step_value(-1.78, 1, &StepperUnit::Decibels, &db) - -1.53).abs() < 1e-10);
        assert!((step_value(1.78, -1, &StepperUnit::Decibels, &(-9.0..=9.0)) - 1.53).abs() < 1e-10);
        assert_eq!(step_value(-0.1, 1, &StepperUnit::Decibels, &db), 0.0);
        let ms = 500.0..=5000.0;
        assert_eq!(step_value(342.0, 1, &StepperUnit::Milliseconds, &ms), 500.0);
        assert_eq!(
            step_value(1000.0, -1, &StepperUnit::Milliseconds, &ms),
            950.0
        );
        assert_eq!(
            step_value(520.0, -1, &StepperUnit::Milliseconds, &ms),
            500.0
        );
    }
}
