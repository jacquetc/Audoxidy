//! Equalizer rules: band labels, gain rounding and clamping, the unified
//! preset list and its delete rule, the band-gain settings encoding, and the
//! name of an exported preset.

use crate::audio::preset::EqPreset;
use std::path::Path;

/// Lowest gain (dB) of the preamp and of every band.
pub const EQ_GAIN_MIN: f32 = -9.0;

/// Highest gain (dB) of the preamp and of every band.
pub const EQ_GAIN_MAX: f32 = 9.0;

/// Name of the built-in flat preset, always listed first.
pub const DEFAULT_PRESET_NAME: &str = "Default";

/// Name given to an exported preset whose file name has no usable stem.
pub const FALLBACK_EXPORT_PRESET_NAME: &str = "Custom";

/// Label of a band's centre frequency: `{:.1}k` from 1000 Hz up, with a
/// trailing `.0k` shortened to `k` (`1000` → `"1k"`, `1250` → `"1.2k"`), and
/// whole hertz below (`31.5` → `"32"`).
pub fn band_frequency_label(freq: f32) -> String {
    if freq >= 1000.0 {
        format!("{:.1}k", freq / 1000.0).replace(".0k", "k")
    } else {
        format!("{:.0}", freq)
    }
}

/// Rounds a gain to one decimal place, without clamping (used when a preset's
/// gains are loaded into the sliders).
pub fn round_to_tenth(gain: f32) -> f32 {
    (gain * 10.0).round() / 10.0
}

/// Gain set from a slider (preamp or band): rounded to one decimal place, then
/// clamped to `[EQ_GAIN_MIN, EQ_GAIN_MAX]`.
pub fn slider_gain(value: f32) -> f32 {
    round_to_tenth(value).clamp(EQ_GAIN_MIN, EQ_GAIN_MAX)
}

/// Gains loaded from a preset, each rounded to one decimal place.
pub fn rounded_gains(gains: &[f32]) -> Vec<f32> {
    gains.iter().map(|&g| round_to_tenth(g)).collect()
}

/// Preset list of the Load dialog, in display order:
/// 1. the built-in "Default" (when present);
/// 2. the custom presets, A–Z case-insensitively;
/// 3. the other built-ins, A–Z case-insensitively, without the hidden ones.
pub fn unified_presets<'a>(
    builtins: &'a [EqPreset],
    customs: &'a [EqPreset],
    hidden_builtins: &[String],
) -> Vec<&'a EqPreset> {
    let mut all: Vec<&EqPreset> = Vec::new();

    if let Some(default) = builtins.iter().find(|p| p.name == DEFAULT_PRESET_NAME) {
        all.push(default);
    }

    let mut custom: Vec<&EqPreset> = customs.iter().collect();
    custom.sort_by_key(|p| p.name.to_lowercase());
    all.extend(custom);

    let mut builtin: Vec<&EqPreset> = builtins
        .iter()
        .filter(|p| p.name != DEFAULT_PRESET_NAME && !hidden_builtins.contains(&p.name))
        .collect();
    builtin.sort_by_key(|p| p.name.to_lowercase());
    all.extend(builtin);

    all
}

/// What deleting a preset by name does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetDeleteAction {
    /// A built-in with no custom preset of the same name: hide it.
    HideBuiltin,
    /// A custom preset (or an unknown name): delete it from storage.
    DeleteCustom,
}

/// Decides how a preset is deleted: a built-in name with no custom override is
/// hidden, anything else is deleted as a custom preset.
pub fn preset_delete_action(
    name: &str,
    builtins: &[EqPreset],
    customs: &[EqPreset],
) -> PresetDeleteAction {
    let is_builtin =
        builtins.iter().any(|p| p.name == name) && !customs.iter().any(|p| p.name == name);
    if is_builtin {
        PresetDeleteAction::HideBuiltin
    } else {
        PresetDeleteAction::DeleteCustom
    }
}

