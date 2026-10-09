//! Typed accessors for every `APP_SETTINGS` key the Iced UI reads or writes.
//!
//! Both UIs share one database, so the strings stored here are byte-identical
//! to what the Iced UI writes, and the values read back follow the Iced UI's
//! defaults and parse rules (`src/gui/app.rs`, `AudoxidyApp::new`, and
//! `src/gui/audio_center.rs`, the `save_*_settings_to_db` functions). The keys
//! are grouped by domain, one struct per domain:
//!
//! | Domain | Struct | Keys |
//! | --- | --- | --- |
//! | Player | [`PlayerSettings`] | `player_volume`, `last_active_playlist_id` |
//! | Output device | [`DeviceSettings`] | `audio_host`, `audio_device`, `audio_device_id`, `audio_sample_rate`, `audio_bit_depth`, `audio_channels`, `audio_buffer_size`, `audio_system_rate`, `audio_system_quantum` |
//! | Equalizer | [`EqSettings`] | `eq_enabled`, `eq_bands_31`, `preamp_gain`, `eq_band_gains_20`, `eq_band_gains_31`, `eq_selected_preset` |
//! | Equalizer presets | [`PersistedSettings::eq_hidden_builtin_presets`] | `eq_hidden_builtin_presets` (Teksilo only, quirk fix 14) |
//! | Effects | [`DspSettings`] | the 21 `dsp_*` keys |
//! | Downmix | [`DownmixSettings`] | `audio_downmix_{center,lfe,surround}` and their `_enabled` keys |
//! | Volume and mix | [`MixSettings`] | the 19 `vol_*` keys |
//! | Crossfade | [`CrossfadeSettings`] | the 5 `crossfade_*` keys |
//! | VU meter | [`MeterSettings`] | the 5 `meter_*` keys |
//!
//! # Parse rules
//!
//! The Iced UI reads a flag as `value == "1"`: a missing flag takes its
//! default, but a flag holding anything else (garbage included) is `false`.
//! A number that does not parse takes its default. The defaults are the ones
//! the Iced startup code uses, which differ from `AudioState::default()` for
//! `vol_fade_in_ms` (2000, not 1000) and `vol_fade_out_ms` (3000, not 2000).
//!
//! # Writes
//!
//! Every write goes through [`DbWorker::write_coalesced`], one job per key,
//! keyed by the setting key: a burst of slider changes stores only the last
//! value of each key. [`write_entries`] returns one reply for a group of keys.
//! Do not write these keys any other way while a coalesced write may be
//! pending, or the last write might not win.

#![warn(missing_docs)]

use super::db_worker::DbWorker;
use super::error::FrontendError;
use super::reply::Reply;
use crate::audio::dsp::{DspChain, ExpanderMode};
use crate::audio::engine::{AudioSettings, AudioState, BitDepth, ChannelConfig};
use crate::db::Database;
use crate::logic::audio_settings::{self as codec, SystemSelection};
use crate::logic::eq;

/// The `APP_SETTINGS` key names, as the Iced UI spells them.
pub mod keys {
    /// Player volume, `{:.4}` (0.0 to 1.0).
    pub const PLAYER_VOLUME: &str = "player_volume";
    /// Id of the playlist shown last, as digits.
    pub const LAST_ACTIVE_PLAYLIST_ID: &str = "last_active_playlist_id";

    /// Audio host name ("" = system default).
    pub const AUDIO_HOST: &str = "audio_host";
    /// Output device name ("" = none chosen).
    pub const AUDIO_DEVICE: &str = "audio_device";
    /// Stable output device id ("" = unknown).
    pub const AUDIO_DEVICE_ID: &str = "audio_device_id";
    /// Player sample rate: digits or "auto".
    pub const AUDIO_SAMPLE_RATE: &str = "audio_sample_rate";
    /// Output bit depth: "16", "24" or "32".
    pub const AUDIO_BIT_DEPTH: &str = "audio_bit_depth";
    /// Output channel count, as digits.
    pub const AUDIO_CHANNELS: &str = "audio_channels";
    /// Output buffer size: digits or "auto".
    pub const AUDIO_BUFFER_SIZE: &str = "audio_buffer_size";
    /// Sound-server rate: "default", "auto" or "fixed:N".
    pub const AUDIO_SYSTEM_RATE: &str = "audio_system_rate";
    /// Sound-server quantum: "default", "auto" or "fixed:N".
    pub const AUDIO_SYSTEM_QUANTUM: &str = "audio_system_quantum";

    /// Equalizer on ("1") or off ("0").
    pub const EQ_ENABLED: &str = "eq_enabled";
    /// 31-band mode ("1") or 20-band mode ("0").
    pub const EQ_BANDS_31: &str = "eq_bands_31";
    /// Preamplifier gain in dB, `{:.1}`.
    pub const PREAMP_GAIN: &str = "preamp_gain";
    /// The 20 band gains, comma-separated, `{:.1}` each.
    pub const EQ_BAND_GAINS_20: &str = "eq_band_gains_20";
    /// The 31 band gains, comma-separated, `{:.1}` each.
    pub const EQ_BAND_GAINS_31: &str = "eq_band_gains_31";
    /// Name of the selected preset ("" = none).
    pub const EQ_SELECTED_PRESET: &str = "eq_selected_preset";
    /// Built-in presets the user deleted, as a JSON array of names. Written
    /// by the Teksilo UI only (quirk fix 14); the Iced UI ignores it.
    pub const EQ_HIDDEN_BUILTIN_PRESETS: &str = "eq_hidden_builtin_presets";

    /// Sub-bass effect on/off.
    pub const DSP_SUB_BASS_ENABLED: &str = "dsp_sub_bass_enabled";
    /// Mid-bass effect on/off.
    pub const DSP_MID_BASS_ENABLED: &str = "dsp_mid_bass_enabled";
    /// Voice boost on/off.
    pub const DSP_VOICE_BOOST_ENABLED: &str = "dsp_voice_boost_enabled";
    /// Noise gate on/off.
    pub const DSP_NOISE_GATE_ENABLED: &str = "dsp_noise_gate_enabled";
    /// Stereo expander on/off.
    pub const DSP_STEREO_EXPANDER_ENABLED: &str = "dsp_stereo_expander_enabled";
    /// Stereo balance on/off.
    pub const DSP_STEREO_BALANCE_ENABLED: &str = "dsp_stereo_balance_enabled";
    /// Compressor on/off.
    pub const DSP_COMPRESSOR_ENABLED: &str = "dsp_compressor_enabled";
    /// Limiter on/off.
    pub const DSP_LIMITER_ENABLED: &str = "dsp_limiter_enabled";
    /// Reverb on/off.
    pub const DSP_REVERB_ENABLED: &str = "dsp_reverb_enabled";
    /// Sub-bass gain in dB, `{:.1}`.
    pub const DSP_SUB_BASS_GAIN: &str = "dsp_sub_bass_gain";
    /// Mid-bass gain in dB, `{:.1}`.
    pub const DSP_MID_BASS_GAIN: &str = "dsp_mid_bass_gain";
    /// Voice boost gain in dB, `{:.1}`.
    pub const DSP_VOICE_BOOST_GAIN: &str = "dsp_voice_boost_gain";
    /// Noise gate threshold in dB, `{:.1}`.
    pub const DSP_NOISE_GATE_THRESHOLD: &str = "dsp_noise_gate_threshold";
    /// Stereo expander width as a factor (1.0 = 100 %), `{:.1}`.
    pub const DSP_STEREO_EXPANDER_WIDTH: &str = "dsp_stereo_expander_width";
    /// Stereo expander mode: "surround" or "hybrid".
    pub const DSP_STEREO_EXPANDER_MODE: &str = "dsp_stereo_expander_mode";
    /// Stereo balance (-1 left to 1 right), `{:.1}`.
    pub const DSP_STEREO_BALANCE_BALANCE: &str = "dsp_stereo_balance_balance";
    /// Compressor threshold in dB, `{:.1}`.
    pub const DSP_COMPRESSOR_THRESHOLD: &str = "dsp_compressor_threshold";
    /// Compressor intensity as a factor (0 to 1), `{:.1}`.
    pub const DSP_COMPRESSOR_INTENSITY: &str = "dsp_compressor_intensity";
    /// Limiter ceiling in dB, `{:.1}`.
    pub const DSP_LIMITER_CEILING: &str = "dsp_limiter_ceiling";
    /// Reverb wet mix (0 to 1), `{:.2}`.
    pub const DSP_REVERB_WET: &str = "dsp_reverb_wet";
    /// Reverb room size (0 to 1), `{:.2}`.
    pub const DSP_REVERB_ROOM_SIZE: &str = "dsp_reverb_room_size";

    /// Downmix gain of the centre channel, `{:.2}`.
    pub const AUDIO_DOWNMIX_CENTER: &str = "audio_downmix_center";
    /// Downmix gain of the LFE channel, `{:.2}`.
    pub const AUDIO_DOWNMIX_LFE: &str = "audio_downmix_lfe";
    /// Downmix gain of the surround channels, `{:.2}`.
    pub const AUDIO_DOWNMIX_SURROUND: &str = "audio_downmix_surround";
    /// Centre channel downmix on/off.
    pub const AUDIO_DOWNMIX_CENTER_ENABLED: &str = "audio_downmix_center_enabled";
    /// LFE channel downmix on/off.
    pub const AUDIO_DOWNMIX_LFE_ENABLED: &str = "audio_downmix_lfe_enabled";
    /// Surround channels downmix on/off.
    pub const AUDIO_DOWNMIX_SURROUND_ENABLED: &str = "audio_downmix_surround_enabled";

    /// Volume-change group master on/off.
    pub const VOL_FADES_ENABLED: &str = "vol_fades_enabled";
    /// Smooth volume changes on/off.
    pub const VOL_SMOOTH_VOLUME_ENABLED: &str = "vol_smooth_volume_enabled";
    /// Fade-in length in ms, `{:.0}`.
    pub const VOL_FADE_IN_MS: &str = "vol_fade_in_ms";
    /// Fade-out length in ms, `{:.0}`.
    pub const VOL_FADE_OUT_MS: &str = "vol_fade_out_ms";
    /// Fade-in on/off.
    pub const VOL_FADE_IN_ENABLED: &str = "vol_fade_in_enabled";
    /// Fade-out on/off.
    pub const VOL_FADE_OUT_ENABLED: &str = "vol_fade_out_enabled";
    /// Silence removal on/off.
    pub const VOL_SILENCE_ENABLED: &str = "vol_silence_enabled";
    /// Shortest silence removed, in ms, `{:.0}`.
    pub const VOL_SILENCE_DURATION_MS: &str = "vol_silence_duration_ms";
    /// Silence threshold in dB, `{:.2}`.
    pub const VOL_SILENCE_THRESHOLD_DB: &str = "vol_silence_threshold_db";
    /// Trim silences at the start and end of tracks on/off.
    pub const VOL_SILENCE_EDGE_TRIM_ENABLED: &str = "vol_silence_edge_trim_enabled";
    /// Fixed pre-amplifier on/off.
    pub const VOL_RG_FIXED_ENABLED: &str = "vol_rg_fixed_enabled";
    /// Fixed pre-amplifier level in dB, `{:.2}`.
    pub const VOL_RG_FIXED_DB: &str = "vol_rg_fixed_db";
    /// ReplayGain group master on/off.
    pub const VOL_RG_MASTER_ENABLED: &str = "vol_rg_master_enabled";
    /// Track ReplayGain on/off.
    pub const VOL_RG_TRACK_ENABLED: &str = "vol_rg_track_enabled";
    /// Album ReplayGain on/off.
    pub const VOL_RG_ALBUM_ENABLED: &str = "vol_rg_album_enabled";
    /// Real-time loudness analysis on/off.
    pub const VOL_RG_ANALYZE_RT_ENABLED: &str = "vol_rg_analyze_rt_enabled";
    /// Album ReplayGain offset in dB, `{:.2}`.
    pub const VOL_RG_OFFSET_ALBUM_DB: &str = "vol_rg_offset_album_db";
    /// Track ReplayGain offset in dB, `{:.2}`.
    pub const VOL_RG_OFFSET_TRACK_DB: &str = "vol_rg_offset_track_db";
    /// Real-time analysis offset in dB, `{:.2}`.
    pub const VOL_RG_OFFSET_RT_DB: &str = "vol_rg_offset_rt_db";

