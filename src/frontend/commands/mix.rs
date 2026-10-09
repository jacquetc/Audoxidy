//! The volume-and-mix tab: fades, smooth volume, silence removal, the fixed
//! pre-amp, ReplayGain (master, track, album, real-time analysis and their
//! offsets) and crossfade.
//!
//! Each change goes into the engine state, as the Iced `Volumen*` and
//! `Crossfade*` handlers do (`src/gui/audio_center.rs`,
//! `AudioCenterManager::update`), and is persisted at once: a `vol_*` change
//! writes the 19 `vol_*` keys (`save_volumen_settings_to_db`), a crossfade
//! change the 5 `crossfade_*` keys (`save_crossfade_settings_to_db`), read back
//! from the engine state.
//!
//! The steppers clamp to the Iced ranges ([`MixValue::range`]); a secondary
//! click resets to [`MixValue::reset_value`]. Those reset values are the Iced
//! steppers' and differ from the startup defaults for the two fade lengths
//! (1000 and 2000 ms, against 2000 and 3000 ms when the key is missing).
//!
//! [`current`] reads the engine state, so the tab shows the values in effect
//! (quirk fix 21: on a fresh install the Iced tab shows 1000 and 2000 ms while
//! the engine fades over 2000 and 3000 ms).

#![warn(missing_docs)]

use std::ops::RangeInclusive;

use crate::audio::engine::AudioState;
use crate::frontend::app_context::AppContext;
use crate::frontend::reply::Reply;
use crate::frontend::settings::{self, CrossfadeSettings, MixSettings};
use crate::logic::stepper::StepperUnit;

/// An on/off control of the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixToggle {
    /// "Cambio de Volumen" group master (`fades_enabled`).
    Fades,
    /// Smooth volume changes.
    SmoothVolume,
    /// Fade-in at the start of a track.
    FadeIn,
    /// Fade-out at the end of a track.
    FadeOut,
    /// "Eliminar silencio" group master.
    Silence,
    /// Trim silences at the start and end of tracks.
    SilenceEdgeTrim,
    /// Fixed pre-amplifier.
    RgFixed,
    /// ReplayGain group master.
    RgMaster,
    /// Track ReplayGain.
    RgTrack,
    /// Album ReplayGain.
    RgAlbum,
    /// Real-time loudness analysis.
    RgAnalyzeRt,
    /// "Mezcla Cruzada" group master.
    Crossfade,
    /// Crossfade on manual track changes.
    CrossfadeManual,
    /// Crossfade on automatic track changes.
    CrossfadeAuto,
}

impl MixToggle {
    /// Whether the control belongs to the crossfade group (persisted with the
    /// `crossfade_*` keys).
    pub fn is_crossfade(self) -> bool {
        matches!(
            self,
            MixToggle::Crossfade | MixToggle::CrossfadeManual | MixToggle::CrossfadeAuto
        )
    }

    /// The control's value in the engine state.
    pub fn get(self, s: &AudioState) -> bool {
        match self {
            MixToggle::Fades => s.fades_enabled,
            MixToggle::SmoothVolume => s.smooth_volume_enabled,
            MixToggle::FadeIn => s.fade_in_enabled,
            MixToggle::FadeOut => s.fade_out_enabled,
            MixToggle::Silence => s.silence_enabled,
            MixToggle::SilenceEdgeTrim => s.silence_edge_trim_enabled,
            MixToggle::RgFixed => s.rg_fixed_enabled,
            MixToggle::RgMaster => s.rg_master_enabled,
            MixToggle::RgTrack => s.replay_gain_track_enabled,
            MixToggle::RgAlbum => s.replay_gain_album_enabled,
            MixToggle::RgAnalyzeRt => s.rg_analyze_rt_enabled,
            MixToggle::Crossfade => s.crossfade_enabled,
            MixToggle::CrossfadeManual => s.crossfade_manual_enabled,
            MixToggle::CrossfadeAuto => s.crossfade_auto_enabled,
        }
    }