/// Adds `name` to the hidden built-ins unless it is already there.
pub fn hide_builtin(hidden_builtins: &mut Vec<String>, name: &str) {
    if !hidden_builtins.iter().any(|h| h == name) {
        hidden_builtins.push(name.to_string());
    }
}

/// Comma-separated band gains with one decimal each (`"0.0,-1.5,3.0"`).
pub fn encode_band_gains<'a>(gains: impl IntoIterator<Item = &'a f32>) -> String {
    gains
        .into_iter()
        .map(|g| format!("{:.1}", g))
        .collect::<Vec<_>>()
        .join(",")
}

/// Values persisted under `eq_band_gains_20` and `eq_band_gains_31` for the
/// active gains: the 20-band value is always the first 20 gains; the 31-band
/// value is written only when there are more than 20 gains.
pub fn encode_band_gain_settings(gains: &[f32]) -> (String, Option<String>) {
    let bands_20 = encode_band_gains(gains.iter().take(20));
    let bands_31 = (gains.len() > 20).then(|| encode_band_gains(gains));
    (bands_20, bands_31)
}

/// Decodes a persisted band-gain list: values that do not parse are skipped,
/// and the list is accepted only if exactly `expected_len` values remain.
pub fn decode_band_gains(value: &str, expected_len: usize) -> Option<Vec<f32>> {
    let gains: Vec<f32> = value
        .split(',')
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    (gains.len() == expected_len).then_some(gains)
}

/// Persisted form of the preamp gain (`preamp_gain`): one decimal.
pub fn encode_preamp_gain(gain: f32) -> String {
    format!("{:.1}", gain)
}