    /// Crossfade group master on/off.
    pub const CROSSFADE_ENABLED: &str = "crossfade_enabled";
    /// Crossfade on manual track changes on/off.
    pub const CROSSFADE_MANUAL_ENABLED: &str = "crossfade_manual_enabled";
    /// Manual crossfade length in ms, `{:.0}`.
    pub const CROSSFADE_MANUAL_MS: &str = "crossfade_manual_ms";
    /// Crossfade on automatic track changes on/off.
    pub const CROSSFADE_AUTO_ENABLED: &str = "crossfade_auto_enabled";
    /// Automatic crossfade length in ms, `{:.0}`.
    pub const CROSSFADE_AUTO_MS: &str = "crossfade_auto_ms";

    /// Peak hold time in ms, `{:.0}` (saved, never read by the meter).
    pub const METER_HOLD_TIME_MS: &str = "meter_hold_time_ms";
    /// RMS window in ms, `{:.0}` (saved, never read by the meter).
    pub const METER_RMS_WINDOW_MS: &str = "meter_rms_window_ms";
    /// Mid/side display on/off.
    pub const METER_MS_MODE: &str = "meter_ms_mode";
    /// Infinite peak hold on/off.
    pub const METER_INFINITE_HOLD: &str = "meter_infinite_hold";
    /// Meter shown on/off.
    pub const METER_ENABLED: &str = "meter_enabled";

    /// Every key above, grouped by domain.
    pub const ALL: [&str; 74] = [
        PLAYER_VOLUME,
        LAST_ACTIVE_PLAYLIST_ID,
        AUDIO_HOST,
        AUDIO_DEVICE,
        AUDIO_DEVICE_ID,
        AUDIO_SAMPLE_RATE,
        AUDIO_BIT_DEPTH,
        AUDIO_CHANNELS,
        AUDIO_BUFFER_SIZE,
        AUDIO_SYSTEM_RATE,
        AUDIO_SYSTEM_QUANTUM,
        EQ_ENABLED,
        EQ_BANDS_31,
        PREAMP_GAIN,
        EQ_BAND_GAINS_20,
        EQ_BAND_GAINS_31,
        EQ_SELECTED_PRESET,
        EQ_HIDDEN_BUILTIN_PRESETS,
        DSP_SUB_BASS_ENABLED,
        DSP_MID_BASS_ENABLED,
        DSP_VOICE_BOOST_ENABLED,
        DSP_NOISE_GATE_ENABLED,
        DSP_STEREO_EXPANDER_ENABLED,
        DSP_STEREO_BALANCE_ENABLED,
        DSP_COMPRESSOR_ENABLED,
        DSP_LIMITER_ENABLED,
        DSP_REVERB_ENABLED,
        DSP_SUB_BASS_GAIN,
        DSP_MID_BASS_GAIN,
        DSP_VOICE_BOOST_GAIN,
        DSP_NOISE_GATE_THRESHOLD,
        DSP_STEREO_EXPANDER_WIDTH,
        DSP_STEREO_EXPANDER_MODE,
        DSP_STEREO_BALANCE_BALANCE,
        DSP_COMPRESSOR_THRESHOLD,
        DSP_COMPRESSOR_INTENSITY,
        DSP_LIMITER_CEILING,
        DSP_REVERB_WET,
        DSP_REVERB_ROOM_SIZE,
        AUDIO_DOWNMIX_CENTER,
        AUDIO_DOWNMIX_LFE,
        AUDIO_DOWNMIX_SURROUND,
        AUDIO_DOWNMIX_CENTER_ENABLED,
        AUDIO_DOWNMIX_LFE_ENABLED,
        AUDIO_DOWNMIX_SURROUND_ENABLED,
        VOL_FADES_ENABLED,
        VOL_SMOOTH_VOLUME_ENABLED,
        VOL_FADE_IN_MS,
        VOL_FADE_OUT_MS,
        VOL_FADE_IN_ENABLED,
        VOL_FADE_OUT_ENABLED,
        VOL_SILENCE_ENABLED,
        VOL_SILENCE_DURATION_MS,
        VOL_SILENCE_THRESHOLD_DB,
        VOL_SILENCE_EDGE_TRIM_ENABLED,
        VOL_RG_FIXED_ENABLED,
        VOL_RG_FIXED_DB,
        VOL_RG_MASTER_ENABLED,
        VOL_RG_TRACK_ENABLED,
        VOL_RG_ALBUM_ENABLED,
        VOL_RG_ANALYZE_RT_ENABLED,
        VOL_RG_OFFSET_ALBUM_DB,
        VOL_RG_OFFSET_TRACK_DB,
        VOL_RG_OFFSET_RT_DB,
        CROSSFADE_ENABLED,
        CROSSFADE_MANUAL_ENABLED,
        CROSSFADE_MANUAL_MS,
        CROSSFADE_AUTO_ENABLED,
        CROSSFADE_AUTO_MS,
        METER_HOLD_TIME_MS,
        METER_RMS_WINDOW_MS,
        METER_MS_MODE,
        METER_INFINITE_HOLD,
        METER_ENABLED,
    ];
}

use keys::*;

/// One key and the exact string to store under it.
pub type SettingEntry = (&'static str, String);

// --- Encoders (the Iced formats) -------------------------------------------

/// A flag as the Iced UI stores it: "1" or "0".
pub fn encode_flag(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

/// The player volume as stored under `player_volume`: `{:.4}`.
pub fn encode_volume(volume: f32) -> String {
    format!("{volume:.4}")
}

/// A duration in milliseconds as the Iced UI stores it: `{:.0}`.
pub fn encode_ms(ms: f32) -> String {
    format!("{ms:.0}")
}

/// A value with one decimal (`{:.1}`): EQ preamp, DSP gains and thresholds.
pub fn encode_tenths(value: f32) -> String {
    format!("{value:.1}")
}

/// A value with two decimals (`{:.2}`): mix levels, downmix gains, reverb.
pub fn encode_hundredths(value: f32) -> String {
    format!("{value:.2}")
}

/// The stereo expander mode as stored: "surround" or "hybrid".
pub fn encode_expander_mode(mode: ExpanderMode) -> &'static str {
    if mode == ExpanderMode::Surround {
        "surround"
    } else {
        "hybrid"
    }
}

/// Decodes the stereo expander mode the Iced way: "surround" is Surround,
/// anything else Hybrid.
pub fn decode_expander_mode(value: &str) -> ExpanderMode {
    if value == "surround" {
        ExpanderMode::Surround
    } else {
        ExpanderMode::Hybrid
    }
}

/// The hidden built-in presets as stored under `eq_hidden_builtin_presets`: a
/// JSON array of names (names may contain commas).
pub fn encode_hidden_builtin_presets(names: &[String]) -> String {
    serde_json::to_string(names).unwrap_or_else(|_| "[]".to_string())
}

/// Decodes `eq_hidden_builtin_presets`; anything that is not a JSON array of
/// strings is an empty list.
pub fn decode_hidden_builtin_presets(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_default()
}

// --- Raw readers (the Iced parse rules) -------------------------------------

/// A flag: missing is `default`; present is `value == "1"`.
fn read_flag(db: &Database, key: &str, default: bool) -> bool {
    db.get_setting(key).map_or(default, |v| v == "1")
}

/// A number: missing or unparsable is `default`.
fn read_f32(db: &Database, key: &str, default: f32) -> f32 {
    db.get_setting(key)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(default)
}

/// A string where "" means "not set".
fn read_non_empty(db: &Database, key: &str) -> Option<String> {
    db.get_setting(key).filter(|s| !s.is_empty())
}

// --- Player ------------------------------------------------------------------

/// The player keys.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerSettings {
    /// The saved volume, clamped to 0.0..=1.0; `None` when missing or
    /// unparsable (the Iced UI then leaves the engine's volume alone).
    pub volume: Option<f32>,
    /// The playlist shown last; `None` when missing or unparsable.
    pub last_active_playlist_id: Option<i64>,
}

impl PlayerSettings {
    /// Reads the player keys.
    pub fn read(db: &Database) -> Self {
        Self {
            volume: db
                .get_setting(PLAYER_VOLUME)
                .and_then(|v| v.parse::<f32>().ok())
                .map(|v| v.clamp(0.0, 1.0)),
            last_active_playlist_id: db
                .get_setting(LAST_ACTIVE_PLAYLIST_ID)
                .and_then(|v| v.parse::<i64>().ok()),
        }
    }

    /// The stored strings; a `None` field is not written.
    pub fn entries(&self) -> Vec<SettingEntry> {
        let mut entries = Vec::new();
        if let Some(volume) = self.volume {
            entries.push((PLAYER_VOLUME, encode_volume(volume)));
        }
        if let Some(id) = self.last_active_playlist_id {
            entries.push((LAST_ACTIVE_PLAYLIST_ID, id.to_string()));
        }
        entries
    }
}

/// Stores the player volume as `{:.4}`, the way the Iced `VolumeChanged` and
/// `PlayerScroll` handlers do.
pub fn write_player_volume(db: &DbWorker, volume: f32) -> Reply<()> {
    write_entries(db, vec![(PLAYER_VOLUME, encode_volume(volume))])
}

/// Stores the id of the playlist shown last.
pub fn write_last_active_playlist_id(db: &DbWorker, playlist_id: i64) -> Reply<()> {
    write_entries(db, vec![(LAST_ACTIVE_PLAYLIST_ID, playlist_id.to_string())])
}

// --- Output device ----------------------------------------------------------

/// The output device and sound-server keys.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSettings {
    /// The audio host; `None` when missing or "".
    pub host: Option<String>,
    /// The output device name; `None` when missing or "".
    pub device: Option<String>,
    /// The stable output device id; `None` when missing or "".
    pub device_id: Option<String>,
    /// The player sample rate; `None` for "auto", missing or unparsable.
    pub sample_rate: Option<u32>,
    /// The output bit depth; `None` when missing or unknown (the engine then
    /// picks the device's format).
    pub bit_depth: Option<BitDepth>,
    /// The output channel count; 2 when missing or unparsable.
    pub channels: u16,
    /// The output buffer size; `None` for "auto", missing or unparsable.
    pub buffer_size: Option<u32>,
    /// The sound-server rate; Default when missing or malformed.
    pub system_rate: SystemSelection<u32>,
    /// The sound-server quantum; Default when missing or malformed.
    pub system_quantum: SystemSelection<u32>,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            host: None,
            device: None,
            device_id: None,
            sample_rate: None,
            bit_depth: None,
            channels: codec::DEFAULT_CHANNELS,
            buffer_size: None,
            system_rate: SystemSelection::Default,
            system_quantum: SystemSelection::Default,
        }
    }
}

