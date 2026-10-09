//! The effects tab: the DSP effects and the downmix settings.
//!
//! Each toggle and value maps onto the engine exactly as the Iced
//! `DspToggle`, `DspValueChanged`, `StereoExpanderModeToggled`,
//! `AudioStateToggle` and `AudioStateValueChanged` handlers do
//! (`src/gui/audio_center.rs`, `AudioCenterManager::update`):
//!
//! | Control | Engine |
//! | --- | --- |
//! | Sub-bass | `sub_bass.gain` (45 Hz low shelf) |
//! | Mid-bass | `mid_bass.gain` (100 Hz peak) |
//! | Voice boost | `voice_boost.gain` (peaks at 1.5 and 3 kHz) |
//! | Noise gate | `noise_gate.threshold` |
//! | Compressor | `compressor.threshold`; intensity `v / 100`, then `update_intensity_params` |
//! | Limiter | `limiter.ceiling` |
//! | Reverb | `reverb.set_wet`, `reverb.set_room_size` |
//! | Stereo expander | width `v / 100`; mode Hybrid ("Natural") or Surround |
//! | Stereo balance | `stereo_balance.balance` |
//! | Downmix | `AudioState::downmix_*` and `downmix_*_enabled` |
//!
//! Values are in the units the sliders show ([`DspParam::range`]); a value
//! outside the range is clamped to it, as the sliders do. After each change the
//! changed key is persisted in `save_dsp_settings_to_db`'s format, read back
//! from the engine.

#![warn(missing_docs)]

use std::ops::RangeInclusive;

use crate::audio::dsp::{DspChain, ExpanderMode};
use crate::frontend::app_context::AppContext;
use crate::frontend::reply::Reply;
use crate::frontend::settings::{self, DownmixSettings, DspKey, DspSettings};

pub use crate::frontend::settings::DownmixChannel;

/// An effect that can be turned on and off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DspEffect {
    /// "Refuerzo de Sub-Graves".
    SubBass,
    /// "Refuerzo de Graves".
    MidBass,
    /// "Refuerzo de Voces".
    VoiceBoost,
    /// "Reducción de Ruido".
    NoiseGate,
    /// "Expansor Estéreo".
    StereoExpander,
    /// "Balance Estéreo".
    StereoBalance,
    /// "Compresor".
    Compressor,
    /// "Limitador".
    Limiter,
    /// "Reverberación".
    Reverb,
}

impl DspEffect {
    /// The key of the effect's on/off flag.
    pub fn key(self) -> DspKey {
        match self {
            DspEffect::SubBass => DspKey::SubBassEnabled,
            DspEffect::MidBass => DspKey::MidBassEnabled,
            DspEffect::VoiceBoost => DspKey::VoiceBoostEnabled,
            DspEffect::NoiseGate => DspKey::NoiseGateEnabled,
            DspEffect::StereoExpander => DspKey::StereoExpanderEnabled,
            DspEffect::StereoBalance => DspKey::StereoBalanceEnabled,
            DspEffect::Compressor => DspKey::CompressorEnabled,
            DspEffect::Limiter => DspKey::LimiterEnabled,
            DspEffect::Reverb => DspKey::ReverbEnabled,
        }
    }

    /// Whether the effect is on in `settings`.
    pub fn is_enabled(self, settings: &DspSettings) -> bool {
        match self {
            DspEffect::SubBass => settings.sub_bass_enabled,
            DspEffect::MidBass => settings.mid_bass_enabled,
            DspEffect::VoiceBoost => settings.voice_boost_enabled,
            DspEffect::NoiseGate => settings.noise_gate_enabled,
            DspEffect::StereoExpander => settings.stereo_expander_enabled,
            DspEffect::StereoBalance => settings.stereo_balance_enabled,
            DspEffect::Compressor => settings.compressor_enabled,
            DspEffect::Limiter => settings.limiter_enabled,
            DspEffect::Reverb => settings.reverb_enabled,
        }
    }