    fn set(self, s: &mut AudioState, on: bool) {
        let field = match self {
            MixToggle::Fades => &mut s.fades_enabled,
            MixToggle::SmoothVolume => &mut s.smooth_volume_enabled,
            MixToggle::FadeIn => &mut s.fade_in_enabled,
            MixToggle::FadeOut => &mut s.fade_out_enabled,
            MixToggle::Silence => &mut s.silence_enabled,
            MixToggle::SilenceEdgeTrim => &mut s.silence_edge_trim_enabled,
            MixToggle::RgFixed => &mut s.rg_fixed_enabled,
            MixToggle::RgMaster => &mut s.rg_master_enabled,
            MixToggle::RgTrack => &mut s.replay_gain_track_enabled,
            MixToggle::RgAlbum => &mut s.replay_gain_album_enabled,
            MixToggle::RgAnalyzeRt => &mut s.rg_analyze_rt_enabled,
            MixToggle::Crossfade => &mut s.crossfade_enabled,
            MixToggle::CrossfadeManual => &mut s.crossfade_manual_enabled,
            MixToggle::CrossfadeAuto => &mut s.crossfade_auto_enabled,
        };
        *field = on;
    }
}

/// A stepper of the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixValue {
    /// Fade-in length, ms.
    FadeInMs,
    /// Fade-out length, ms.
    FadeOutMs,
    /// Shortest silence removed, ms.
    SilenceDurationMs,
    /// Silence threshold, dB.
    SilenceThresholdDb,
    /// Fixed pre-amplifier level, dB.
    RgFixedDb,
    /// Album ReplayGain offset, dB.
    RgOffsetAlbumDb,
    /// Track ReplayGain offset, dB.
    RgOffsetTrackDb,
    /// Real-time analysis offset, dB.
    RgOffsetRtDb,
    /// Manual crossfade length, ms.
    CrossfadeManualMs,
    /// Automatic crossfade length, ms.
    CrossfadeAutoMs,
}

impl MixValue {
    /// Every stepper.
    pub const ALL: [MixValue; 10] = [
        MixValue::FadeInMs,
        MixValue::FadeOutMs,
        MixValue::SilenceDurationMs,
        MixValue::SilenceThresholdDb,
        MixValue::RgFixedDb,
        MixValue::RgOffsetAlbumDb,
        MixValue::RgOffsetTrackDb,
        MixValue::RgOffsetRtDb,
        MixValue::CrossfadeManualMs,
        MixValue::CrossfadeAutoMs,
    ];

    /// The stepper's range (the Iced tab's ranges).
    pub fn range(self) -> RangeInclusive<f64> {
        match self {
            MixValue::FadeInMs
            | MixValue::FadeOutMs
            | MixValue::CrossfadeManualMs
            | MixValue::CrossfadeAutoMs => 0.0..=10_000.0,
            MixValue::SilenceDurationMs => 100.0..=10_000.0,
            MixValue::SilenceThresholdDb => -80.0..=0.0,
            MixValue::RgFixedDb => -30.0..=12.0,
            MixValue::RgOffsetAlbumDb | MixValue::RgOffsetTrackDb | MixValue::RgOffsetRtDb => {
                -12.0..=12.0
            }
        }
    }

    /// The stepper's unit (step and suffix).
    pub fn unit(self) -> StepperUnit {
        match self {
            MixValue::FadeInMs
            | MixValue::FadeOutMs
            | MixValue::SilenceDurationMs
            | MixValue::CrossfadeManualMs
            | MixValue::CrossfadeAutoMs => StepperUnit::Milliseconds,
            MixValue::SilenceThresholdDb
            | MixValue::RgFixedDb
            | MixValue::RgOffsetAlbumDb
            | MixValue::RgOffsetTrackDb
            | MixValue::RgOffsetRtDb => StepperUnit::Decibels,
        }
    }

    /// The value a secondary click resets the stepper to.
    pub fn reset_value(self) -> f64 {
        match self {
            MixValue::FadeInMs | MixValue::SilenceDurationMs | MixValue::CrossfadeManualMs => {
                1000.0
            }
            MixValue::FadeOutMs => 2000.0,
            MixValue::SilenceThresholdDb => -47.0,
            MixValue::CrossfadeAutoMs => 250.0,
            MixValue::RgFixedDb
            | MixValue::RgOffsetAlbumDb
            | MixValue::RgOffsetTrackDb
            | MixValue::RgOffsetRtDb => 0.0,
        }
    }

    /// Whether the stepper belongs to the crossfade group.
    pub fn is_crossfade(self) -> bool {
        matches!(
            self,
            MixValue::CrossfadeManualMs | MixValue::CrossfadeAutoMs
        )
    }

    /// The stepper's value in the engine state.
    pub fn get(self, s: &AudioState) -> f64 {
        let value = match self {
            MixValue::FadeInMs => s.fade_in_ms,
            MixValue::FadeOutMs => s.fade_out_ms,
            MixValue::SilenceDurationMs => s.silence_duration_ms,
            MixValue::SilenceThresholdDb => s.silence_threshold_db,
            MixValue::RgFixedDb => s.rg_fixed_db,
            MixValue::RgOffsetAlbumDb => s.rg_offset_album_db,
            MixValue::RgOffsetTrackDb => s.rg_offset_track_db,
            MixValue::RgOffsetRtDb => s.rg_offset_rt_db,
            MixValue::CrossfadeManualMs => s.crossfade_manual_ms,
            MixValue::CrossfadeAutoMs => s.crossfade_auto_ms,
        };
        f64::from(value)
    }

