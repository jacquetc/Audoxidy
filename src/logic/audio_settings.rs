//! Audio-configuration rules: the system (PipeWire/PulseAudio) clock
//! selection, the quantum estimate, the "reset to defaults" choices, the
//! option lists and labels of the configuration tab, and the exact strings the
//! settings are persisted with (shared with the database the Iced UI writes).

use crate::audio::device_manager::{AudioDeviceInfo, BitDepth};

/// How the system sound server's rate or quantum is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemSelection<Val> {
    /// Leave the server's own setting alone.
    Default,
    /// Follow the player (rate) or estimate from the rate (quantum).
    Automatic,
    /// Force this value.
    Fixed(Val),
}

/// Sample rate chosen by "reset to defaults", and the fallback when the player
/// rate is automatic.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Output channel count chosen by "reset to defaults" and used when none is
/// stored.
pub const DEFAULT_CHANNELS: u16 = 2;

/// Output bit depth chosen by "reset to defaults" (32-bit float).
pub const DEFAULT_BIT_DEPTH: BitDepth = BitDepth::Bits32Float;

/// Output buffer size chosen by "reset to defaults" (`None` = automatic).
pub const DEFAULT_BUFFER_SIZE: Option<u32> = None;

/// Host preferred by "reset to defaults" when it is available.
pub const PREFERRED_HOST: &str = "ALSA";

/// Player sample-rate options, in Hz.
pub const SAMPLE_RATE_OPTIONS: [u32; 8] =
    [44100, 48000, 88200, 96000, 176400, 192000, 352800, 384000];

/// Fixed buffer-size options, in frames (an "automatic" entry precedes them).
pub const BUFFER_SIZE_OPTIONS: [u32; 6] = [256, 512, 1024, 2048, 4096, 8192];

/// Fixed system-rate options, in Hz ("Default" and "Automatic" precede them).
pub const SYSTEM_RATE_OPTIONS: [u32; 8] = SAMPLE_RATE_OPTIONS;

/// Fixed system-quantum options, in frames ("Default" and "Automatic" precede
/// them).
pub const SYSTEM_QUANTUM_OPTIONS: [u32; 6] = BUFFER_SIZE_OPTIONS;

/// Bit-depth options, in display order.
pub const BIT_DEPTH_OPTIONS: [BitDepth; 3] =
    [BitDepth::Bits16, BitDepth::Bits24, BitDepth::Bits32Float];

/// Output channel options of the configuration tab: `(label, channel count)`.
pub const OUTPUT_CHANNEL_OPTIONS: [(&str, u16); 6] = [
    ("1.0 Mono", 1),
    ("2.0 Stereo", 2),
    ("2.1 Stereo + Sub", 3),
    ("4.0 Quad", 4),
    ("5.1 Surround", 6),
    ("7.1 Surround", 8),
];

/// Quantum estimated for a rate: `rate × 10 ms` frames, raised to the next of
/// 256, 512, 1024, 2048, 4096 or 8192 (a value equal to one of them moves up
/// to the next; 8192 is the ceiling).
pub fn estimate_quantum(rate: u32) -> u32 {
    let frames = (rate as f64 * 10.0 / 1000.0) as u32;
    if frames < 256 {
        256
    } else if frames < 512 {
        512
    } else if frames < 1024 {
        1024
    } else if frames < 2048 {
        2048
    } else if frames < 4096 {
        4096
    } else {
        8192
    }
}

/// Rate forced on the system server for a selection: `None` for Default, the
/// caller's `automatic_rate` for Automatic, the value for Fixed.
pub fn resolve_system_rate(
    selection: SystemSelection<u32>,
    automatic_rate: Option<u32>,
) -> Option<u32> {
    match selection {
        SystemSelection::Default => None,
        SystemSelection::Automatic => automatic_rate,
        SystemSelection::Fixed(r) => Some(r),
    }
}

/// Quantum forced on the system server for a selection: `None` for Default,
/// [`estimate_quantum`] of `estimate_rate` for Automatic, the value for Fixed.
pub fn resolve_system_quantum(selection: SystemSelection<u32>, estimate_rate: u32) -> Option<u32> {
    match selection {
        SystemSelection::Default => None,
        SystemSelection::Automatic => Some(estimate_quantum(estimate_rate)),
        SystemSelection::Fixed(q) => Some(q),
    }
}