    fn set(self, dsp: &mut DspChain, enabled: bool) {
        match self {
            DspEffect::SubBass => dsp.sub_bass.enabled = enabled,
            DspEffect::MidBass => dsp.mid_bass.enabled = enabled,
            DspEffect::VoiceBoost => dsp.voice_boost.enabled = enabled,
            DspEffect::NoiseGate => dsp.noise_gate.enabled = enabled,
            DspEffect::StereoExpander => dsp.stereo_expander.enabled = enabled,
            DspEffect::StereoBalance => dsp.stereo_balance.enabled = enabled,
            DspEffect::Compressor => dsp.compressor.enabled = enabled,
            DspEffect::Limiter => dsp.limiter.enabled = enabled,
            DspEffect::Reverb => dsp.reverb.enabled = enabled,
        }
    }
}

/// A slider of the effects tab, in the units it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DspParam {
    /// Sub-bass level, dB.
    SubBassGain,
    /// Mid-bass level, dB.
    MidBassGain,
    /// Voice boost level, dB.
    VoiceBoostGain,
    /// Noise gate threshold, dB.
    NoiseGateThreshold,
    /// Stereo expander width, percent (the engine takes `v / 100`).
    StereoExpanderWidth,
    /// Stereo balance, -1 (left) to 1 (right).
    StereoBalance,
    /// Compressor threshold, dB.
    CompressorThreshold,
    /// Compressor intensity, percent (the engine takes `v / 100`).
    CompressorIntensity,
    /// Limiter ceiling, dB.
    LimiterCeiling,
    /// Reverb wet mix, 0 to 1.
    ReverbWet,
    /// Reverb room size, 0 to 1.
    ReverbRoomSize,
}

impl DspParam {
    /// Every slider.
    pub const ALL: [DspParam; 11] = [
        DspParam::SubBassGain,
        DspParam::MidBassGain,
        DspParam::VoiceBoostGain,
        DspParam::NoiseGateThreshold,
        DspParam::StereoExpanderWidth,
        DspParam::StereoBalance,
        DspParam::CompressorThreshold,
        DspParam::CompressorIntensity,
        DspParam::LimiterCeiling,
        DspParam::ReverbWet,
        DspParam::ReverbRoomSize,
    ];

    /// The slider's range, in its units (the Iced effects tab's ranges).
    pub fn range(self) -> RangeInclusive<f32> {
        match self {
            DspParam::SubBassGain => -4.0..=24.0,
            DspParam::MidBassGain => -4.0..=15.0,
            DspParam::VoiceBoostGain => -4.0..=13.0,
            DspParam::NoiseGateThreshold => -85.0..=-10.0,
            DspParam::StereoExpanderWidth => 0.0..=260.0,
            DspParam::StereoBalance => -1.0..=1.0,
            DspParam::CompressorThreshold => -24.0..=0.0,
            DspParam::CompressorIntensity => 0.0..=100.0,
            DspParam::LimiterCeiling => -12.0..=0.0,
            DspParam::ReverbWet | DspParam::ReverbRoomSize => 0.0..=1.0,
        }
    }

    /// The value a secondary click resets the slider to, in its units.
    pub fn default_value(self) -> f32 {
        match self {
            DspParam::SubBassGain
            | DspParam::MidBassGain
            | DspParam::VoiceBoostGain
            | DspParam::StereoBalance => 0.0,
            DspParam::NoiseGateThreshold => -60.0,
            DspParam::StereoExpanderWidth => 100.0,
            DspParam::CompressorThreshold => -3.0,
            DspParam::CompressorIntensity => 50.0,
            DspParam::LimiterCeiling => -1.0,
            DspParam::ReverbWet | DspParam::ReverbRoomSize => 0.5,
        }
    }

