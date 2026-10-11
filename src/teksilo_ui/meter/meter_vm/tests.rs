//! Headless tests of [`MeterViewModel`]: frames over a scripted clock fed by
//! `FakePlayer::set_meter`, the activity lifecycle, the clip LED, Mid/Side and
//! correlation from the linear statistics, and the five settings.

use std::time::{Duration, Instant};

use teksilo::core::WidgetTree;

use super::*;
use crate::audio::meter::StereoReading;
use crate::frontend::AppContext;
use crate::frontend::fakes::InMemoryFakes;
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::keys;
use crate::logic::meter::{METER_FADE_OUT_SECS, METER_MIN_DB};
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{press, run_until, tree_with_async};

struct Fixture {
    services: AppServices,
    fakes: InMemoryFakes,
    tree: WidgetTree,
    vm: MeterViewModel,
    t0: Instant,
}

impl Fixture {
    fn new() -> Self {
        Self::with_engine(|_| {})
    }

    /// A view-model built after `engine` changed the player's state.
    fn with_engine(engine: impl FnOnce(&mut crate::audio::engine::AudioState)) -> Self {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        engine(&mut fakes.player.state().write());
        let (tree, runtime) = tree_with_async(&ctx);
        let services = AppServices::new(&AppHandles::new(ctx), runtime);
        let vm = MeterViewModel::new(&services);
        Self {
            services,
            fakes,
            tree,
            vm,
            t0: Instant::now(),
        }
    }

    fn play(&self, path: &str) {
        let now_playing = &self.services.now_playing;
        now_playing.path().set(path.to_string());
        now_playing.is_playing().set(true);
    }

    fn pause(&self) {
        self.services.now_playing.is_playing().set(false);
    }

    /// Ticks at `t0 + ms`.
    fn tick_at(&self, ms: u64) {
        self.vm.tick(self.t0 + Duration::from_millis(ms));
    }

    /// Ticks every 30 ms from `from` to `to` (inclusive).
    fn run(&self, from: u64, to: u64) {
        let mut ms = from;
        while ms <= to {
            self.tick_at(ms);
            ms += 30;
        }
    }

    fn frame(&self) -> VuMeterFrame {
        self.vm.frame().get()
    }

    /// Runs `f` as a handler would, with a real `EventContext`.
    fn act(&mut self, f: impl Fn(&MeterViewModel, &mut EventContext) + 'static) {
        let vm = self.vm.clone();
        press(&mut self.tree, move |ctx| f(&vm, ctx));
    }