/// Host chosen by "reset to defaults": "ALSA" when listed, else the first
/// host; `None` (keep the current one) when the list is empty.
pub fn default_host(hosts: &[String]) -> Option<&str> {
    if hosts.iter().any(|h| h == PREFERRED_HOST) {
        Some(PREFERRED_HOST)
    } else {
        hosts.first().map(String::as_str)
    }
}

/// Device chosen by "reset to defaults": the first whose lower-cased name
/// contains "pipewire", else "pulse", else "default", else the first device;
/// `None` (keep the current one) when the list is empty.
pub fn default_device(devices: &[AudioDeviceInfo]) -> Option<&AudioDeviceInfo> {
    let find = |needle: &str| {
        devices
            .iter()
            .find(|d| d.name.to_lowercase().contains(needle))
    };
    find("pipewire")
        .or_else(|| find("pulse"))
        .or_else(|| find("default"))
        .or_else(|| devices.first())
}

/// Label of a bit depth in the output selector.
pub fn bit_depth_label(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Bits16 => "16 bits Int",
        BitDepth::Bits24 => "24 bits Int",
        BitDepth::Bits32Float => "32 bits Float",
    }
}

/// Normalises the engine's bit-depth description for display: every 24-bit
/// integer spelling becomes "24 bits Int"; otherwise "-bit" becomes " bits".
pub fn bit_depth_display(raw: &str) -> String {
    match raw {
        "24/32-bit Int" | "24/32-bits Int" | "24-bit Int" | "24 bits Int" => {
            "24 bits Int".to_string()
        }
        other => other.replace("-bit", " bits"),
    }
}

/// Label of a channel count in the info column (file input and device output):
/// Mono, Stereo, Stereo + Sub, 4.0 Quad, 5.1 Surround, 7.1 Surround, else
/// "N canales".
pub fn channel_count_label(channels: u16) -> String {
    match channels {
        1 => "Mono".to_string(),
        2 => "Stereo".to_string(),
        3 => "Stereo + Sub".to_string(),
        4 => "4.0 Quad".to_string(),
        6 => "5.1 Surround".to_string(),
        8 => "7.1 Surround".to_string(),
        other => format!("{} canales", other),
    }
}

/// Persisted form of an optional number (`audio_sample_rate`,
/// `audio_buffer_size`): the number, or `"auto"` for `None`.
pub fn encode_auto_u32(value: Option<u32>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "auto".to_string())
}

/// Decodes `audio_sample_rate` / `audio_buffer_size`: `"auto"` and anything
/// that does not parse are `None`.
pub fn decode_auto_u32(value: &str) -> Option<u32> {
    if value == "auto" {
        None
    } else {
        value.parse::<u32>().ok()
    }
}

/// Persisted form of a bit depth (`audio_bit_depth`): "16", "24" or "32".
pub fn encode_bit_depth(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Bits16 => "16",
        BitDepth::Bits24 => "24",
        BitDepth::Bits32Float => "32",
    }
}

/// Decodes `audio_bit_depth`; any other string is `None`.
pub fn decode_bit_depth(value: &str) -> Option<BitDepth> {
    match value {
        "16" => Some(BitDepth::Bits16),
        "24" => Some(BitDepth::Bits24),
        "32" => Some(BitDepth::Bits32Float),
        _ => None,
    }
}

/// Persisted form of the channel count (`audio_channels`).
pub fn encode_channels(channels: u16) -> String {
    channels.to_string()
}

/// Decodes `audio_channels`; a string that does not parse is `None`.
pub fn decode_channels(value: &str) -> Option<u16> {
    value.parse::<u16>().ok()
}

/// Persisted form of a system selection (`audio_system_rate`,
/// `audio_system_quantum`): "default", "auto" or "fixed:N".
pub fn encode_system_selection(selection: SystemSelection<u32>) -> String {
    match selection {
        SystemSelection::Default => "default".to_string(),
        SystemSelection::Automatic => "auto".to_string(),
        SystemSelection::Fixed(v) => format!("fixed:{}", v),
    }
}