impl DeviceSettings {
    /// Reads the device keys.
    pub fn read(db: &Database) -> Self {
        let selection = |key| {
            db.get_setting(key).map_or(SystemSelection::Default, |s| {
                codec::decode_system_selection(&s)
            })
        };
        Self {
            host: read_non_empty(db, AUDIO_HOST),
            device: read_non_empty(db, AUDIO_DEVICE),
            device_id: read_non_empty(db, AUDIO_DEVICE_ID),
            sample_rate: db
                .get_setting(AUDIO_SAMPLE_RATE)
                .and_then(|s| codec::decode_auto_u32(&s)),
            bit_depth: db
                .get_setting(AUDIO_BIT_DEPTH)
                .and_then(|s| codec::decode_bit_depth(&s)),
            channels: db
                .get_setting(AUDIO_CHANNELS)
                .and_then(|s| codec::decode_channels(&s))
                .unwrap_or(codec::DEFAULT_CHANNELS),
            buffer_size: db
                .get_setting(AUDIO_BUFFER_SIZE)
                .and_then(|s| codec::decode_auto_u32(&s)),
            system_rate: selection(AUDIO_SYSTEM_RATE),
            system_quantum: selection(AUDIO_SYSTEM_QUANTUM),
        }
    }

    /// Whether an output configuration was ever saved: the Iced startup
    /// rebuilds the stream only then (`audio_channels` alone does not count).
    pub fn has_saved_output(&self) -> bool {
        self.host.is_some()
            || self.device.is_some()
            || self.device_id.is_some()
            || self.sample_rate.is_some()
            || self.bit_depth.is_some()
            || self.buffer_size.is_some()
    }

    /// The stream configuration the Iced startup applies.
    pub fn to_audio_settings(&self) -> AudioSettings {
        AudioSettings {
            host_id: self.host.clone(),
            device_name: self.device.clone(),
            device_id: self.device_id.clone(),
            sample_rate: self.sample_rate,
            bit_depth: self.bit_depth,
            channels: ChannelConfig::Manual(self.channels),
            buffer_size: self.buffer_size,
        }
    }

    /// The stored strings, as the Iced `ApplySettings` handler writes them. A
    /// missing bit depth is not written (Apply always has one).
    pub fn entries(&self) -> Vec<SettingEntry> {
        let mut entries = vec![
            (AUDIO_HOST, self.host.clone().unwrap_or_default()),
            (AUDIO_DEVICE, self.device.clone().unwrap_or_default()),
            (AUDIO_DEVICE_ID, self.device_id.clone().unwrap_or_default()),
            (AUDIO_SAMPLE_RATE, codec::encode_auto_u32(self.sample_rate)),
        ];
        if let Some(depth) = self.bit_depth {
            entries.push((AUDIO_BIT_DEPTH, codec::encode_bit_depth(depth).to_string()));
        }
        entries.extend([
            (AUDIO_CHANNELS, codec::encode_channels(self.channels)),
            (AUDIO_BUFFER_SIZE, codec::encode_auto_u32(self.buffer_size)),
            (
                AUDIO_SYSTEM_RATE,
                codec::encode_system_selection(self.system_rate),
            ),
            (
                AUDIO_SYSTEM_QUANTUM,
                codec::encode_system_selection(self.system_quantum),
            ),
        ]);
        entries
    }
}

/// Stores the stable id of the output device (the startup backfill when only
/// the device name was saved).
pub fn write_audio_device_id(db: &DbWorker, device_id: &str) -> Reply<()> {
    write_entries(db, vec![(AUDIO_DEVICE_ID, device_id.to_string())])
}

// --- Equalizer ----------------------------------------------------------------

/// The equalizer keys the Iced UI writes (`save_eq_settings_to_db`).
#[derive(Debug, Clone, PartialEq)]
pub struct EqSettings {
    /// Equalizer on; false when missing.
    pub enabled: bool,
    /// 31-band mode; false (20 bands) when missing.
    pub bands_31: bool,
    /// Preamp gain in dB; 0.0 when missing or unparsable.
    pub preamp_gain: f32,
    /// The 20 band gains; flat when missing or not exactly 20 values.
    pub band_gains_20: Vec<f32>,
    /// The 31 band gains; flat when missing or not exactly 31 values.
    pub band_gains_31: Vec<f32>,
    /// The selected preset; `None` when missing, "" or "Default" (the Iced UI
    /// shows no selection for those).
    pub selected_preset: Option<String>,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            bands_31: false,
            preamp_gain: 0.0,
            band_gains_20: vec![0.0; 20],
            band_gains_31: vec![0.0; 31],
            selected_preset: None,
        }
    }
}

impl EqSettings {
    /// Reads the equalizer keys.
    pub fn read(db: &Database) -> Self {
        Self {
            enabled: read_flag(db, EQ_ENABLED, false),
            bands_31: read_flag(db, EQ_BANDS_31, false),
            preamp_gain: read_f32(db, PREAMP_GAIN, 0.0),
            band_gains_20: db
                .get_setting(EQ_BAND_GAINS_20)
                .and_then(|v| eq::decode_band_gains(&v, 20))
                .unwrap_or_else(|| vec![0.0; 20]),
            band_gains_31: db
                .get_setting(EQ_BAND_GAINS_31)
                .and_then(|v| eq::decode_band_gains(&v, 31))
                .unwrap_or_else(|| vec![0.0; 31]),
            selected_preset: db
                .get_setting(EQ_SELECTED_PRESET)
                .filter(|name| !name.is_empty() && name != eq::DEFAULT_PRESET_NAME),
        }
    }

    /// The stored strings: the six keys, in the order the Iced UI writes them.
    /// A `None` preset is stored as "".
    pub fn entries(&self) -> Vec<SettingEntry> {
        vec![
            (EQ_ENABLED, encode_flag(self.enabled).to_string()),
            (EQ_BANDS_31, encode_flag(self.bands_31).to_string()),
            (PREAMP_GAIN, eq::encode_preamp_gain(self.preamp_gain)),
            (
                EQ_SELECTED_PRESET,
                self.selected_preset.clone().unwrap_or_default(),
            ),
            (EQ_BAND_GAINS_20, eq::encode_band_gains(&self.band_gains_20)),
            (EQ_BAND_GAINS_31, eq::encode_band_gains(&self.band_gains_31)),
        ]
    }
}

/// Stores the hidden built-in presets (quirk fix 14).
pub fn write_hidden_builtin_presets(db: &DbWorker, names: &[String]) -> Reply<()> {
    write_entries(
        db,
        vec![(
            EQ_HIDDEN_BUILTIN_PRESETS,
            encode_hidden_builtin_presets(names),
        )],
    )
}

/// Reads the hidden built-in presets; empty when missing or malformed.
pub fn read_hidden_builtin_presets(db: &Database) -> Vec<String> {
    db.get_setting(EQ_HIDDEN_BUILTIN_PRESETS)
        .map(|v| decode_hidden_builtin_presets(&v))
        .unwrap_or_default()
}

// --- Effects (DSP) ------------------------------------------------------------

/// One `dsp_*` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DspKey {
    /// `dsp_sub_bass_enabled`.
    SubBassEnabled,
    /// `dsp_mid_bass_enabled`.
    MidBassEnabled,
    /// `dsp_voice_boost_enabled`.
    VoiceBoostEnabled,
    /// `dsp_noise_gate_enabled`.
    NoiseGateEnabled,
    /// `dsp_stereo_expander_enabled`.
    StereoExpanderEnabled,
    /// `dsp_stereo_balance_enabled`.
    StereoBalanceEnabled,
    /// `dsp_compressor_enabled`.
    CompressorEnabled,
    /// `dsp_limiter_enabled`.
    LimiterEnabled,
    /// `dsp_reverb_enabled`.
    ReverbEnabled,
    /// `dsp_sub_bass_gain`.
    SubBassGain,
    /// `dsp_mid_bass_gain`.
    MidBassGain,
    /// `dsp_voice_boost_gain`.
    VoiceBoostGain,
    /// `dsp_noise_gate_threshold`.
    NoiseGateThreshold,
    /// `dsp_stereo_expander_width`.
    StereoExpanderWidth,
    /// `dsp_stereo_expander_mode`.
    StereoExpanderMode,
    /// `dsp_stereo_balance_balance`.
    StereoBalance,
    /// `dsp_compressor_threshold`.
    CompressorThreshold,
    /// `dsp_compressor_intensity`.
    CompressorIntensity,
    /// `dsp_limiter_ceiling`.
    LimiterCeiling,
    /// `dsp_reverb_wet`.
    ReverbWet,
    /// `dsp_reverb_room_size`.
    ReverbRoomSize,
}

impl DspKey {
    /// Every DSP key, in the order the Iced UI writes them.
    pub const ALL: [DspKey; 21] = [
        DspKey::SubBassEnabled,
        DspKey::MidBassEnabled,
        DspKey::VoiceBoostEnabled,
        DspKey::NoiseGateEnabled,
        DspKey::StereoExpanderEnabled,
        DspKey::StereoBalanceEnabled,
        DspKey::CompressorEnabled,
        DspKey::LimiterEnabled,
        DspKey::ReverbEnabled,
        DspKey::SubBassGain,
        DspKey::MidBassGain,
        DspKey::VoiceBoostGain,
        DspKey::NoiseGateThreshold,
        DspKey::StereoExpanderWidth,
        DspKey::StereoExpanderMode,
        DspKey::StereoBalance,
        DspKey::CompressorThreshold,
        DspKey::CompressorIntensity,
        DspKey::LimiterCeiling,
        DspKey::ReverbWet,
        DspKey::ReverbRoomSize,
    ];

    /// The `APP_SETTINGS` key.
    pub fn key(self) -> &'static str {
        match self {
            DspKey::SubBassEnabled => DSP_SUB_BASS_ENABLED,
            DspKey::MidBassEnabled => DSP_MID_BASS_ENABLED,
            DspKey::VoiceBoostEnabled => DSP_VOICE_BOOST_ENABLED,
            DspKey::NoiseGateEnabled => DSP_NOISE_GATE_ENABLED,
            DspKey::StereoExpanderEnabled => DSP_STEREO_EXPANDER_ENABLED,
            DspKey::StereoBalanceEnabled => DSP_STEREO_BALANCE_ENABLED,
            DspKey::CompressorEnabled => DSP_COMPRESSOR_ENABLED,
            DspKey::LimiterEnabled => DSP_LIMITER_ENABLED,
            DspKey::ReverbEnabled => DSP_REVERB_ENABLED,
            DspKey::SubBassGain => DSP_SUB_BASS_GAIN,
            DspKey::MidBassGain => DSP_MID_BASS_GAIN,
            DspKey::VoiceBoostGain => DSP_VOICE_BOOST_GAIN,
            DspKey::NoiseGateThreshold => DSP_NOISE_GATE_THRESHOLD,
            DspKey::StereoExpanderWidth => DSP_STEREO_EXPANDER_WIDTH,
            DspKey::StereoExpanderMode => DSP_STEREO_EXPANDER_MODE,
            DspKey::StereoBalance => DSP_STEREO_BALANCE_BALANCE,
            DspKey::CompressorThreshold => DSP_COMPRESSOR_THRESHOLD,
            DspKey::CompressorIntensity => DSP_COMPRESSOR_INTENSITY,
            DspKey::LimiterCeiling => DSP_LIMITER_CEILING,
            DspKey::ReverbWet => DSP_REVERB_WET,
            DspKey::ReverbRoomSize => DSP_REVERB_ROOM_SIZE,
        }
    }
}