    fn field(self, s: &mut AudioState) -> &mut f32 {
        match self {
            MixValue::FadeInMs => &mut s.fade_in_ms,
            MixValue::FadeOutMs => &mut s.fade_out_ms,
            MixValue::SilenceDurationMs => &mut s.silence_duration_ms,
            MixValue::SilenceThresholdDb => &mut s.silence_threshold_db,
            MixValue::RgFixedDb => &mut s.rg_fixed_db,
            MixValue::RgOffsetAlbumDb => &mut s.rg_offset_album_db,
            MixValue::RgOffsetTrackDb => &mut s.rg_offset_track_db,
            MixValue::RgOffsetRtDb => &mut s.rg_offset_rt_db,
            MixValue::CrossfadeManualMs => &mut s.crossfade_manual_ms,
            MixValue::CrossfadeAutoMs => &mut s.crossfade_auto_ms,
        }
    }
}

/// The tab's settings as the engine has them.
#[derive(Debug, Clone, PartialEq)]
pub struct MixState {
    /// The `vol_*` group.
    pub mix: MixSettings,
    /// The crossfade group.
    pub crossfade: CrossfadeSettings,
}

/// The tab's settings as the engine has them (after the startup restore, the
/// persisted values).
pub fn current(ctx: &AppContext) -> MixState {
    let state = ctx.player.snapshot();
    MixState {
        mix: MixSettings::from_state(&state),
        crossfade: CrossfadeSettings::from_state(&state),
    }
}

/// Applies `change` to the engine state, then persists the group it belongs
/// to.
fn change_state(
    ctx: &AppContext,
    crossfade: bool,
    change: impl FnOnce(&mut AudioState),
) -> Reply<()> {
    let entries = {
        let state = ctx.player.state();
        let mut state = state.write();
        change(&mut state);
        if crossfade {
            CrossfadeSettings::from_state(&state).entries()
        } else {
            MixSettings::from_state(&state).entries()
        }
    };
    settings::write_entries(&ctx.db, entries)
}

/// Turns `toggle` on or off, and persists its group.
pub fn set_toggle(ctx: &AppContext, toggle: MixToggle, on: bool) -> Reply<()> {
    change_state(ctx, toggle.is_crossfade(), |state| toggle.set(state, on))
}

/// Sets `value` to `v` (clamped to its range), and persists its group.
pub fn set_value(ctx: &AppContext, value: MixValue, v: f64) -> Reply<()> {
    let range = value.range();
    let v = v.clamp(*range.start(), *range.end());
    change_state(ctx, value.is_crossfade(), |state| {
        *value.field(state) = v as f32;
    })
}