    /// The slider's step.
    pub fn step(self) -> f32 {
        match self {
            DspParam::CompressorIntensity => 1.0,
            DspParam::ReverbWet | DspParam::ReverbRoomSize => 0.01,
            _ => 0.1,
        }
    }

    /// The persisted key.
    pub fn key(self) -> DspKey {
        match self {
            DspParam::SubBassGain => DspKey::SubBassGain,
            DspParam::MidBassGain => DspKey::MidBassGain,
            DspParam::VoiceBoostGain => DspKey::VoiceBoostGain,
            DspParam::NoiseGateThreshold => DspKey::NoiseGateThreshold,
            DspParam::StereoExpanderWidth => DspKey::StereoExpanderWidth,
            DspParam::StereoBalance => DspKey::StereoBalance,
            DspParam::CompressorThreshold => DspKey::CompressorThreshold,
            DspParam::CompressorIntensity => DspKey::CompressorIntensity,
            DspParam::LimiterCeiling => DspKey::LimiterCeiling,
            DspParam::ReverbWet => DspKey::ReverbWet,
            DspParam::ReverbRoomSize => DspKey::ReverbRoomSize,
        }
    }

    /// The slider's value in `settings`, in its units.
    pub fn value(self, settings: &DspSettings) -> f32 {
        match self {
            DspParam::SubBassGain => settings.sub_bass_gain,
            DspParam::MidBassGain => settings.mid_bass_gain,
            DspParam::VoiceBoostGain => settings.voice_boost_gain,
            DspParam::NoiseGateThreshold => settings.noise_gate_threshold,
            DspParam::StereoExpanderWidth => settings.stereo_expander_width * 100.0,
            DspParam::StereoBalance => settings.stereo_balance,
            DspParam::CompressorThreshold => settings.compressor_threshold,
            DspParam::CompressorIntensity => settings.compressor_intensity * 100.0,
            DspParam::LimiterCeiling => settings.limiter_ceiling,
            DspParam::ReverbWet => settings.reverb_wet,
            DspParam::ReverbRoomSize => settings.reverb_room_size,
        }
    }

    /// Writes `value` (in the slider's units) into the chain the Iced way.
    fn set(self, dsp: &mut DspChain, value: f32) {
        match self {
            DspParam::SubBassGain => dsp.sub_bass.gain = value,
            DspParam::MidBassGain => dsp.mid_bass.gain = value,
            DspParam::VoiceBoostGain => dsp.voice_boost.gain = value,
            DspParam::NoiseGateThreshold => dsp.noise_gate.threshold = value,
            DspParam::StereoExpanderWidth => dsp.stereo_expander.width = value / 100.0,
            DspParam::StereoBalance => dsp.stereo_balance.balance = value,
            DspParam::CompressorThreshold => dsp.compressor.threshold = value,
            DspParam::CompressorIntensity => {
                dsp.compressor.intensity = value / 100.0;
                dsp.compressor.update_intensity_params();
            }
            DspParam::LimiterCeiling => dsp.limiter.ceiling = value,
            DspParam::ReverbWet => dsp.reverb.set_wet(value),
            DspParam::ReverbRoomSize => dsp.reverb.set_room_size(value),
        }
    }
}

/// The range of the downmix sliders.
pub const DOWNMIX_RANGE: RangeInclusive<f32> = 0.0..=2.0;

/// The step of the downmix sliders.
pub const DOWNMIX_STEP: f32 = 0.01;

/// The value a secondary click resets a downmix slider to.
pub fn downmix_default(channel: DownmixChannel) -> f32 {
    DownmixSettings::default().mix(channel)
}

/// The effect settings as the engine has them.
pub fn current(ctx: &AppContext) -> DspSettings {
    let mut settings = DspSettings::default();
    ctx.player
        .read_dsp(&mut |dsp| settings = DspSettings::from_chain(dsp));
    settings
}