/// The effect keys. The defaults are the DSP chain's own defaults, which the
/// Iced startup keeps when a key is missing.
#[derive(Debug, Clone, PartialEq)]
pub struct DspSettings {
    /// Sub-bass on.
    pub sub_bass_enabled: bool,
    /// Mid-bass on.
    pub mid_bass_enabled: bool,
    /// Voice boost on.
    pub voice_boost_enabled: bool,
    /// Noise gate on.
    pub noise_gate_enabled: bool,
    /// Stereo expander on.
    pub stereo_expander_enabled: bool,
    /// Stereo balance on.
    pub stereo_balance_enabled: bool,
    /// Compressor on.
    pub compressor_enabled: bool,
    /// Limiter on.
    pub limiter_enabled: bool,
    /// Reverb on.
    pub reverb_enabled: bool,
    /// Sub-bass gain in dB (default 0).
    pub sub_bass_gain: f32,
    /// Mid-bass gain in dB (default 0).
    pub mid_bass_gain: f32,
    /// Voice boost gain in dB (default 0).
    pub voice_boost_gain: f32,
    /// Noise gate threshold in dB (default -60).
    pub noise_gate_threshold: f32,
    /// Stereo expander width as a factor (default 1.0).
    pub stereo_expander_width: f32,
    /// Stereo expander mode (default Hybrid).
    pub stereo_expander_mode: ExpanderMode,
    /// Stereo balance (default 0).
    pub stereo_balance: f32,
    /// Compressor threshold in dB (default -3).
    pub compressor_threshold: f32,
    /// Compressor intensity as a factor (default 0.5).
    pub compressor_intensity: f32,
    /// Limiter ceiling in dB (default -1).
    pub limiter_ceiling: f32,
    /// Reverb wet mix (default 0.5).
    pub reverb_wet: f32,
    /// Reverb room size (default 0.5).
    pub reverb_room_size: f32,
}

impl Default for DspSettings {
    fn default() -> Self {
        Self::from_chain(&DspChain::default())
    }
}

impl DspSettings {
    /// Copies the persisted fields out of a DSP chain.
    pub fn from_chain(dsp: &DspChain) -> Self {
        Self {
            sub_bass_enabled: dsp.sub_bass.enabled,
            mid_bass_enabled: dsp.mid_bass.enabled,
            voice_boost_enabled: dsp.voice_boost.enabled,
            noise_gate_enabled: dsp.noise_gate.enabled,
            stereo_expander_enabled: dsp.stereo_expander.enabled,
            stereo_balance_enabled: dsp.stereo_balance.enabled,
            compressor_enabled: dsp.compressor.enabled,
            limiter_enabled: dsp.limiter.enabled,
            reverb_enabled: dsp.reverb.enabled,
            sub_bass_gain: dsp.sub_bass.gain,
            mid_bass_gain: dsp.mid_bass.gain,
            voice_boost_gain: dsp.voice_boost.gain,
            noise_gate_threshold: dsp.noise_gate.threshold,
            stereo_expander_width: dsp.stereo_expander.width,
            stereo_expander_mode: dsp.stereo_expander.mode,
            stereo_balance: dsp.stereo_balance.balance,
            compressor_threshold: dsp.compressor.threshold,
            compressor_intensity: dsp.compressor.intensity,
            limiter_ceiling: dsp.limiter.ceiling,
            reverb_wet: dsp.reverb.wet,
            reverb_room_size: dsp.reverb.room_size,
        }
    }

    /// Writes the fields into a DSP chain the way the Iced startup does: plain
    /// field assignments. The compressor's derived parameters and the reverb's
    /// room parameters are recomputed by the engine at the next stream build,
    /// as in Iced.
    pub fn apply_to(&self, dsp: &mut DspChain) {
        dsp.sub_bass.enabled = self.sub_bass_enabled;
        dsp.mid_bass.enabled = self.mid_bass_enabled;
        dsp.voice_boost.enabled = self.voice_boost_enabled;
        dsp.noise_gate.enabled = self.noise_gate_enabled;
        dsp.stereo_expander.enabled = self.stereo_expander_enabled;
        dsp.stereo_balance.enabled = self.stereo_balance_enabled;
        dsp.compressor.enabled = self.compressor_enabled;
        dsp.limiter.enabled = self.limiter_enabled;
        dsp.reverb.enabled = self.reverb_enabled;
        dsp.sub_bass.gain = self.sub_bass_gain;
        dsp.mid_bass.gain = self.mid_bass_gain;
        dsp.voice_boost.gain = self.voice_boost_gain;
        dsp.noise_gate.threshold = self.noise_gate_threshold;
        dsp.stereo_expander.width = self.stereo_expander_width;
        dsp.stereo_expander.mode = self.stereo_expander_mode;
        dsp.stereo_balance.balance = self.stereo_balance;
        dsp.compressor.threshold = self.compressor_threshold;
        dsp.compressor.intensity = self.compressor_intensity;
        dsp.limiter.ceiling = self.limiter_ceiling;
        dsp.reverb.wet = self.reverb_wet;
        dsp.reverb.room_size = self.reverb_room_size;
    }

    /// Reads the effect keys.
    pub fn read(db: &Database) -> Self {
        let d = Self::default();
        Self {
            sub_bass_enabled: read_flag(db, DSP_SUB_BASS_ENABLED, d.sub_bass_enabled),
            mid_bass_enabled: read_flag(db, DSP_MID_BASS_ENABLED, d.mid_bass_enabled),
            voice_boost_enabled: read_flag(db, DSP_VOICE_BOOST_ENABLED, d.voice_boost_enabled),
            noise_gate_enabled: read_flag(db, DSP_NOISE_GATE_ENABLED, d.noise_gate_enabled),
            stereo_expander_enabled: read_flag(
                db,
                DSP_STEREO_EXPANDER_ENABLED,
                d.stereo_expander_enabled,
            ),
            stereo_balance_enabled: read_flag(
                db,
                DSP_STEREO_BALANCE_ENABLED,
                d.stereo_balance_enabled,
            ),
            compressor_enabled: read_flag(db, DSP_COMPRESSOR_ENABLED, d.compressor_enabled),
            limiter_enabled: read_flag(db, DSP_LIMITER_ENABLED, d.limiter_enabled),
            reverb_enabled: read_flag(db, DSP_REVERB_ENABLED, d.reverb_enabled),
            sub_bass_gain: read_f32(db, DSP_SUB_BASS_GAIN, d.sub_bass_gain),
            mid_bass_gain: read_f32(db, DSP_MID_BASS_GAIN, d.mid_bass_gain),
            voice_boost_gain: read_f32(db, DSP_VOICE_BOOST_GAIN, d.voice_boost_gain),
            noise_gate_threshold: read_f32(db, DSP_NOISE_GATE_THRESHOLD, d.noise_gate_threshold),
            stereo_expander_width: read_f32(db, DSP_STEREO_EXPANDER_WIDTH, d.stereo_expander_width),
            stereo_expander_mode: db
                .get_setting(DSP_STEREO_EXPANDER_MODE)
                .map_or(d.stereo_expander_mode, |v| decode_expander_mode(&v)),
            stereo_balance: read_f32(db, DSP_STEREO_BALANCE_BALANCE, d.stereo_balance),
            compressor_threshold: read_f32(db, DSP_COMPRESSOR_THRESHOLD, d.compressor_threshold),
            compressor_intensity: read_f32(db, DSP_COMPRESSOR_INTENSITY, d.compressor_intensity),
            limiter_ceiling: read_f32(db, DSP_LIMITER_CEILING, d.limiter_ceiling),
            reverb_wet: read_f32(db, DSP_REVERB_WET, d.reverb_wet),
            reverb_room_size: read_f32(db, DSP_REVERB_ROOM_SIZE, d.reverb_room_size),
        }
    }

    /// The stored string of one key, in `save_dsp_settings_to_db`'s format.
    pub fn entry(&self, key: DspKey) -> SettingEntry {
        let value = match key {
            DspKey::SubBassEnabled => encode_flag(self.sub_bass_enabled).to_string(),
            DspKey::MidBassEnabled => encode_flag(self.mid_bass_enabled).to_string(),
            DspKey::VoiceBoostEnabled => encode_flag(self.voice_boost_enabled).to_string(),
            DspKey::NoiseGateEnabled => encode_flag(self.noise_gate_enabled).to_string(),
            DspKey::StereoExpanderEnabled => encode_flag(self.stereo_expander_enabled).to_string(),
            DspKey::StereoBalanceEnabled => encode_flag(self.stereo_balance_enabled).to_string(),
            DspKey::CompressorEnabled => encode_flag(self.compressor_enabled).to_string(),
            DspKey::LimiterEnabled => encode_flag(self.limiter_enabled).to_string(),
            DspKey::ReverbEnabled => encode_flag(self.reverb_enabled).to_string(),
            DspKey::SubBassGain => encode_tenths(self.sub_bass_gain),
            DspKey::MidBassGain => encode_tenths(self.mid_bass_gain),
            DspKey::VoiceBoostGain => encode_tenths(self.voice_boost_gain),
            DspKey::NoiseGateThreshold => encode_tenths(self.noise_gate_threshold),
            DspKey::StereoExpanderWidth => encode_tenths(self.stereo_expander_width),
            DspKey::StereoExpanderMode => {
                encode_expander_mode(self.stereo_expander_mode).to_string()
            }
            DspKey::StereoBalance => encode_tenths(self.stereo_balance),
            DspKey::CompressorThreshold => encode_tenths(self.compressor_threshold),
            DspKey::CompressorIntensity => encode_tenths(self.compressor_intensity),
            DspKey::LimiterCeiling => encode_tenths(self.limiter_ceiling),
            DspKey::ReverbWet => encode_hundredths(self.reverb_wet),
            DspKey::ReverbRoomSize => encode_hundredths(self.reverb_room_size),
        };
        (key.key(), value)
    }

    /// The stored strings of every key.
    pub fn entries(&self) -> Vec<SettingEntry> {
        DspKey::ALL.iter().map(|&key| self.entry(key)).collect()
    }
}

// --- Downmix ------------------------------------------------------------------

/// A channel group the downmix settings apply to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DownmixChannel {
    /// The centre channel.
    Center,
    /// The LFE (subwoofer) channel.
    Lfe,
    /// The surround channels (SL/SR, SBL/SBR).
    Surround,
}

impl DownmixChannel {
    /// The three channel groups.
    pub const ALL: [DownmixChannel; 3] = [
        DownmixChannel::Center,
        DownmixChannel::Lfe,
        DownmixChannel::Surround,
    ];