    /// What the database holds under `key`, after every write queued before.
    fn stored(&self, key: &'static str) -> Option<String> {
        self.services
            .ctx
            .db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    fn settle(&self) {
        run_until(&self.services.runtime, Duration::from_millis(300), || false);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.services.ctx.shutdown();
    }
}

// --- the frames ---------------------------------------------------------------------------

#[test]
fn an_idle_meter_shows_silence_and_is_not_active() {
    let f = Fixture::new();
    assert_eq!(f.frame(), frame_of(&LiveMeter::default(), false));
    assert_eq!(f.frame().level_db, [METER_MIN_DB; 2]);
    assert!(!f.vm.active().get());
}

#[test]
fn the_bars_show_the_rms_and_the_readout_the_peak() {
    // D6: the Iced bars always read 0 because the widget gets `smooth_peak`.
    let f = Fixture::new();
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.play("/a.flac");
    f.run(0, 3000);
    let frame = f.frame();
    assert!((frame.level_db[0] - -18.0).abs() < 0.05, "{frame:?}");
    assert!((frame.level_db[1] - -18.0).abs() < 0.05);
    assert!((frame.peak_db[0] - -6.0).abs() < 0.05);
    assert!((frame.crest_db[0] - 12.0).abs() < 0.05);
    assert!(f.vm.active().get());
}

#[test]
fn the_hold_marker_sits_at_the_peak_then_falls_with_time() {
    // D5: the Iced hold never decays and pins the bars at 0 dBFS.
    let f = Fixture::new();
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.play("/a.flac");
    f.run(0, 300);
    assert_eq!(f.frame().hold_db[0], -6.0);
    f.fakes.player.set_meter(-40.0, -50.0, -40.0, -50.0);
    f.run(330, 1500);
    assert_eq!(f.frame().hold_db[0], -6.0, "held for 1.5 s");
    f.run(1530, 11_500);
    let hold = f.frame().hold_db[0];
    assert!(hold < -6.0 - 15.0 && hold > -40.0 - 1.0, "{hold}");
    assert!(f.frame().level_db[0] < f.frame().hold_db[0]);
}

#[test]
fn the_frames_follow_elapsed_time_not_the_number_of_ticks() {
    let a = Fixture::new();
    let b = Fixture::new();
    for f in [&a, &b] {
        f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
        f.play("/a.flac");
    }
    a.tick_at(0);
    b.tick_at(0);
    for ms in (10..=1000).step_by(10) {
        a.tick_at(ms);
    }
    for ms in (100..=1000).step_by(100) {
        b.tick_at(ms);
    }
    assert!((a.frame().level_db[0] - b.frame().level_db[0]).abs() < 0.05);
}

#[test]
fn pausing_fades_the_bars_to_silence_then_the_meter_goes_idle() {
    // D7: the Iced fade multiplies the dB value by a factor going to 0.
    let f = Fixture::new();
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.play("/a.flac");
    f.run(0, 3000);
    let before = f.frame().level_db[0];
    assert!(f.vm.active().get());

    f.pause();
    let mut previous = before;
    let mut ms = 3030;
    while ms <= 3000 + 300 {
        f.tick_at(ms);
        let level = f.frame().level_db[0];
        assert!(level <= previous + 1e-4 && level <= before, "{level} rose");
        assert!(f.vm.active().get(), "still fading at {ms}");
        previous = level;
        ms += 30;
    }
    f.tick_at(3000 + (METER_FADE_OUT_SECS * 1000.0) as u64 + 60);
    assert_eq!(f.frame().level_db, [METER_MIN_DB; 2]);
    assert_eq!(f.frame().peak_db, [METER_MIN_DB; 2]);
    assert!(!f.vm.active().get(), "idle once the fade is over");
}

#[test]
fn a_new_track_starts_the_meter_afresh() {
    let f = Fixture::new();
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.play("/a.flac");
    f.run(0, 600);
    assert!(f.frame().level_db[0] > METER_MIN_DB);
    f.play("/b.flac");
    f.tick_at(630);
    assert_eq!(f.frame().level_db, [METER_MIN_DB; 2]);
    assert_eq!(f.frame().hold_db, [METER_MIN_DB; 2]);
}

#[test]
fn the_clip_led_follows_the_peaks_and_reset_peak_clears_it() {
    // Iced hard-codes `clipping = false` (`app.rs:5147`).
    let f = Fixture::new();
    f.play("/a.flac");
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.run(0, 300);
    assert!(!f.frame().clipping);
    f.fakes.player.set_meter(0.5, -12.0, -6.0, -18.0);
    f.run(330, 600);
    assert!(f.frame().clipping);
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.run(630, 1200);
    assert!(f.frame().clipping, "lit for the hold time");
    f.vm.reset_peaks();
    assert!(!f.frame().clipping);
    assert_eq!(f.frame().hold_db, [METER_MIN_DB; 2]);
}

#[test]
fn the_correlation_comes_from_the_linear_statistics() {
    // D8: the Iced correlation is the sign product of two dB values.
    let f = Fixture::new();
    f.play("/a.flac");
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.fakes.player.meter().write_stereo(&StereoReading {
        correlation: 0.37,
        ..StereoReading::SILENCE
    });
    f.run(0, 300);
    assert!((f.frame().correlation - 0.37).abs() < 1e-6);
    f.fakes.player.meter().write_stereo(&StereoReading {
        correlation: -0.9,
        ..StereoReading::SILENCE
    });
    f.run(330, 600);
    assert!((f.frame().correlation - -0.9).abs() < 1e-6);
}

#[test]
fn mid_side_mode_draws_the_linear_mid_and_side_levels() {
    let mut f = Fixture::new();
    f.play("/a.flac");
    f.fakes.player.set_meter(-6.0, -9.0, -6.0, -9.0);
    // A mono signal: everything is mid.
    f.fakes.player.meter().write_stereo(&StereoReading {
        correlation: 1.0,
        mid_rms_db: -9.0,
        side_rms_db: METER_MIN_DB,
        mid_peak_db: -6.0,
        side_peak_db: METER_MIN_DB,
    });
    f.act(|vm, ctx| vm.set_ms_mode(true, ctx));
    f.run(0, 3000);
    let frame = f.frame();
    assert!(frame.mid_side);
    assert!((frame.level_db[0] - -9.0).abs() < 0.05, "{frame:?}");
    assert!(frame.level_db[1] < -59.0);
    assert!((frame.peak_db[0] - -6.0).abs() < 0.05);
    assert!(frame.peak_db[1] < -59.0);
}

#[test]
fn infinite_hold_keeps_the_marker_for_ever() {
    let mut f = Fixture::new();
    f.act(|vm, ctx| vm.set_infinite_hold(true, ctx));
    f.play("/a.flac");
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.run(0, 300);
    f.fakes.player.set_meter(-50.0, -60.0, -50.0, -60.0);
    f.run(330, 60_000);
    assert_eq!(f.frame().hold_db[0], -6.0);
}

#[test]
fn a_silent_never_written_meter_reads_the_floor() {
    // `MeterData` starts at -inf.
    let f = Fixture::new();
    f.play("/a.flac");
    f.run(0, 300);
    assert_eq!(f.frame().level_db, [METER_MIN_DB; 2]);
    assert_eq!(f.frame().correlation, 0.0);
    assert!(!f.frame().clipping);
}

// --- the settings -------------------------------------------------------------------------

#[test]
fn the_settings_start_from_the_engine() {
    let f = Fixture::with_engine(|state| {
        state.meter_enabled = true;
        state.meter_hold_time_ms = 2500.0;
        state.meter_rms_window_ms = 120.0;
        state.meter_infinite_hold = true;
    });
    assert!(f.vm.enabled().get());
    assert_eq!(f.vm.hold_time_ms().get(), 2500.0);
    assert_eq!(f.vm.rms_window_ms().get(), 120.0);
    assert!(f.vm.infinite_hold().get());
    assert!(!f.vm.ms_mode().get());
}

#[test]
fn the_mid_side_mode_is_read_from_the_database() {
    let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
    crate::frontend::commands::meter::set_ms_mode(&ctx, true)
        .recv_blocking()
        .unwrap();
    let (_tree, runtime) = tree_with_async(&ctx);
    let services = AppServices::new(&AppHandles::new(ctx), runtime);
    let vm = MeterViewModel::new(&services);
    assert!(run_until(&services.runtime, Duration::from_secs(5), || {
        vm.ms_mode().get()
    }));
    assert!(vm.frame().get().mid_side);
    drop(fakes);
    services.ctx.shutdown();
}

#[test]
fn a_late_read_does_not_undo_a_change_made_meanwhile() {
    let mut f = Fixture::new();
    f.act(|vm, ctx| vm.set_enabled(true, ctx));
    f.settle();
    assert!(f.vm.enabled().get());
    assert_eq!(f.stored(keys::METER_ENABLED).as_deref(), Some("1"));
}

#[test]
fn showing_the_meter_sets_the_engine_and_persists_it() {
    // Quirk fix 10: it can be enabled on a fresh install.
    let mut f = Fixture::new();
    assert!(!f.vm.enabled().get());
    f.act(|vm, ctx| vm.set_enabled(true, ctx));
    assert!(f.vm.enabled().get());
    assert!(f.fakes.player.snapshot().meter_enabled);
    assert_eq!(f.stored(keys::METER_ENABLED).as_deref(), Some("1"));
    f.act(|vm, ctx| vm.set_enabled(false, ctx));
    assert_eq!(f.stored(keys::METER_ENABLED).as_deref(), Some("0"));
}

#[test]
fn the_hold_time_and_the_rms_window_are_clamped_stored_and_persisted() {
    let f = Fixture::new();
    f.vm.set_hold_time_ms(100.0);
    f.vm.set_rms_window_ms(5000.0);
    assert_eq!(f.vm.hold_time_ms().get(), 500.0);
    assert_eq!(f.vm.rms_window_ms().get(), 1000.0);
    f.vm.set_hold_time_ms(2350.0);
    f.vm.set_rms_window_ms(75.0);
    let state = f.fakes.player.snapshot();
    assert_eq!(state.meter_hold_time_ms, 2350.0);
    assert_eq!(state.meter_rms_window_ms, 75.0);
    assert_eq!(f.stored(keys::METER_HOLD_TIME_MS).as_deref(), Some("2350"));
    assert_eq!(f.stored(keys::METER_RMS_WINDOW_MS).as_deref(), Some("75"));
}

#[test]
fn the_stub_settings_do_not_change_the_meter() {
    // The hold time and RMS window are Iced stubs: stored, never read.
    let f = Fixture::new();
    f.play("/a.flac");
    f.fakes.player.set_meter(-6.0, -18.0, -6.0, -18.0);
    f.run(0, 300);
    f.vm.set_hold_time_ms(5000.0);
    f.fakes.player.set_meter(-40.0, -50.0, -40.0, -50.0);
    f.run(330, 1500 + 6000);
    assert!(f.frame().hold_db[0] < -6.0, "still the fixed 1.5 s hold");
}

#[test]
fn the_mid_side_mode_and_the_infinite_hold_persist() {
    let mut f = Fixture::new();
    f.act(|vm, ctx| vm.set_ms_mode(true, ctx));
    f.act(|vm, ctx| vm.set_infinite_hold(true, ctx));
    assert_eq!(f.stored(keys::METER_MS_MODE).as_deref(), Some("1"));
    assert_eq!(f.stored(keys::METER_INFINITE_HOLD).as_deref(), Some("1"));
    assert!(f.fakes.player.snapshot().meter_infinite_hold);
    assert!(f.frame().mid_side, "the idle frame follows the mode");
    f.act(|vm, ctx| vm.set_ms_mode(false, ctx));
    assert_eq!(f.stored(keys::METER_MS_MODE).as_deref(), Some("0"));
}

#[test]
fn a_failed_save_is_logged_and_does_not_panic_without_a_window() {
    let mut f = Fixture::new();
    f.services.ctx.shutdown();
    f.act(|vm, ctx| vm.set_enabled(true, ctx));
    f.vm.set_hold_time_ms(1000.0);
    f.settle();
    assert!(f.vm.enabled().get(), "the control still shows the choice");
}