/// The downmix settings as the engine has them.
pub fn current_downmix(ctx: &AppContext) -> DownmixSettings {
    DownmixSettings::from_state(&ctx.player.snapshot())
}

/// Applies `change` to the chain, then persists `key` as read back from it.
fn change_dsp(ctx: &AppContext, key: DspKey, change: impl FnOnce(&mut DspChain)) -> Reply<()> {
    let mut change = Some(change);
    let mut settings = DspSettings::default();
    ctx.player.update_dsp(&mut |dsp| {
        if let Some(change) = change.take() {
            change(dsp);
        }
        settings = DspSettings::from_chain(dsp);
    });
    settings::write_entries(&ctx.db, vec![settings.entry(key)])
}

/// Turns `effect` on or off, and persists its flag.
pub fn set_enabled(ctx: &AppContext, effect: DspEffect, enabled: bool) -> Reply<()> {
    change_dsp(ctx, effect.key(), |dsp| effect.set(dsp, enabled))
}

/// Sets `param` to `value` (in the slider's units, clamped to its range), and
/// persists it.
pub fn set_value(ctx: &AppContext, param: DspParam, value: f32) -> Reply<()> {
    let range = param.range();
    let value = value.clamp(*range.start(), *range.end());
    change_dsp(ctx, param.key(), |dsp| param.set(dsp, value))
}

/// Resets `param` to [`DspParam::default_value`] (a secondary click), and
/// persists it.
pub fn reset_value(ctx: &AppContext, param: DspParam) -> Reply<()> {
    set_value(ctx, param, param.default_value())
}

/// Sets the stereo expander mode ("Natural" is Hybrid), and persists it.
pub fn set_expander_mode(ctx: &AppContext, mode: ExpanderMode) -> Reply<()> {
    change_dsp(ctx, DspKey::StereoExpanderMode, |dsp| {
        dsp.stereo_expander.mode = mode;
    })
}

/// Applies `change` to the engine state's downmix fields, then persists the
/// entry `entry` picks from them.
fn change_downmix(
    ctx: &AppContext,
    change: impl FnOnce(&mut crate::audio::engine::AudioState),
    entry: impl FnOnce(&DownmixSettings) -> settings::SettingEntry,
) -> Reply<()> {
    let downmix = {
        let state = ctx.player.state();
        let mut state = state.write();
        change(&mut state);
        DownmixSettings::from_state(&state)
    };
    settings::write_entries(&ctx.db, vec![entry(&downmix)])
}

/// Turns the downmix of `channel` on or off, and persists its flag.
pub fn set_downmix_enabled(ctx: &AppContext, channel: DownmixChannel, enabled: bool) -> Reply<()> {
    change_downmix(
        ctx,
        |state| match channel {
            DownmixChannel::Center => state.downmix_center_enabled = enabled,
            DownmixChannel::Lfe => state.downmix_lfe_enabled = enabled,
            DownmixChannel::Surround => state.downmix_surround_enabled = enabled,
        },
        |downmix| downmix.enabled_entry(channel),
    )
}

/// Sets the downmix gain of `channel` (clamped to [`DOWNMIX_RANGE`]), and
/// persists it.
pub fn set_downmix_mix(ctx: &AppContext, channel: DownmixChannel, value: f32) -> Reply<()> {
    let value = value.clamp(*DOWNMIX_RANGE.start(), *DOWNMIX_RANGE.end());
    change_downmix(
        ctx,
        |state| match channel {
            DownmixChannel::Center => state.downmix_center = value,
            DownmixChannel::Lfe => state.downmix_lfe = value,
            DownmixChannel::Surround => state.downmix_surround = value,
        },
        |downmix| downmix.mix_entry(channel),
    )
}

