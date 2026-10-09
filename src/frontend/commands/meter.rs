//! The VU meter's settings popup: shown or hidden, peak hold time, RMS window,
//! mid/side display, infinite hold.
//!
//! Each change is applied and persisted as the Iced `Meter*` handlers do
//! (`src/gui/app.rs`, `AudoxidyApp::update`): the engine-state fields
//! (`meter_enabled`, `meter_hold_time_ms`, `meter_rms_window_ms`,
//! `meter_infinite_hold`) are set, and the key is written at once. The
//! mid/side display is not part of the engine state (Iced keeps it in its
//! player UI state): it is only persisted, and the meter view-model keeps it.

#![warn(missing_docs)]

use std::ops::RangeInclusive;

use crate::audio::engine::AudioState;
use crate::frontend::app_context::AppContext;
use crate::frontend::reply::Reply;
use crate::frontend::settings::{self, MeterKey, MeterSettings};

/// The range of the peak hold time stepper, in ms.
pub const HOLD_TIME_RANGE_MS: RangeInclusive<f32> = 500.0..=5000.0;

/// The range of the RMS window stepper, in ms.
pub const RMS_WINDOW_RANGE_MS: RangeInclusive<f32> = 50.0..=1000.0;

/// The persisted meter settings, read on the database worker (the mid/side
/// display is only there).
pub fn load(ctx: &AppContext) -> Reply<MeterSettings> {
    ctx.db.read(|db| Ok(MeterSettings::read(db)))
}

/// Applies `change` to the engine state, then persists `key` as read back
/// from it (with `ms_mode` for the one key the state does not hold).
fn change(
    ctx: &AppContext,
    key: MeterKey,
    ms_mode: bool,
    change: impl FnOnce(&mut AudioState),
) -> Reply<()> {
    let settings = {
        let state = ctx.player.state();
        let mut state = state.write();
        change(&mut state);
        MeterSettings::from_state(&state, ms_mode)
    };
    settings::write_entries(&ctx.db, vec![settings.entry(key)])
}

/// Shows or hides the meter ("Mostrar medidor"), and persists it.
pub fn set_enabled(ctx: &AppContext, enabled: bool) -> Reply<()> {
    change(ctx, MeterKey::Enabled, false, |state| {
        state.meter_enabled = enabled;
    })
}

/// Sets the peak hold time, clamped to [`HOLD_TIME_RANGE_MS`], and persists
/// it.
pub fn set_hold_time_ms(ctx: &AppContext, ms: f32) -> Reply<()> {
    let ms = ms.clamp(*HOLD_TIME_RANGE_MS.start(), *HOLD_TIME_RANGE_MS.end());
    change(ctx, MeterKey::HoldTimeMs, false, |state| {
        // TODO(iced-stub): the value is stored in the engine state and persisted, but the Iced meter never reads it: its peak hold stays the fixed 1.5 s of `update_meter`.
        state.meter_hold_time_ms = ms;
    })
}

/// Sets the RMS window, clamped to [`RMS_WINDOW_RANGE_MS`], and persists it.
pub fn set_rms_window_ms(ctx: &AppContext, ms: f32) -> Reply<()> {
    let ms = ms.clamp(*RMS_WINDOW_RANGE_MS.start(), *RMS_WINDOW_RANGE_MS.end());
    change(ctx, MeterKey::RmsWindowMs, false, |state| {
        // TODO(iced-stub): the value is stored in the engine state and persisted, but nothing in the Iced UI or the engine reads it.
        state.meter_rms_window_ms = ms;
    })
}

/// Turns the mid/side display on or off, and persists it. The engine state is
/// not touched.
pub fn set_ms_mode(ctx: &AppContext, ms_mode: bool) -> Reply<()> {
    change(ctx, MeterKey::MsMode, ms_mode, |_| {})
}

/// Turns the infinite peak hold on or off, and persists it.
pub fn set_infinite_hold(ctx: &AppContext, infinite: bool) -> Reply<()> {
    change(ctx, MeterKey::InfiniteHold, false, |state| {
        state.meter_infinite_hold = infinite;
    })
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

    #[test]
    fn showing_the_meter_sets_the_engine_flag_and_persists_it() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_enabled(&ctx, true).recv_blocking().unwrap();
            assert!(fakes.player.snapshot().meter_enabled);
            assert_eq!(raw(&ctx, keys::METER_ENABLED).as_deref(), Some("1"));
            set_enabled(&ctx, false).recv_blocking().unwrap();
            assert_eq!(raw(&ctx, keys::METER_ENABLED).as_deref(), Some("0"));
        });
    }

    #[test]
    fn the_hold_time_and_rms_window_are_clamped_stored_and_persisted() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_hold_time_ms(&ctx, 100.0).recv_blocking().unwrap();
            set_rms_window_ms(&ctx, 2000.0).recv_blocking().unwrap();
            let state = fakes.player.snapshot();
            assert_eq!(state.meter_hold_time_ms, 500.0);
            assert_eq!(state.meter_rms_window_ms, 1000.0);
            assert_eq!(raw(&ctx, keys::METER_HOLD_TIME_MS).as_deref(), Some("500"));
            assert_eq!(
                raw(&ctx, keys::METER_RMS_WINDOW_MS).as_deref(),
                Some("1000")
            );
            set_hold_time_ms(&ctx, 2350.0).recv_blocking().unwrap();
            assert_eq!(raw(&ctx, keys::METER_HOLD_TIME_MS).as_deref(), Some("2350"));
        });
    }

    #[test]
    fn the_mid_side_display_is_persisted_without_touching_the_engine() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let before = fakes.player.snapshot();
            set_ms_mode(&ctx, true).recv_blocking().unwrap();
            assert_eq!(raw(&ctx, keys::METER_MS_MODE).as_deref(), Some("1"));
            let after = fakes.player.snapshot();
            assert_eq!(
                MeterSettings::from_state(&before, false),
                MeterSettings::from_state(&after, false)
            );
            assert!(load(&ctx).recv_blocking().unwrap().ms_mode);
            assert_eq!(raw(&ctx, keys::METER_ENABLED), None, "only its own key");
        });
    }

    #[test]
    fn the_infinite_hold_sets_the_engine_flag_and_persists_it() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            set_infinite_hold(&ctx, true).recv_blocking().unwrap();
            assert!(fakes.player.snapshot().meter_infinite_hold);
            assert_eq!(raw(&ctx, keys::METER_INFINITE_HOLD).as_deref(), Some("1"));
            let loaded = load(&ctx).recv_blocking().unwrap();
            assert!(loaded.infinite_hold);
            assert_eq!(loaded.hold_time_ms, 1500.0, "the default when never set");
        });
    }
}