    /// The key of the channel's mix gain.
    pub fn mix_key(self) -> &'static str {
        match self {
            DownmixChannel::Center => AUDIO_DOWNMIX_CENTER,
            DownmixChannel::Lfe => AUDIO_DOWNMIX_LFE,
            DownmixChannel::Surround => AUDIO_DOWNMIX_SURROUND,
        }
    }

    /// The key of the channel's on/off flag.
    pub fn enabled_key(self) -> &'static str {
        match self {
            DownmixChannel::Center => AUDIO_DOWNMIX_CENTER_ENABLED,
            DownmixChannel::Lfe => AUDIO_DOWNMIX_LFE_ENABLED,
            DownmixChannel::Surround => AUDIO_DOWNMIX_SURROUND_ENABLED,
        }
    }
}

/// The downmix keys; the defaults are `AudioState::default()`'s.
#[derive(Debug, Clone, PartialEq)]
pub struct DownmixSettings {
    /// Centre mix gain (default 0.76).
    pub center: f32,
    /// LFE mix gain (default 0.66).
    pub lfe: f32,
    /// Surround mix gain (default 0.74).
    pub surround: f32,
    /// Centre downmix on.
    pub center_enabled: bool,
    /// LFE downmix on.
    pub lfe_enabled: bool,
    /// Surround downmix on.
    pub surround_enabled: bool,
}

impl Default for DownmixSettings {
    fn default() -> Self {
        Self::from_state(&AudioState::default())
    }
}

impl DownmixSettings {
    /// Copies the downmix fields out of the engine state.
    pub fn from_state(state: &AudioState) -> Self {
        Self {
            center: state.downmix_center,
            lfe: state.downmix_lfe,
            surround: state.downmix_surround,
            center_enabled: state.downmix_center_enabled,
            lfe_enabled: state.downmix_lfe_enabled,
            surround_enabled: state.downmix_surround_enabled,
        }
    }

    /// Writes the downmix fields into the engine state.
    pub fn apply_to(&self, state: &mut AudioState) {
        state.downmix_center = self.center;
        state.downmix_lfe = self.lfe;
        state.downmix_surround = self.surround;
        state.downmix_center_enabled = self.center_enabled;
        state.downmix_lfe_enabled = self.lfe_enabled;
        state.downmix_surround_enabled = self.surround_enabled;
    }

    /// Reads the downmix keys.
    pub fn read(db: &Database) -> Self {
        let d = Self::default();
        Self {
            center: read_f32(db, AUDIO_DOWNMIX_CENTER, d.center),
            lfe: read_f32(db, AUDIO_DOWNMIX_LFE, d.lfe),
            surround: read_f32(db, AUDIO_DOWNMIX_SURROUND, d.surround),
            center_enabled: read_flag(db, AUDIO_DOWNMIX_CENTER_ENABLED, d.center_enabled),
            lfe_enabled: read_flag(db, AUDIO_DOWNMIX_LFE_ENABLED, d.lfe_enabled),
            surround_enabled: read_flag(db, AUDIO_DOWNMIX_SURROUND_ENABLED, d.surround_enabled),
        }
    }

    /// The mix gain of `channel`.
    pub fn mix(&self, channel: DownmixChannel) -> f32 {
        match channel {
            DownmixChannel::Center => self.center,
            DownmixChannel::Lfe => self.lfe,
            DownmixChannel::Surround => self.surround,
        }
    }

    /// Whether `channel` is downmixed.
    pub fn enabled(&self, channel: DownmixChannel) -> bool {
        match channel {
            DownmixChannel::Center => self.center_enabled,
            DownmixChannel::Lfe => self.lfe_enabled,
            DownmixChannel::Surround => self.surround_enabled,
        }
    }

    /// The stored mix gain of `channel` (`{:.2}`).
    pub fn mix_entry(&self, channel: DownmixChannel) -> SettingEntry {
        (channel.mix_key(), encode_hundredths(self.mix(channel)))
    }

    /// The stored flag of `channel`.
    pub fn enabled_entry(&self, channel: DownmixChannel) -> SettingEntry {
        (
            channel.enabled_key(),
            encode_flag(self.enabled(channel)).to_string(),
        )
    }

    /// The stored strings of the six keys, in the Iced order (flags first).
    pub fn entries(&self) -> Vec<SettingEntry> {
        let flags = DownmixChannel::ALL.map(|c| self.enabled_entry(c));
        let mixes = DownmixChannel::ALL.map(|c| self.mix_entry(c));
        flags.into_iter().chain(mixes).collect()
    }
}

// --- Volume and mix -------------------------------------------------------------

/// The 19 `vol_*` keys (fades, silence removal, fixed pre-amp, ReplayGain).
#[derive(Debug, Clone, PartialEq)]
pub struct MixSettings {
    /// Volume-change group master (default off).
    pub fades_enabled: bool,
    /// Smooth volume changes (default off).
    pub smooth_volume_enabled: bool,
    /// Fade-in length in ms (default 2000 when the key is missing).
    pub fade_in_ms: f32,
    /// Fade-out length in ms (default 3000 when the key is missing).
    pub fade_out_ms: f32,
    /// Fade-in on (default off).
    pub fade_in_enabled: bool,
    /// Fade-out on (default off).
    pub fade_out_enabled: bool,
    /// Silence removal on (default on).
    pub silence_enabled: bool,
    /// Shortest silence removed, in ms (default 1000).
    pub silence_duration_ms: f32,
    /// Silence threshold in dB (default -47).
    pub silence_threshold_db: f32,
    /// Trim start and end silences (default on).
    pub silence_edge_trim_enabled: bool,
    /// Fixed pre-amp on (default off).
    pub rg_fixed_enabled: bool,
    /// Fixed pre-amp level in dB (default 0).
    pub rg_fixed_db: f32,
    /// ReplayGain group master (default on).
    pub rg_master_enabled: bool,
    /// Track ReplayGain (default on).
    pub rg_track_enabled: bool,
    /// Album ReplayGain (default on).
    pub rg_album_enabled: bool,
    /// Real-time analysis (default on).
    pub rg_analyze_rt_enabled: bool,
    /// Album offset in dB (default 0).
    pub rg_offset_album_db: f32,
    /// Track offset in dB (default 0).
    pub rg_offset_track_db: f32,
    /// Real-time analysis offset in dB (default 0).
    pub rg_offset_rt_db: f32,
}

impl Default for MixSettings {
    /// The Iced startup defaults (`src/gui/app.rs`, `AudoxidyApp::new`).
    fn default() -> Self {
        Self {
            fades_enabled: false,
            smooth_volume_enabled: false,
            fade_in_ms: 2000.0,
            fade_out_ms: 3000.0,
            fade_in_enabled: false,
            fade_out_enabled: false,
            silence_enabled: true,
            silence_duration_ms: 1000.0,
            silence_threshold_db: -47.0,
            silence_edge_trim_enabled: true,
            rg_fixed_enabled: false,
            rg_fixed_db: 0.0,
            rg_master_enabled: true,
            rg_track_enabled: true,
            rg_album_enabled: true,
            rg_analyze_rt_enabled: true,
            rg_offset_album_db: 0.0,
            rg_offset_track_db: 0.0,
            rg_offset_rt_db: 0.0,
        }
    }
}

impl MixSettings {
    /// Copies the fields out of the engine state.
    pub fn from_state(s: &AudioState) -> Self {
        Self {
            fades_enabled: s.fades_enabled,
            smooth_volume_enabled: s.smooth_volume_enabled,
            fade_in_ms: s.fade_in_ms,
            fade_out_ms: s.fade_out_ms,
            fade_in_enabled: s.fade_in_enabled,
            fade_out_enabled: s.fade_out_enabled,
            silence_enabled: s.silence_enabled,
            silence_duration_ms: s.silence_duration_ms,
            silence_threshold_db: s.silence_threshold_db,
            silence_edge_trim_enabled: s.silence_edge_trim_enabled,
            rg_fixed_enabled: s.rg_fixed_enabled,
            rg_fixed_db: s.rg_fixed_db,
            rg_master_enabled: s.rg_master_enabled,
            rg_track_enabled: s.replay_gain_track_enabled,
            rg_album_enabled: s.replay_gain_album_enabled,
            rg_analyze_rt_enabled: s.rg_analyze_rt_enabled,
            rg_offset_album_db: s.rg_offset_album_db,
            rg_offset_track_db: s.rg_offset_track_db,
            rg_offset_rt_db: s.rg_offset_rt_db,
        }
    }

    /// Writes the fields into the engine state.
    pub fn apply_to(&self, s: &mut AudioState) {
        s.fades_enabled = self.fades_enabled;
        s.smooth_volume_enabled = self.smooth_volume_enabled;
        s.fade_in_ms = self.fade_in_ms;
        s.fade_out_ms = self.fade_out_ms;
        s.fade_in_enabled = self.fade_in_enabled;
        s.fade_out_enabled = self.fade_out_enabled;
        s.silence_enabled = self.silence_enabled;
        s.silence_duration_ms = self.silence_duration_ms;
        s.silence_threshold_db = self.silence_threshold_db;
        s.silence_edge_trim_enabled = self.silence_edge_trim_enabled;
        s.rg_fixed_enabled = self.rg_fixed_enabled;
        s.rg_fixed_db = self.rg_fixed_db;
        s.rg_master_enabled = self.rg_master_enabled;
        s.replay_gain_track_enabled = self.rg_track_enabled;
        s.replay_gain_album_enabled = self.rg_album_enabled;
        s.rg_analyze_rt_enabled = self.rg_analyze_rt_enabled;
        s.rg_offset_album_db = self.rg_offset_album_db;
        s.rg_offset_track_db = self.rg_offset_track_db;
        s.rg_offset_rt_db = self.rg_offset_rt_db;
    }

    /// Reads the `vol_*` keys.
    pub fn read(db: &Database) -> Self {
        let d = Self::default();
        Self {
            fades_enabled: read_flag(db, VOL_FADES_ENABLED, d.fades_enabled),
            smooth_volume_enabled: read_flag(
                db,
                VOL_SMOOTH_VOLUME_ENABLED,
                d.smooth_volume_enabled,
            ),
            fade_in_ms: read_f32(db, VOL_FADE_IN_MS, d.fade_in_ms),
            fade_out_ms: read_f32(db, VOL_FADE_OUT_MS, d.fade_out_ms),
            fade_in_enabled: read_flag(db, VOL_FADE_IN_ENABLED, d.fade_in_enabled),
            fade_out_enabled: read_flag(db, VOL_FADE_OUT_ENABLED, d.fade_out_enabled),
            silence_enabled: read_flag(db, VOL_SILENCE_ENABLED, d.silence_enabled),
            silence_duration_ms: read_f32(db, VOL_SILENCE_DURATION_MS, d.silence_duration_ms),
            silence_threshold_db: read_f32(db, VOL_SILENCE_THRESHOLD_DB, d.silence_threshold_db),
            silence_edge_trim_enabled: read_flag(
                db,
                VOL_SILENCE_EDGE_TRIM_ENABLED,
                d.silence_edge_trim_enabled,
            ),
            rg_fixed_enabled: read_flag(db, VOL_RG_FIXED_ENABLED, d.rg_fixed_enabled),
            rg_fixed_db: read_f32(db, VOL_RG_FIXED_DB, d.rg_fixed_db),
            rg_master_enabled: read_flag(db, VOL_RG_MASTER_ENABLED, d.rg_master_enabled),
            rg_track_enabled: read_flag(db, VOL_RG_TRACK_ENABLED, d.rg_track_enabled),
            rg_album_enabled: read_flag(db, VOL_RG_ALBUM_ENABLED, d.rg_album_enabled),
            rg_analyze_rt_enabled: read_flag(
                db,
                VOL_RG_ANALYZE_RT_ENABLED,
                d.rg_analyze_rt_enabled,
            ),
            rg_offset_album_db: read_f32(db, VOL_RG_OFFSET_ALBUM_DB, d.rg_offset_album_db),
            rg_offset_track_db: read_f32(db, VOL_RG_OFFSET_TRACK_DB, d.rg_offset_track_db),
            rg_offset_rt_db: read_f32(db, VOL_RG_OFFSET_RT_DB, d.rg_offset_rt_db),
        }
    }