/// Name of a preset exported to `path`: the file stem, or
/// [`FALLBACK_EXPORT_PRESET_NAME`] when there is none.
pub fn export_preset_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(FALLBACK_EXPORT_PRESET_NAME)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str) -> EqPreset {
        EqPreset::new(name, 0.0, Some(vec![0.0; 20]), Some(vec![0.0; 31]))
    }

    fn names(list: &[&EqPreset]) -> Vec<String> {
        list.iter().map(|p| p.name.clone()).collect()
    }

    #[test]
    fn band_labels_use_kilohertz_from_one_thousand() {
        assert_eq!(band_frequency_label(1000.0), "1k");
        assert_eq!(band_frequency_label(1250.0), "1.2k");
        assert_eq!(band_frequency_label(1600.0), "1.6k");
        assert_eq!(band_frequency_label(20000.0), "20k");
        assert_eq!(band_frequency_label(12500.0), "12.5k");
    }

    #[test]
    fn band_labels_below_one_thousand_are_whole_hertz() {
        assert_eq!(band_frequency_label(31.5), "32");
        assert_eq!(band_frequency_label(25.0), "25");
        assert_eq!(band_frequency_label(999.0), "999");
    }

    #[test]
    fn slider_gains_round_to_a_tenth_and_clamp_to_nine_db() {
        assert_eq!(slider_gain(1.24), 1.2);
        assert_eq!(slider_gain(-3.06), -3.1);
        assert_eq!(slider_gain(9.04), 9.0);
        assert_eq!(slider_gain(12.0), 9.0);
        assert_eq!(slider_gain(-12.0), -9.0);
    }

    #[test]
    fn preset_gains_round_to_a_tenth_without_clamping() {
        assert_eq!(rounded_gains(&[1.26, -12.34]), vec![1.3, -12.3]);
    }

    #[test]
    fn the_unified_list_puts_default_then_customs_then_builtins() {
        let builtins = vec![preset("Rock"), preset("Default"), preset("bass")];
        let customs = vec![preset("zeta"), preset("Alpha")];
        let list = unified_presets(&builtins, &customs, &[]);
        assert_eq!(names(&list), ["Default", "Alpha", "zeta", "bass", "Rock"]);
    }

    #[test]
    fn the_unified_list_drops_hidden_builtins_but_never_default() {
        let builtins = vec![preset("Default"), preset("Rock"), preset("Jazz")];
        let hidden = vec!["Rock".to_string(), "Default".to_string()];
        let list = unified_presets(&builtins, &[], &hidden);
        assert_eq!(names(&list), ["Default", "Jazz"]);
    }

    #[test]
    fn the_unified_list_has_no_default_entry_without_a_default_builtin() {
        let builtins = vec![preset("Rock")];
        let list = unified_presets(&builtins, &[], &[]);
        assert_eq!(names(&list), ["Rock"]);
    }

    #[test]
    fn deleting_a_builtin_without_override_hides_it() {
        let builtins = vec![preset("Rock")];
        assert_eq!(
            preset_delete_action("Rock", &builtins, &[]),
            PresetDeleteAction::HideBuiltin
        );
    }

    #[test]
    fn deleting_a_custom_or_an_overridden_builtin_deletes_the_custom() {
        let builtins = vec![preset("Rock")];
        let customs = vec![preset("Rock"), preset("Mine")];
        assert_eq!(
            preset_delete_action("Rock", &builtins, &customs),
            PresetDeleteAction::DeleteCustom
        );
        assert_eq!(
            preset_delete_action("Mine", &builtins, &customs),
            PresetDeleteAction::DeleteCustom
        );
        assert_eq!(
            preset_delete_action("Unknown", &builtins, &customs),
            PresetDeleteAction::DeleteCustom
        );
    }

    #[test]
    fn hiding_a_builtin_twice_lists_it_once() {
        let mut hidden = Vec::new();
        hide_builtin(&mut hidden, "Rock");
        hide_builtin(&mut hidden, "Rock");
        assert_eq!(hidden, ["Rock"]);
    }

    #[test]
    fn band_gains_encode_with_one_decimal() {
        assert_eq!(encode_band_gains(&[0.0, -1.26, 3.0]), "0.0,-1.3,3.0");
    }

    #[test]
    fn twenty_band_settings_write_only_the_twenty_band_value() {
        let gains = vec![1.0; 20];
        let (b20, b31) = encode_band_gain_settings(&gains);
        assert_eq!(b20.split(',').count(), 20);
        assert_eq!(b31, None);
    }

    #[test]
    fn thirty_one_band_settings_write_the_first_twenty_and_all_thirty_one() {
        let gains: Vec<f32> = (0..31).map(|i| i as f32).collect();
        let (b20, b31) = encode_band_gain_settings(&gains);
        assert_eq!(b20.split(',').count(), 20);
        assert!(b20.starts_with("0.0,1.0,2.0"));
        assert!(b20.ends_with("19.0"));
        let b31 = b31.unwrap_or_default();
        assert_eq!(b31.split(',').count(), 31);
        assert!(b31.ends_with("30.0"));
    }

    #[test]
    fn band_gains_decode_only_with_the_expected_count() {
        assert_eq!(decode_band_gains("1.0,-2.5", 2), Some(vec![1.0, -2.5]));
        assert_eq!(decode_band_gains("1.0,-2.5", 3), None);
        assert_eq!(decode_band_gains("1.0,x,-2.5", 2), Some(vec![1.0, -2.5]));
        assert_eq!(decode_band_gains("", 0), Some(vec![]));
    }

    #[test]
    fn band_gains_round_trip_through_the_settings_encoding() {
        let gains: Vec<f32> = (0..20).map(|i| i as f32 * 0.5 - 4.0).collect();
        let (b20, _) = encode_band_gain_settings(&gains);
        assert_eq!(decode_band_gains(&b20, 20), Some(gains));
    }

    #[test]
    fn the_preamp_is_persisted_with_one_decimal() {
        assert_eq!(encode_preamp_gain(-3.26), "-3.3");
        assert_eq!(encode_preamp_gain(0.0), "0.0");
    }

    #[test]
    fn an_exported_preset_is_named_after_the_file_stem() {
        assert_eq!(export_preset_name(Path::new("/tmp/My EQ.json")), "My EQ");
        assert_eq!(
            export_preset_name(Path::new("bass.boost.json")),
            "bass.boost"
        );
        assert_eq!(export_preset_name(Path::new("/")), "Custom");
    }
}