/// Decodes `audio_system_rate` / `audio_system_quantum`; anything that is not
/// "auto" or a valid "fixed:N" is `Default`.
pub fn decode_system_selection(value: &str) -> SystemSelection<u32> {
    match value {
        "default" => SystemSelection::Default,
        "auto" => SystemSelection::Automatic,
        other => other
            .strip_prefix("fixed:")
            .and_then(|n| n.parse::<u32>().ok())
            .map_or(SystemSelection::Default, SystemSelection::Fixed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: name.to_string(),
            id: None,
        }
    }

    #[test]
    fn the_quantum_estimate_is_ten_milliseconds_rounded_up() {
        assert_eq!(estimate_quantum(8_000), 256); // 80 frames
        assert_eq!(estimate_quantum(44_100), 512); // 441
        assert_eq!(estimate_quantum(48_000), 512); // 480
        assert_eq!(estimate_quantum(96_000), 1024); // 960
        assert_eq!(estimate_quantum(192_000), 2048); // 1920
        assert_eq!(estimate_quantum(384_000), 4096); // 3840
        assert_eq!(estimate_quantum(1_000_000), 8192); // 10000, capped
    }

    #[test]
    fn a_quantum_estimate_on_a_boundary_moves_up() {
        assert_eq!(estimate_quantum(25_600), 512); // exactly 256 frames
        assert_eq!(estimate_quantum(409_600), 8192); // exactly 4096 frames
    }

    #[test]
    fn system_rate_resolution_follows_the_selection() {
        assert_eq!(
            resolve_system_rate(SystemSelection::Default, Some(44100)),
            None
        );
        assert_eq!(
            resolve_system_rate(SystemSelection::Automatic, Some(44100)),
            Some(44100)
        );
        assert_eq!(resolve_system_rate(SystemSelection::Automatic, None), None);
        assert_eq!(
            resolve_system_rate(SystemSelection::Fixed(96000), Some(44100)),
            Some(96000)
        );
    }

    #[test]
    fn system_quantum_resolution_estimates_only_when_automatic() {
        assert_eq!(
            resolve_system_quantum(SystemSelection::Default, 48000),
            None
        );
        assert_eq!(
            resolve_system_quantum(SystemSelection::Automatic, 48000),
            Some(512)
        );
        assert_eq!(
            resolve_system_quantum(SystemSelection::Fixed(128), 48000),
            Some(128)
        );
    }

    #[test]
    fn the_default_host_prefers_alsa_then_the_first_host() {
        let hosts = vec!["JACK".to_string(), "ALSA".to_string()];
        assert_eq!(default_host(&hosts), Some("ALSA"));
        let hosts = vec!["JACK".to_string(), "Pulse".to_string()];
        assert_eq!(default_host(&hosts), Some("JACK"));
        assert_eq!(default_host(&[]), None);
    }

    #[test]
    fn the_default_device_prefers_pipewire_then_pulse_then_default() {
        let devices = vec![
            device("hw:0"),
            device("PulseAudio"),
            device("PipeWire Sound"),
        ];
        assert_eq!(
            default_device(&devices).map(|d| d.name.as_str()),
            Some("PipeWire Sound")
        );
        let devices = vec![device("default"), device("pulse")];
        assert_eq!(
            default_device(&devices).map(|d| d.name.as_str()),
            Some("pulse")
        );
        let devices = vec![device("hw:0"), device("Default Output")];
        assert_eq!(
            default_device(&devices).map(|d| d.name.as_str()),
            Some("Default Output")
        );
    }

    #[test]
    fn the_default_device_falls_back_to_the_first_one() {
        let devices = vec![device("hw:0"), device("hw:1")];
        assert_eq!(
            default_device(&devices).map(|d| d.name.as_str()),
            Some("hw:0")
        );
        assert!(default_device(&[]).is_none());
    }

    #[test]
    fn the_reset_defaults_are_48k_float_auto_buffer_and_stereo() {
        assert_eq!(DEFAULT_SAMPLE_RATE, 48000);
        assert_eq!(DEFAULT_BIT_DEPTH, BitDepth::Bits32Float);
        assert_eq!(DEFAULT_BUFFER_SIZE, None);
        assert_eq!(DEFAULT_CHANNELS, 2);
    }

    #[test]
    fn bit_depth_labels_match_the_selector() {
        let labels: Vec<_> = BIT_DEPTH_OPTIONS
            .iter()
            .map(|&b| bit_depth_label(b))
            .collect();
        assert_eq!(labels, ["16 bits Int", "24 bits Int", "32 bits Float"]);
    }

    #[test]
    fn the_bit_depth_display_normalises_24_bit_spellings() {
        for raw in [
            "24/32-bit Int",
            "24/32-bits Int",
            "24-bit Int",
            "24 bits Int",
        ] {
            assert_eq!(bit_depth_display(raw), "24 bits Int");
        }
        assert_eq!(bit_depth_display("16-bit Int"), "16 bits Int");
        assert_eq!(bit_depth_display("32-bit Float"), "32 bits Float");
        assert_eq!(bit_depth_display("--"), "--");
    }

    #[test]
    fn channel_counts_have_layout_labels() {
        let labels: Vec<_> = [1, 2, 3, 4, 6, 8]
            .iter()
            .map(|&c| channel_count_label(c))
            .collect();
        assert_eq!(
            labels,
            [
                "Mono",
                "Stereo",
                "Stereo + Sub",
                "4.0 Quad",
                "5.1 Surround",
                "7.1 Surround"
            ]
        );
        assert_eq!(channel_count_label(5), "5 canales");
        assert_eq!(channel_count_label(16), "16 canales");
    }

    #[test]
    fn the_output_channel_options_list_six_layouts() {
        let counts: Vec<u16> = OUTPUT_CHANNEL_OPTIONS.iter().map(|o| o.1).collect();
        assert_eq!(counts, [1, 2, 3, 4, 6, 8]);
        assert_eq!(OUTPUT_CHANNEL_OPTIONS[2].0, "2.1 Stereo + Sub");
    }

    #[test]
    fn the_rate_and_size_option_lists_are_complete() {
        assert_eq!(SAMPLE_RATE_OPTIONS.first(), Some(&44100));
        assert_eq!(SAMPLE_RATE_OPTIONS.last(), Some(&384000));
        assert_eq!(SYSTEM_RATE_OPTIONS, SAMPLE_RATE_OPTIONS);
        assert_eq!(BUFFER_SIZE_OPTIONS, [256, 512, 1024, 2048, 4096, 8192]);
        assert_eq!(SYSTEM_QUANTUM_OPTIONS, BUFFER_SIZE_OPTIONS);
    }

    #[test]
    fn optional_numbers_persist_as_auto_or_digits() {
        assert_eq!(encode_auto_u32(None), "auto");
        assert_eq!(encode_auto_u32(Some(48000)), "48000");
        assert_eq!(decode_auto_u32("auto"), None);
        assert_eq!(decode_auto_u32("1024"), Some(1024));
        assert_eq!(decode_auto_u32("garbage"), None);
    }

    #[test]
    fn bit_depths_persist_as_their_bit_count() {
        for depth in BIT_DEPTH_OPTIONS {
            assert_eq!(decode_bit_depth(encode_bit_depth(depth)), Some(depth));
        }
        assert_eq!(encode_bit_depth(BitDepth::Bits32Float), "32");
        assert_eq!(decode_bit_depth("32f"), None);
    }

    #[test]
    fn channels_persist_as_digits() {
        assert_eq!(encode_channels(6), "6");
        assert_eq!(decode_channels("8"), Some(8));
        assert_eq!(decode_channels(""), None);
    }

    #[test]
    fn system_selections_persist_as_default_auto_or_fixed() {
        assert_eq!(encode_system_selection(SystemSelection::Default), "default");
        assert_eq!(encode_system_selection(SystemSelection::Automatic), "auto");
        assert_eq!(
            encode_system_selection(SystemSelection::Fixed(96000)),
            "fixed:96000"
        );
        assert_eq!(decode_system_selection("default"), SystemSelection::Default);
        assert_eq!(decode_system_selection("auto"), SystemSelection::Automatic);
        assert_eq!(
            decode_system_selection("fixed:512"),
            SystemSelection::Fixed(512)
        );
    }

    #[test]
    fn malformed_system_selections_decode_to_default() {
        assert_eq!(decode_system_selection("fixed:"), SystemSelection::Default);
        assert_eq!(
            decode_system_selection("fixed:abc"),
            SystemSelection::Default
        );
        assert_eq!(
            decode_system_selection("FIXED:512"),
            SystemSelection::Default
        );
        assert_eq!(decode_system_selection(""), SystemSelection::Default);
    }
}