/// Resets `value` to [`MixValue::reset_value`] (a secondary click), and
/// persists its group.
pub fn reset_value(ctx: &AppContext, value: MixValue) -> Reply<()> {
    set_value(ctx, value, value.reset_value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::fakes::InMemoryFakes;
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

    const TOGGLES: [MixToggle; 14] = [
        MixToggle::Fades,
        MixToggle::SmoothVolume,
        MixToggle::FadeIn,
        MixToggle::FadeOut,
        MixToggle::Silence,
        MixToggle::SilenceEdgeTrim,
        MixToggle::RgFixed,
        MixToggle::RgMaster,
        MixToggle::RgTrack,
        MixToggle::RgAlbum,
        MixToggle::RgAnalyzeRt,
        MixToggle::Crossfade,
        MixToggle::CrossfadeManual,
        MixToggle::CrossfadeAuto,
    ];

    #[test]
    fn each_toggle_changes_the_engine_state() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            for toggle in TOGGLES {
                let before = toggle.get(&fakes.player.snapshot());
                set_toggle(&ctx, toggle, !before).recv_blocking().unwrap();
                assert_eq!(toggle.get(&fakes.player.snapshot()), !before, "{toggle:?}");
            }
        });
    }

    #[test]
    fn a_vol_change_persists_the_nineteen_vol_keys_in_the_legacy_ui_format() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_value(&ctx, MixValue::SilenceThresholdDb, -50.25)
                .recv_blocking()
                .unwrap();
            assert_eq!(fakes.player.snapshot().silence_threshold_db, -50.25);
            let stored: Vec<&str> = keys::ALL
                .iter()
                .copied()
                .filter(|key| raw(&ctx, key).is_some())
                .collect();
            assert_eq!(stored.len(), 19);
            assert!(stored.iter().all(|key| key.starts_with("vol_")));
            assert_eq!(
                raw(&ctx, keys::VOL_SILENCE_THRESHOLD_DB).as_deref(),
                Some("-50.25")
            );
            // The other keys hold the engine's values, as Iced writes them.
            assert_eq!(raw(&ctx, keys::VOL_FADE_IN_MS).as_deref(), Some("1000"));
            assert_eq!(raw(&ctx, keys::VOL_SILENCE_ENABLED).as_deref(), Some("1"));
            assert_eq!(raw(&ctx, keys::VOL_RG_FIXED_DB).as_deref(), Some("0.00"));
        });
    }

    #[test]
    fn a_crossfade_change_persists_the_five_crossfade_keys() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_toggle(&ctx, MixToggle::Crossfade, true)
                .recv_blocking()
                .unwrap();
            set_value(&ctx, MixValue::CrossfadeManualMs, 2500.0)
                .recv_blocking()
                .unwrap();
            let state = fakes.player.snapshot();
            assert!(state.crossfade_enabled);
            assert_eq!(state.crossfade_manual_ms, 2500.0);
            let expected = [
                (keys::CROSSFADE_ENABLED, "1"),
                (keys::CROSSFADE_MANUAL_ENABLED, "0"),
                (keys::CROSSFADE_MANUAL_MS, "2500"),
                (keys::CROSSFADE_AUTO_ENABLED, "0"),
                (keys::CROSSFADE_AUTO_MS, "250"),
            ];
            for (key, value) in expected {
                assert_eq!(raw(&ctx, key).as_deref(), Some(value), "{key}");
            }
            assert_eq!(raw(&ctx, keys::VOL_FADES_ENABLED), None);
        });
    }

    #[test]
    fn values_are_clamped_to_the_legacy_ui_ranges() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_value(&ctx, MixValue::SilenceDurationMs, 0.0)
                .recv_blocking()
                .unwrap();
            set_value(&ctx, MixValue::RgFixedDb, -40.0)
                .recv_blocking()
                .unwrap();
            set_value(&ctx, MixValue::RgOffsetTrackDb, 20.0)
                .recv_blocking()
                .unwrap();
            let state = fakes.player.snapshot();
            assert_eq!(state.silence_duration_ms, 100.0);
            assert_eq!(state.rg_fixed_db, -30.0);
            assert_eq!(state.rg_offset_track_db, 12.0);
            assert_eq!(raw(&ctx, keys::VOL_RG_FIXED_DB).as_deref(), Some("-30.00"));
        });
    }

    #[test]
    fn a_secondary_click_resets_to_the_legacy_stepper_values() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            for value in MixValue::ALL {
                set_value(&ctx, value, *value.range().end())
                    .recv_blocking()
                    .unwrap();
                reset_value(&ctx, value).recv_blocking().unwrap();
                assert_eq!(
                    value.get(&fakes.player.snapshot()),
                    value.reset_value(),
                    "{value:?}"
                );
                assert!(value.range().contains(&value.reset_value()));
            }
            assert_eq!(raw(&ctx, keys::VOL_FADE_OUT_MS).as_deref(), Some("2000"));
            assert_eq!(raw(&ctx, keys::CROSSFADE_AUTO_MS).as_deref(), Some("250"));
            assert_eq!(MixValue::FadeInMs.unit(), StepperUnit::Milliseconds);
            assert_eq!(MixValue::RgOffsetRtDb.unit(), StepperUnit::Decibels);
        });
    }

    #[test]
    fn after_a_fresh_install_restore_the_tab_shows_the_fade_lengths_in_effect() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            crate::frontend::commands::engine_settings::restore_persisted(&ctx)
                .recv_blocking()
                .unwrap();
            let state = current(&ctx);
            assert_eq!(state.mix.fade_in_ms, 2000.0);
            assert_eq!(state.mix.fade_out_ms, 3000.0);
        });
    }

    #[test]
    fn the_current_state_reads_the_engine() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            fakes.player.state().write().crossfade_auto_ms = 600.0;
            fakes.player.state().write().fade_out_ms = 4000.0;
            let state = current(&ctx);
            assert_eq!(state.crossfade.auto_ms, 600.0);
            assert_eq!(state.mix.fade_out_ms, 4000.0);
        });
    }
}