/// Resets the downmix gain of `channel` to [`downmix_default`], and persists it.
pub fn reset_downmix_mix(ctx: &AppContext, channel: DownmixChannel) -> Reply<()> {
    set_downmix_mix(ctx, channel, downmix_default(channel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
    use crate::frontend::ports::PlayerPort;
    use crate::frontend::settings::keys;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};

    fn context() -> (AppContext, InMemoryFakes) {
        AppContext::in_memory_with_fakes().unwrap()
    }

    fn raw(ctx: &AppContext, key: &'static str) -> Option<String> {
        ctx.db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    fn stored_keys(ctx: &AppContext) -> Vec<&'static str> {
        keys::ALL
            .iter()
            .copied()
            .filter(|key| raw(ctx, key).is_some())
            .collect()
    }

    #[test]
    fn each_toggle_switches_its_effect_and_persists_only_its_flag() {
        with_timeout(TEST_TIMEOUT, || {
            let effects = [
                DspEffect::SubBass,
                DspEffect::MidBass,
                DspEffect::VoiceBoost,
                DspEffect::NoiseGate,
                DspEffect::StereoExpander,
                DspEffect::StereoBalance,
                DspEffect::Compressor,
                DspEffect::Limiter,
                DspEffect::Reverb,
            ];
            for effect in effects {
                let (ctx, fakes) = context();
                set_enabled(&ctx, effect, true).recv_blocking().unwrap();
                assert!(effect.is_enabled(&current(&ctx)), "{effect:?}");
                assert_eq!(fakes.player.calls(), vec![PlayerCall::UpdateDsp]);
                assert_eq!(stored_keys(&ctx), vec![effect.key().key()]);
                assert_eq!(raw(&ctx, effect.key().key()).as_deref(), Some("1"));
                set_enabled(&ctx, effect, false).recv_blocking().unwrap();
                assert_eq!(raw(&ctx, effect.key().key()).as_deref(), Some("0"));
            }
        });
    }

    #[test]
    fn each_value_maps_onto_the_engine_like_the_legacy_handler() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let cases: [(DspParam, f32, &str); 11] = [
                (DspParam::SubBassGain, 6.5, "6.5"),
                (DspParam::MidBassGain, -2.0, "-2.0"),
                (DspParam::VoiceBoostGain, 3.5, "3.5"),
                (DspParam::NoiseGateThreshold, -40.0, "-40.0"),
                (DspParam::StereoExpanderWidth, 150.0, "1.5"),
                (DspParam::StereoBalance, -0.5, "-0.5"),
                (DspParam::CompressorThreshold, -12.0, "-12.0"),
                (DspParam::CompressorIntensity, 80.0, "0.8"),
                (DspParam::LimiterCeiling, -3.0, "-3.0"),
                (DspParam::ReverbWet, 0.35, "0.35"),
                (DspParam::ReverbRoomSize, 0.8, "0.80"),
            ];
            for (param, value, stored) in cases {
                set_value(&ctx, param, value).recv_blocking().unwrap();
                assert!(
                    (param.value(&current(&ctx)) - value).abs() < 1e-4,
                    "{param:?}"
                );
                assert_eq!(
                    raw(&ctx, param.key().key()).as_deref(),
                    Some(stored),
                    "{param:?}"
                );
            }
            fakes.player.read_dsp(&mut |dsp| {
                assert_eq!(dsp.stereo_expander.width, 1.5);
                assert_eq!(dsp.compressor.intensity, 0.8);
                assert_eq!(dsp.sub_bass.freq, 45.0, "the low shelf stays at 45 Hz");
                assert_eq!(dsp.mid_bass.freq, 100.0, "the peak stays at 100 Hz");
            });
        });
    }

    #[test]
    fn the_compressor_intensity_recomputes_its_derived_parameters() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_value(&ctx, DspParam::CompressorIntensity, 75.0)
                .recv_blocking()
                .unwrap();
            fakes
                .player
                .read_dsp(&mut |dsp| assert_eq!(dsp.compressor.intensity_ratio, 8.0));
            set_value(&ctx, DspParam::CompressorIntensity, 0.0)
                .recv_blocking()
                .unwrap();
            fakes
                .player
                .read_dsp(&mut |dsp| assert_eq!(dsp.compressor.intensity_ratio, 1.5));
        });
    }

    #[test]
    fn values_are_clamped_to_the_slider_range() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            set_value(&ctx, DspParam::SubBassGain, 40.0)
                .recv_blocking()
                .unwrap();
            set_value(&ctx, DspParam::StereoExpanderWidth, -10.0)
                .recv_blocking()
                .unwrap();
            let settings = current(&ctx);
            assert_eq!(settings.sub_bass_gain, 24.0);
            assert_eq!(settings.stereo_expander_width, 0.0);
        });
    }

    #[test]
    fn a_secondary_click_resets_to_the_slider_defaults() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            for param in DspParam::ALL {
                let range = param.range();
                set_value(&ctx, param, *range.end())
                    .recv_blocking()
                    .unwrap();
                reset_value(&ctx, param).recv_blocking().unwrap();
            }
            assert_eq!(current(&ctx), DspSettings::default());
            assert_eq!(
                raw(&ctx, keys::DSP_STEREO_EXPANDER_WIDTH).as_deref(),
                Some("1.0")
            );
            assert_eq!(
                raw(&ctx, keys::DSP_COMPRESSOR_INTENSITY).as_deref(),
                Some("0.5")
            );
            for param in DspParam::ALL {
                assert!(param.range().contains(&param.default_value()), "{param:?}");
            }
        });
    }

    #[test]
    fn the_expander_mode_is_persisted_as_surround_or_hybrid() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            set_expander_mode(&ctx, ExpanderMode::Surround)
                .recv_blocking()
                .unwrap();
            assert_eq!(current(&ctx).stereo_expander_mode, ExpanderMode::Surround);
            assert_eq!(
                raw(&ctx, keys::DSP_STEREO_EXPANDER_MODE).as_deref(),
                Some("surround")
            );
            set_expander_mode(&ctx, ExpanderMode::Hybrid)
                .recv_blocking()
                .unwrap();
            assert_eq!(
                raw(&ctx, keys::DSP_STEREO_EXPANDER_MODE).as_deref(),
                Some("hybrid")
            );
        });
    }

    #[test]
    fn downmix_toggles_and_gains_go_into_the_engine_state_and_are_persisted() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_downmix_enabled(&ctx, DownmixChannel::Lfe, true)
                .recv_blocking()
                .unwrap();
            set_downmix_mix(&ctx, DownmixChannel::Surround, 1.25)
                .recv_blocking()
                .unwrap();
            set_downmix_mix(&ctx, DownmixChannel::Center, 5.0)
                .recv_blocking()
                .unwrap();
            let state = fakes.player.snapshot();
            assert!(state.downmix_lfe_enabled);
            assert_eq!(state.downmix_surround, 1.25);
            assert_eq!(state.downmix_center, 2.0, "clamped");
            assert_eq!(
                raw(&ctx, keys::AUDIO_DOWNMIX_LFE_ENABLED).as_deref(),
                Some("1")
            );
            assert_eq!(
                raw(&ctx, keys::AUDIO_DOWNMIX_SURROUND).as_deref(),
                Some("1.25")
            );
            assert_eq!(
                raw(&ctx, keys::AUDIO_DOWNMIX_CENTER).as_deref(),
                Some("2.00")
            );
            reset_downmix_mix(&ctx, DownmixChannel::Center)
                .recv_blocking()
                .unwrap();
            assert_eq!(current_downmix(&ctx).center, 0.76);
            assert_eq!(
                raw(&ctx, keys::AUDIO_DOWNMIX_CENTER).as_deref(),
                Some("0.76")
            );
            assert_eq!(downmix_default(DownmixChannel::Lfe), 0.66);
            assert_eq!(downmix_default(DownmixChannel::Surround), 0.74);
        });
    }
}