    /// The stored strings of the 19 keys, in `save_volumen_settings_to_db`'s
    /// order and format.
    pub fn entries(&self) -> Vec<SettingEntry> {
        let flag = |v: bool| encode_flag(v).to_string();
        vec![
            (VOL_FADES_ENABLED, flag(self.fades_enabled)),
            (VOL_SMOOTH_VOLUME_ENABLED, flag(self.smooth_volume_enabled)),
            (VOL_FADE_IN_MS, encode_ms(self.fade_in_ms)),
            (VOL_FADE_OUT_MS, encode_ms(self.fade_out_ms)),
            (VOL_SILENCE_ENABLED, flag(self.silence_enabled)),
            (VOL_SILENCE_DURATION_MS, encode_ms(self.silence_duration_ms)),
            (
                VOL_SILENCE_THRESHOLD_DB,
                encode_hundredths(self.silence_threshold_db),
            ),
            (
                VOL_SILENCE_EDGE_TRIM_ENABLED,
                flag(self.silence_edge_trim_enabled),
            ),
            (VOL_RG_FIXED_ENABLED, flag(self.rg_fixed_enabled)),
            (VOL_RG_FIXED_DB, encode_hundredths(self.rg_fixed_db)),
            (VOL_FADE_IN_ENABLED, flag(self.fade_in_enabled)),
            (VOL_FADE_OUT_ENABLED, flag(self.fade_out_enabled)),
            (VOL_RG_MASTER_ENABLED, flag(self.rg_master_enabled)),
            (VOL_RG_TRACK_ENABLED, flag(self.rg_track_enabled)),
            (VOL_RG_ALBUM_ENABLED, flag(self.rg_album_enabled)),
            (VOL_RG_ANALYZE_RT_ENABLED, flag(self.rg_analyze_rt_enabled)),
            (
                VOL_RG_OFFSET_ALBUM_DB,
                encode_hundredths(self.rg_offset_album_db),
            ),
            (
                VOL_RG_OFFSET_TRACK_DB,
                encode_hundredths(self.rg_offset_track_db),
            ),
            (VOL_RG_OFFSET_RT_DB, encode_hundredths(self.rg_offset_rt_db)),
        ]
    }
}

// --- Crossfade ------------------------------------------------------------------

/// The 5 `crossfade_*` keys.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossfadeSettings {
    /// Crossfade group master (default off).
    pub enabled: bool,
    /// Crossfade on manual changes (default off).
    pub manual_enabled: bool,
    /// Manual crossfade length in ms (default 1000).
    pub manual_ms: f32,
    /// Crossfade on automatic changes (default off).
    pub auto_enabled: bool,
    /// Automatic crossfade length in ms (default 250).
    pub auto_ms: f32,
}

impl Default for CrossfadeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            manual_enabled: false,
            manual_ms: 1000.0,
            auto_enabled: false,
            auto_ms: 250.0,
        }
    }
}

impl CrossfadeSettings {
    /// Copies the fields out of the engine state.
    pub fn from_state(s: &AudioState) -> Self {
        Self {
            enabled: s.crossfade_enabled,
            manual_enabled: s.crossfade_manual_enabled,
            manual_ms: s.crossfade_manual_ms,
            auto_enabled: s.crossfade_auto_enabled,
            auto_ms: s.crossfade_auto_ms,
        }
    }

    /// Writes the fields into the engine state.
    pub fn apply_to(&self, s: &mut AudioState) {
        s.crossfade_enabled = self.enabled;
        s.crossfade_manual_enabled = self.manual_enabled;
        s.crossfade_manual_ms = self.manual_ms;
        s.crossfade_auto_enabled = self.auto_enabled;
        s.crossfade_auto_ms = self.auto_ms;
    }

    /// Reads the crossfade keys.
    pub fn read(db: &Database) -> Self {
        let d = Self::default();
        Self {
            enabled: read_flag(db, CROSSFADE_ENABLED, d.enabled),
            manual_enabled: read_flag(db, CROSSFADE_MANUAL_ENABLED, d.manual_enabled),
            manual_ms: read_f32(db, CROSSFADE_MANUAL_MS, d.manual_ms),
            auto_enabled: read_flag(db, CROSSFADE_AUTO_ENABLED, d.auto_enabled),
            auto_ms: read_f32(db, CROSSFADE_AUTO_MS, d.auto_ms),
        }
    }

    /// The stored strings of the 5 keys, in `save_crossfade_settings_to_db`'s
    /// order and format.
    pub fn entries(&self) -> Vec<SettingEntry> {
        vec![
            (CROSSFADE_ENABLED, encode_flag(self.enabled).to_string()),
            (
                CROSSFADE_MANUAL_ENABLED,
                encode_flag(self.manual_enabled).to_string(),
            ),
            (CROSSFADE_MANUAL_MS, encode_ms(self.manual_ms)),
            (
                CROSSFADE_AUTO_ENABLED,
                encode_flag(self.auto_enabled).to_string(),
            ),
            (CROSSFADE_AUTO_MS, encode_ms(self.auto_ms)),
        ]
    }
}

// --- VU meter -------------------------------------------------------------------

/// One `meter_*` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeterKey {
    /// `meter_hold_time_ms`.
    HoldTimeMs,
    /// `meter_rms_window_ms`.
    RmsWindowMs,
    /// `meter_ms_mode`.
    MsMode,
    /// `meter_infinite_hold`.
    InfiniteHold,
    /// `meter_enabled`.
    Enabled,
}

impl MeterKey {
    /// Every meter key.
    pub const ALL: [MeterKey; 5] = [
        MeterKey::HoldTimeMs,
        MeterKey::RmsWindowMs,
        MeterKey::MsMode,
        MeterKey::InfiniteHold,
        MeterKey::Enabled,
    ];

    /// The `APP_SETTINGS` key.
    pub fn key(self) -> &'static str {
        match self {
            MeterKey::HoldTimeMs => METER_HOLD_TIME_MS,
            MeterKey::RmsWindowMs => METER_RMS_WINDOW_MS,
            MeterKey::MsMode => METER_MS_MODE,
            MeterKey::InfiniteHold => METER_INFINITE_HOLD,
            MeterKey::Enabled => METER_ENABLED,
        }
    }
}

/// The 5 `meter_*` keys.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterSettings {
    /// Peak hold time in ms (default 1500).
    pub hold_time_ms: f32,
    /// RMS window in ms (default 300).
    pub rms_window_ms: f32,
    /// Mid/side display (default off). The Iced UI keeps it in its player UI
    /// state, not in the engine state.
    pub ms_mode: bool,
    /// Infinite peak hold (default off).
    pub infinite_hold: bool,
    /// Meter shown (default off).
    pub enabled: bool,
}

impl Default for MeterSettings {
    fn default() -> Self {
        Self {
            hold_time_ms: 1500.0,
            rms_window_ms: 300.0,
            ms_mode: false,
            infinite_hold: false,
            enabled: false,
        }
    }
}

impl MeterSettings {
    /// Copies the engine-state fields; `ms_mode` is not part of the engine
    /// state and is set to `ms_mode`.
    pub fn from_state(s: &AudioState, ms_mode: bool) -> Self {
        Self {
            hold_time_ms: s.meter_hold_time_ms,
            rms_window_ms: s.meter_rms_window_ms,
            ms_mode,
            infinite_hold: s.meter_infinite_hold,
            enabled: s.meter_enabled,
        }
    }

    /// Writes the engine-state fields (all but `ms_mode`).
    pub fn apply_to(&self, s: &mut AudioState) {
        s.meter_hold_time_ms = self.hold_time_ms;
        s.meter_rms_window_ms = self.rms_window_ms;
        s.meter_infinite_hold = self.infinite_hold;
        s.meter_enabled = self.enabled;
    }

    /// Reads the meter keys.
    pub fn read(db: &Database) -> Self {
        let d = Self::default();
        Self {
            hold_time_ms: read_f32(db, METER_HOLD_TIME_MS, d.hold_time_ms),
            rms_window_ms: read_f32(db, METER_RMS_WINDOW_MS, d.rms_window_ms),
            ms_mode: read_flag(db, METER_MS_MODE, d.ms_mode),
            infinite_hold: read_flag(db, METER_INFINITE_HOLD, d.infinite_hold),
            enabled: read_flag(db, METER_ENABLED, d.enabled),
        }
    }

    /// The stored string of one key, as the Iced `Meter*` handlers write it.
    pub fn entry(&self, key: MeterKey) -> SettingEntry {
        let value = match key {
            MeterKey::HoldTimeMs => encode_ms(self.hold_time_ms),
            MeterKey::RmsWindowMs => encode_ms(self.rms_window_ms),
            MeterKey::MsMode => encode_flag(self.ms_mode).to_string(),
            MeterKey::InfiniteHold => encode_flag(self.infinite_hold).to_string(),
            MeterKey::Enabled => encode_flag(self.enabled).to_string(),
        };
        (key.key(), value)
    }

    /// The stored strings of every key.
    pub fn entries(&self) -> Vec<SettingEntry> {
        MeterKey::ALL.iter().map(|&key| self.entry(key)).collect()
    }
}

// --- Everything -----------------------------------------------------------------

/// Every persisted setting, as read at startup and by the Audio Center.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersistedSettings {
    /// The player keys.
    pub player: PlayerSettings,
    /// The output device keys.
    pub device: DeviceSettings,
    /// The equalizer keys.
    pub eq: EqSettings,
    /// The built-in presets the user deleted (quirk fix 14).
    pub eq_hidden_builtin_presets: Vec<String>,
    /// The effect keys.
    pub dsp: DspSettings,
    /// The downmix keys.
    pub downmix: DownmixSettings,
    /// The `vol_*` keys.
    pub mix: MixSettings,
    /// The crossfade keys.
    pub crossfade: CrossfadeSettings,
    /// The meter keys.
    pub meter: MeterSettings,
}

impl PersistedSettings {
    /// Reads every key from `db`.
    pub fn read(db: &Database) -> Self {
        Self {
            player: PlayerSettings::read(db),
            device: DeviceSettings::read(db),
            eq: EqSettings::read(db),
            eq_hidden_builtin_presets: read_hidden_builtin_presets(db),
            dsp: DspSettings::read(db),
            downmix: DownmixSettings::read(db),
            mix: MixSettings::read(db),
            crossfade: CrossfadeSettings::read(db),
            meter: MeterSettings::read(db),
        }
    }

    /// The stored strings of every key (a `None` player field and a missing
    /// bit depth are left out).
    pub fn entries(&self) -> Vec<SettingEntry> {
        let mut entries = self.player.entries();
        entries.extend(self.device.entries());
        entries.extend(self.eq.entries());
        entries.push((
            EQ_HIDDEN_BUILTIN_PRESETS,
            encode_hidden_builtin_presets(&self.eq_hidden_builtin_presets),
        ));
        entries.extend(self.dsp.entries());
        entries.extend(self.downmix.entries());
        entries.extend(self.mix.entries());
        entries.extend(self.crossfade.entries());
        entries.extend(self.meter.entries());
        entries
    }
}

/// Reads every persisted setting on the database worker (for startup and the
/// Audio Center).
pub fn load_all(db: &DbWorker) -> Reply<PersistedSettings> {
    db.read(|db| Ok(PersistedSettings::read(db)))
}

// --- Writes -----------------------------------------------------------------------

/// The coalescing key of a setting's writes.
fn coalescing_key(key: &str) -> String {
    format!("app_settings:{key}")
}

/// Stores `entries`, one coalesced write per key (last write wins per key).
///
/// The reply resolves once every entry has been stored, or replaced by a newer
/// write of the same key, which counts as success. A failed write is logged
/// (as the Iced UI does) and the first failure is returned.
pub fn write_entries(db: &DbWorker, entries: Vec<SettingEntry>) -> Reply<()> {
    let replies: Vec<Reply<()>> = entries
        .into_iter()
        .map(|(key, value)| {
            db.write_coalesced(coalescing_key(key), move |db| {
                db.set_setting(key, &value).map_err(|e| {
                    tracing::warn!(
                        target: "audoxidy::persistence",
                        key,
                        error = %e,
                        "the setting could not be persisted"
                    );
                    FrontendError::from(e)
                })
            })
        })
        .collect();
    // Every write above sits before this job in the queue (a coalesced write
    // keeps its place), so their results are in by the time it runs.
    db.read(move |_| {
        for reply in replies {
            match reply.try_recv() {
                Some(Err(FrontendError::Superseded)) | Some(Ok(())) | None => {}
                Some(Err(e)) => return Err(e),
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::test_fixtures::LibraryFixture;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::collections::{BTreeMap, HashSet};
    use std::sync::{Arc, Mutex};

    fn worker() -> DbWorker {
        DbWorker::new(Arc::new(Mutex::new(Database::new_memory().unwrap()))).unwrap()
    }

    fn raw(worker: &DbWorker, key: &'static str) -> Option<String> {
        worker
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    fn seed(worker: &DbWorker, pairs: &[(&str, &str)]) {
        let mut fixture = LibraryFixture::new();
        for (key, value) in pairs {
            fixture = fixture.setting(key, value);
        }
        fixture.seed_on(worker).recv_blocking().unwrap();
    }

    /// A value for every key, none of them a default.
    fn non_default() -> PersistedSettings {
        PersistedSettings {
            player: PlayerSettings {
                volume: Some(0.625),
                last_active_playlist_id: Some(7),
            },
            device: DeviceSettings {
                host: Some("ALSA".into()),
                device: Some("PipeWire Sound Server".into()),
                device_id: Some("alsa:pipewire".into()),
                sample_rate: Some(96_000),
                bit_depth: Some(BitDepth::Bits24),
                channels: 6,
                buffer_size: Some(1024),
                system_rate: SystemSelection::Fixed(96_000),
                system_quantum: SystemSelection::Automatic,
            },
            eq: EqSettings {
                enabled: true,
                bands_31: true,
                preamp_gain: -3.5,
                band_gains_20: (0..20).map(|i| i as f32 * 0.5 - 4.0).collect(),
                band_gains_31: (0..31).map(|i| (i as f32 - 15.0) * 0.5).collect(),
                selected_preset: Some("Rock, Live".into()),
            },
            eq_hidden_builtin_presets: vec!["Jazz".into(), "Odd, \"name\"".into()],
            dsp: DspSettings {
                sub_bass_enabled: true,
                mid_bass_enabled: true,
                voice_boost_enabled: true,
                noise_gate_enabled: true,
                stereo_expander_enabled: true,
                stereo_balance_enabled: true,
                compressor_enabled: true,
                limiter_enabled: true,
                reverb_enabled: true,
                sub_bass_gain: 6.5,
                mid_bass_gain: -2.5,
                voice_boost_gain: 3.0,
                noise_gate_threshold: -45.5,
                stereo_expander_width: 1.5,
                stereo_expander_mode: ExpanderMode::Surround,
                stereo_balance: -0.5,
                compressor_threshold: -12.5,
                compressor_intensity: 0.8,
                limiter_ceiling: -3.5,
                reverb_wet: 0.25,
                reverb_room_size: 0.75,
            },
            downmix: DownmixSettings {
                center: 1.25,
                lfe: 0.5,
                surround: 1.75,
                center_enabled: true,
                lfe_enabled: true,
                surround_enabled: true,
            },
            mix: MixSettings {
                fades_enabled: true,
                smooth_volume_enabled: true,
                fade_in_ms: 1500.0,
                fade_out_ms: 4500.0,
                fade_in_enabled: true,
                fade_out_enabled: true,
                silence_enabled: false,
                silence_duration_ms: 2500.0,
                silence_threshold_db: -60.25,
                silence_edge_trim_enabled: false,
                rg_fixed_enabled: true,
                rg_fixed_db: -6.5,
                rg_master_enabled: false,
                rg_track_enabled: false,
                rg_album_enabled: false,
                rg_analyze_rt_enabled: false,
                rg_offset_album_db: 1.25,
                rg_offset_track_db: -2.75,
                rg_offset_rt_db: 3.5,
            },
            crossfade: CrossfadeSettings {
                enabled: true,
                manual_enabled: true,
                manual_ms: 3000.0,
                auto_enabled: true,
                auto_ms: 750.0,
            },
            meter: MeterSettings {
                hold_time_ms: 2500.0,
                rms_window_ms: 150.0,
                ms_mode: true,
                infinite_hold: true,
                enabled: true,
            },
        }
    }

    #[test]
    fn the_key_list_names_every_key_once() {
        let unique: HashSet<&str> = keys::ALL.iter().copied().collect();
        assert_eq!(unique.len(), keys::ALL.len());
        let written: HashSet<&str> = non_default().entries().iter().map(|e| e.0).collect();
        assert_eq!(written, unique, "every key has a writer");
    }

    #[test]
    fn every_key_round_trips_through_the_database() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            let settings = non_default();
            write_entries(&worker, settings.entries())
                .recv_blocking()
                .unwrap();
            let read = load_all(&worker).recv_blocking().unwrap();
            assert_eq!(read, settings);
        });
    }

    #[test]
    fn every_stored_string_is_byte_identical_to_the_legacy_ui_format() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            write_entries(&worker, non_default().entries())
                .recv_blocking()
                .unwrap();
            let expected: BTreeMap<&str, &str> = [
                ("player_volume", "0.6250"),
                ("last_active_playlist_id", "7"),
                ("audio_host", "ALSA"),
                ("audio_device", "PipeWire Sound Server"),
                ("audio_device_id", "alsa:pipewire"),
                ("audio_sample_rate", "96000"),
                ("audio_bit_depth", "24"),
                ("audio_channels", "6"),
                ("audio_buffer_size", "1024"),
                ("audio_system_rate", "fixed:96000"),
                ("audio_system_quantum", "auto"),
                ("eq_enabled", "1"),
                ("eq_bands_31", "1"),
                ("preamp_gain", "-3.5"),
                (
                    "eq_band_gains_20",
                    "-4.0,-3.5,-3.0,-2.5,-2.0,-1.5,-1.0,-0.5,0.0,0.5,1.0,1.5,2.0,2.5,3.0,3.5,4.0,4.5,5.0,5.5",
                ),
                (
                    "eq_band_gains_31",
                    "-7.5,-7.0,-6.5,-6.0,-5.5,-5.0,-4.5,-4.0,-3.5,-3.0,-2.5,-2.0,-1.5,-1.0,-0.5,0.0,0.5,1.0,1.5,2.0,2.5,3.0,3.5,4.0,4.5,5.0,5.5,6.0,6.5,7.0,7.5",
                ),
                ("eq_selected_preset", "Rock, Live"),
                ("eq_hidden_builtin_presets", r#"["Jazz","Odd, \"name\""]"#),
                ("dsp_sub_bass_enabled", "1"),
                ("dsp_mid_bass_enabled", "1"),
                ("dsp_voice_boost_enabled", "1"),
                ("dsp_noise_gate_enabled", "1"),
                ("dsp_stereo_expander_enabled", "1"),
                ("dsp_stereo_balance_enabled", "1"),
                ("dsp_compressor_enabled", "1"),
                ("dsp_limiter_enabled", "1"),
                ("dsp_reverb_enabled", "1"),
                ("dsp_sub_bass_gain", "6.5"),
                ("dsp_mid_bass_gain", "-2.5"),
                ("dsp_voice_boost_gain", "3.0"),
                ("dsp_noise_gate_threshold", "-45.5"),
                ("dsp_stereo_expander_width", "1.5"),
                ("dsp_stereo_expander_mode", "surround"),
                ("dsp_stereo_balance_balance", "-0.5"),
                ("dsp_compressor_threshold", "-12.5"),
                ("dsp_compressor_intensity", "0.8"),
                ("dsp_limiter_ceiling", "-3.5"),
                ("dsp_reverb_wet", "0.25"),
                ("dsp_reverb_room_size", "0.75"),
                ("audio_downmix_center", "1.25"),
                ("audio_downmix_lfe", "0.50"),
                ("audio_downmix_surround", "1.75"),
                ("audio_downmix_center_enabled", "1"),
                ("audio_downmix_lfe_enabled", "1"),
                ("audio_downmix_surround_enabled", "1"),
                ("vol_fades_enabled", "1"),
                ("vol_smooth_volume_enabled", "1"),
                ("vol_fade_in_ms", "1500"),
                ("vol_fade_out_ms", "4500"),
                ("vol_fade_in_enabled", "1"),
                ("vol_fade_out_enabled", "1"),
                ("vol_silence_enabled", "0"),
                ("vol_silence_duration_ms", "2500"),
                ("vol_silence_threshold_db", "-60.25"),
                ("vol_silence_edge_trim_enabled", "0"),
                ("vol_rg_fixed_enabled", "1"),
                ("vol_rg_fixed_db", "-6.50"),
                ("vol_rg_master_enabled", "0"),
                ("vol_rg_track_enabled", "0"),
                ("vol_rg_album_enabled", "0"),
                ("vol_rg_analyze_rt_enabled", "0"),
                ("vol_rg_offset_album_db", "1.25"),
                ("vol_rg_offset_track_db", "-2.75"),
                ("vol_rg_offset_rt_db", "3.50"),
                ("crossfade_enabled", "1"),
                ("crossfade_manual_enabled", "1"),
                ("crossfade_manual_ms", "3000"),
                ("crossfade_auto_enabled", "1"),
                ("crossfade_auto_ms", "750"),
                ("meter_hold_time_ms", "2500"),
                ("meter_rms_window_ms", "150"),
                ("meter_ms_mode", "1"),
                ("meter_infinite_hold", "1"),
                ("meter_enabled", "1"),
            ]
            .into_iter()
            .collect();
            assert_eq!(expected.len(), keys::ALL.len());
            for key in keys::ALL {
                assert_eq!(
                    raw(&worker, key).as_deref(),
                    expected.get(key).copied(),
                    "{key}"
                );
            }
        });
    }

    #[test]
    fn the_default_values_keep_the_legacy_ui_formats() {
        assert_eq!(encode_volume(1.0), "1.0000");
        assert_eq!(encode_volume(0.3), "0.3000");
        assert_eq!(encode_ms(250.0), "250");
        assert_eq!(encode_tenths(0.0), "0.0");
        assert_eq!(encode_hundredths(0.76), "0.76");
        assert_eq!(encode_flag(false), "0");
        let mut defaults: BTreeMap<&str, String> =
            PersistedSettings::default().entries().into_iter().collect();
        assert_eq!(
            defaults.remove("eq_band_gains_20").unwrap(),
            vec!["0.0"; 20].join(",")
        );
        assert_eq!(defaults["audio_sample_rate"], "auto");
        assert_eq!(defaults["audio_buffer_size"], "auto");
        assert_eq!(defaults["audio_system_rate"], "default");
        assert_eq!(defaults["audio_host"], "");
        assert_eq!(defaults["eq_selected_preset"], "");
        assert_eq!(defaults["eq_hidden_builtin_presets"], "[]");
        assert_eq!(defaults["dsp_stereo_expander_mode"], "hybrid");
        assert_eq!(defaults["dsp_compressor_intensity"], "0.5");
        assert_eq!(defaults["dsp_reverb_wet"], "0.50");
        assert_eq!(defaults["vol_fade_in_ms"], "2000");
        assert_eq!(defaults["vol_silence_threshold_db"], "-47.00");
        assert_eq!(defaults["crossfade_auto_ms"], "250");
        assert_eq!(defaults["meter_hold_time_ms"], "1500");
        assert!(!defaults.contains_key("player_volume"));
        assert!(!defaults.contains_key("audio_bit_depth"));
    }

    #[test]
    fn strings_written_by_the_legacy_ui_parse_back() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            seed(
                &worker,
                &[
                    ("player_volume", "0.4500"),
                    ("last_active_playlist_id", "3"),
                    ("audio_host", ""),
                    ("audio_device", "pulse"),
                    ("audio_device_id", ""),
                    ("audio_sample_rate", "auto"),
                    ("audio_bit_depth", "32"),
                    ("audio_channels", "2"),
                    ("audio_buffer_size", "auto"),
                    ("audio_system_rate", "auto"),
                    ("audio_system_quantum", "fixed:512"),
                    ("eq_enabled", "1"),
                    ("eq_bands_31", "0"),
                    ("preamp_gain", "-2.0"),
                    ("eq_band_gains_20", &vec!["1.5"; 20].join(",")),
                    ("eq_selected_preset", "Default"),
                    ("dsp_stereo_expander_mode", "hybrid"),
                    ("dsp_reverb_room_size", "0.80"),
                    ("dsp_compressor_intensity", "0.7"),
                    ("audio_downmix_lfe", "1.00"),
                    ("vol_silence_threshold_db", "-50.25"),
                    ("vol_fade_in_ms", "0"),
                    ("crossfade_auto_ms", "500"),
                    ("meter_rms_window_ms", "50"),
                ],
            );
            let s = load_all(&worker).recv_blocking().unwrap();
            assert_eq!(s.player.volume, Some(0.45));
            assert_eq!(s.player.last_active_playlist_id, Some(3));
            assert_eq!(s.device.host, None, "an empty host is no host");
            assert_eq!(s.device.device.as_deref(), Some("pulse"));
            assert_eq!(s.device.device_id, None);
            assert_eq!(s.device.sample_rate, None);
            assert_eq!(s.device.bit_depth, Some(BitDepth::Bits32Float));
            assert_eq!(s.device.channels, 2);
            assert_eq!(s.device.system_rate, SystemSelection::Automatic);
            assert_eq!(s.device.system_quantum, SystemSelection::Fixed(512));
            assert!(s.eq.enabled);
            assert!(!s.eq.bands_31);
            assert_eq!(s.eq.preamp_gain, -2.0);
            assert_eq!(s.eq.band_gains_20, vec![1.5; 20]);
            assert_eq!(s.eq.band_gains_31, vec![0.0; 31]);
            assert_eq!(s.eq.selected_preset, None, "\"Default\" is no selection");
            assert_eq!(s.dsp.stereo_expander_mode, ExpanderMode::Hybrid);
            assert_eq!(s.dsp.reverb_room_size, 0.8);
            assert_eq!(s.dsp.compressor_intensity, 0.7);
            assert_eq!(s.downmix.lfe, 1.0);
            assert_eq!(s.mix.silence_threshold_db, -50.25);
            assert_eq!(s.mix.fade_in_ms, 0.0);
            assert_eq!(s.crossfade.auto_ms, 500.0);
            assert_eq!(s.meter.rms_window_ms, 50.0);
        });
    }

    #[test]
    fn missing_keys_give_the_legacy_startup_defaults() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            let s = load_all(&worker).recv_blocking().unwrap();
            assert_eq!(s, PersistedSettings::default());
            assert_eq!(s.player.volume, None);
            assert!(!s.device.has_saved_output());
            assert_eq!(s.device.channels, 2);
            assert_eq!(s.eq.band_gains_20, vec![0.0; 20]);
            assert_eq!(s.eq.band_gains_31, vec![0.0; 31]);
            assert_eq!(s.dsp.noise_gate_threshold, -60.0);
            assert_eq!(s.dsp.compressor_threshold, -3.0);
            assert_eq!(s.dsp.compressor_intensity, 0.5);
            assert_eq!(s.dsp.limiter_ceiling, -1.0);
            assert_eq!(s.dsp.stereo_expander_width, 1.0);
            assert_eq!(s.downmix.center, 0.76);
            assert_eq!(s.downmix.lfe, 0.66);
            assert_eq!(s.downmix.surround, 0.74);
            assert_eq!(
                (s.mix.fade_in_ms, s.mix.fade_out_ms),
                (2000.0, 3000.0),
                "the startup defaults, not AudioState::default()"
            );
            assert!(s.mix.silence_enabled && s.mix.rg_master_enabled);
            assert_eq!(s.crossfade.manual_ms, 1000.0);
            assert_eq!(s.meter.hold_time_ms, 1500.0);
            assert_eq!(s.meter.rms_window_ms, 300.0);
            assert!(!s.meter.enabled);
            assert!(s.eq_hidden_builtin_presets.is_empty());
        });
    }

    #[test]
    fn garbage_values_give_the_legacy_defaults_and_garbage_flags_read_as_off() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            seed(
                &worker,
                &[
                    ("player_volume", "loud"),
                    ("last_active_playlist_id", "x"),
                    ("audio_sample_rate", "fast"),
                    ("audio_bit_depth", "32f"),
                    ("audio_channels", "stereo"),
                    ("audio_buffer_size", "-1"),
                    ("audio_system_rate", "fixed:"),
                    ("audio_system_quantum", "FIXED:512"),
                    ("preamp_gain", "high"),
                    ("eq_band_gains_20", "1.0,2.0"),
                    ("eq_band_gains_31", "nope"),
                    ("eq_hidden_builtin_presets", "Rock,Jazz"),
                    ("dsp_sub_bass_gain", "?"),
                    ("dsp_stereo_expander_mode", "wide"),
                    ("audio_downmix_center", ""),
                    ("vol_fade_out_ms", "3s"),
                    ("crossfade_manual_ms", "--"),
                    ("meter_hold_time_ms", "long"),
                    ("vol_silence_enabled", "true"),
                    ("vol_rg_master_enabled", "yes"),
                ],
            );
            let s = load_all(&worker).recv_blocking().unwrap();
            assert_eq!(s.player, PlayerSettings::default());
            assert_eq!(s.device.sample_rate, None);
            assert_eq!(s.device.bit_depth, None);
            assert_eq!(s.device.channels, 2);
            assert_eq!(s.device.buffer_size, None);
            assert_eq!(s.device.system_rate, SystemSelection::Default);
            assert_eq!(s.device.system_quantum, SystemSelection::Default);
            assert_eq!(s.eq.preamp_gain, 0.0);
            assert_eq!(s.eq.band_gains_20, vec![0.0; 20]);
            assert_eq!(s.eq.band_gains_31, vec![0.0; 31]);
            assert!(s.eq_hidden_builtin_presets.is_empty());
            assert_eq!(s.dsp.sub_bass_gain, 0.0);
            assert_eq!(s.dsp.stereo_expander_mode, ExpanderMode::Hybrid);
            assert_eq!(s.downmix.center, 0.76);
            assert_eq!(s.mix.fade_out_ms, 3000.0);
            assert_eq!(s.crossfade.manual_ms, 1000.0);
            assert_eq!(s.meter.hold_time_ms, 1500.0);
            // The Iced rule for a flag is `value == "1"`: anything else is off,
            // even for a flag whose default is on.
            assert!(!s.mix.silence_enabled);
            assert!(!s.mix.rg_master_enabled);
        });
    }

    #[test]
    fn a_saved_volume_is_clamped_to_the_unit_range() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            seed(&worker, &[("player_volume", "1.5000")]);
            assert_eq!(
                load_all(&worker).recv_blocking().unwrap().player.volume,
                Some(1.0)
            );
        });
    }

    #[test]
    fn only_saved_output_keys_other_than_channels_count_as_a_saved_output() {
        let channels_only = DeviceSettings {
            channels: 6,
            ..DeviceSettings::default()
        };
        assert!(!channels_only.has_saved_output());
        let rate_only = DeviceSettings {
            sample_rate: Some(44_100),
            ..DeviceSettings::default()
        };
        assert!(rate_only.has_saved_output());
        let applied = rate_only.to_audio_settings();
        assert_eq!(applied.sample_rate, Some(44_100));
        assert_eq!(applied.channels, ChannelConfig::Manual(2));
    }

    #[test]
    fn hidden_preset_names_with_commas_survive_the_json_encoding() {
        let names = vec!["Rock, Live".to_string(), "[x]".to_string()];
        let encoded = encode_hidden_builtin_presets(&names);
        assert_eq!(encoded, r#"["Rock, Live","[x]"]"#);
        assert_eq!(decode_hidden_builtin_presets(&encoded), names);
        assert!(decode_hidden_builtin_presets("{}").is_empty());
    }

    #[test]
    fn a_burst_of_writes_to_one_key_stores_the_last_value() {
        with_timeout(TEST_TIMEOUT, || {
            let worker = worker();
            let replies: Vec<_> = (0..50)
                .map(|i| write_player_volume(&worker, i as f32 / 100.0))
                .collect();
            for reply in replies {
                reply
                    .recv_blocking()
                    .expect("a superseded write counts as stored");
            }
            assert_eq!(raw(&worker, "player_volume").as_deref(), Some("0.4900"));
        });
    }

    #[test]
    fn the_dsp_settings_copy_to_and_from_the_chain() {
        let settings = non_default().dsp;
        let mut chain = DspChain::default();
        settings.apply_to(&mut chain);
        assert_eq!(DspSettings::from_chain(&chain), settings);
        assert_eq!(chain.stereo_expander.mode, ExpanderMode::Surround);
    }

    #[test]
    fn the_engine_state_groups_copy_to_and_from_the_state() {
        let s = non_default();
        let mut state = AudioState::default();
        s.mix.apply_to(&mut state);
        s.crossfade.apply_to(&mut state);
        s.downmix.apply_to(&mut state);
        s.meter.apply_to(&mut state);
        assert_eq!(MixSettings::from_state(&state), s.mix);
        assert_eq!(CrossfadeSettings::from_state(&state), s.crossfade);
        assert_eq!(DownmixSettings::from_state(&state), s.downmix);
        assert_eq!(MeterSettings::from_state(&state, true), s.meter);
    }
}
