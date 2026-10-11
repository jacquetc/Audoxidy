use super::*;
use crate::frontend::settings::{self, PersistedSettings};
use crate::teksilo_ui::audio_center::shell::testing::services_with_fakes;

fn fixture() -> (VolumeMixViewModel, AppServices) {
    let (services, _fakes) = services_with_fakes();
    (VolumeMixViewModel::new(&services), services)
}

/// What the database holds, after every change made so far (the facade's
/// database worker takes its jobs in order).
fn stored(services: &AppServices) -> PersistedSettings {
    settings::load_all(&services.ctx.db)
        .recv_blocking()
        .unwrap()
}

fn stored_toggle(s: &PersistedSettings, toggle: MixToggle) -> bool {
    match toggle {
        MixToggle::Fades => s.mix.fades_enabled,
        MixToggle::SmoothVolume => s.mix.smooth_volume_enabled,
        MixToggle::FadeIn => s.mix.fade_in_enabled,
        MixToggle::FadeOut => s.mix.fade_out_enabled,
        MixToggle::Silence => s.mix.silence_enabled,
        MixToggle::SilenceEdgeTrim => s.mix.silence_edge_trim_enabled,
        MixToggle::RgFixed => s.mix.rg_fixed_enabled,
        MixToggle::RgMaster => s.mix.rg_master_enabled,
        MixToggle::RgTrack => s.mix.rg_track_enabled,
        MixToggle::RgAlbum => s.mix.rg_album_enabled,
        MixToggle::RgAnalyzeRt => s.mix.rg_analyze_rt_enabled,
        MixToggle::Crossfade => s.crossfade.enabled,
        MixToggle::CrossfadeManual => s.crossfade.manual_enabled,
        MixToggle::CrossfadeAuto => s.crossfade.auto_enabled,
    }
}

fn stored_value(s: &PersistedSettings, value: MixValue) -> f64 {
    f64::from(match value {
        MixValue::FadeInMs => s.mix.fade_in_ms,
        MixValue::FadeOutMs => s.mix.fade_out_ms,
        MixValue::SilenceDurationMs => s.mix.silence_duration_ms,
        MixValue::SilenceThresholdDb => s.mix.silence_threshold_db,
        MixValue::RgFixedDb => s.mix.rg_fixed_db,
        MixValue::RgOffsetAlbumDb => s.mix.rg_offset_album_db,
        MixValue::RgOffsetTrackDb => s.mix.rg_offset_track_db,
        MixValue::RgOffsetRtDb => s.mix.rg_offset_rt_db,
        MixValue::CrossfadeManualMs => s.crossfade.manual_ms,
        MixValue::CrossfadeAutoMs => s.crossfade.auto_ms,
    })
}

fn engine_toggle(services: &AppServices, toggle: MixToggle) -> bool {
    toggle.get(&services.ctx.player.snapshot())
}

fn engine_value(services: &AppServices, value: MixValue) -> f64 {
    value.get(&services.ctx.player.snapshot())
}

#[test]
fn the_tables_agree_with_the_enums() {
    for (at, toggle) in TOGGLES.iter().enumerate() {
        assert_eq!(toggle_index(*toggle), at);
    }
    for (at, value) in MixValue::ALL.iter().enumerate() {
        assert_eq!(value_index(*value), at);
    }
}

#[test]
fn the_tab_shows_the_values_the_engine_uses() {
    // Quirk fix 21: a fresh install fades over the engine's lengths.
    let (vm, services) = fixture();
    let state = services.ctx.player.snapshot();
    for value in MixValue::ALL {
        assert_eq!(vm.value(value).get(), value.get(&state), "{value:?}");
    }
    for toggle in TOGGLES {
        assert_eq!(vm.is_on(toggle), toggle.get(&state), "{toggle:?}");
    }
    assert!(services.ctx.shutdown());
}

#[test]
fn showing_the_tab_reads_the_engine_again() {
    let (vm, services) = fixture();
    mix::set_value(&services.ctx, MixValue::FadeInMs, 4321.0)
        .recv_blocking()
        .unwrap();
    assert_ne!(vm.value(MixValue::FadeInMs).get(), 4321.0);
    vm.on_shown();
    assert_eq!(vm.value(MixValue::FadeInMs).get(), 4321.0);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_tab_is_shown_only_while_the_panel_is_open_on_it() {
    let (vm, services) = fixture();
    let shown = vm.shown();
    assert!(!shown.get());
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::VOLUME_MIX));
    assert!(shown.get());
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::CONFIG));
    assert!(!shown.get());
    assert!(services.ctx.shutdown());
}

// ---- persistence on every change ----

#[test]
fn every_switch_is_applied_and_stored_the_moment_it_changes() {
    let (vm, services) = fixture();
    // The masters first, so that their groups are unlocked.
    for master in [
        MixToggle::Fades,
        MixToggle::Silence,
        MixToggle::RgFixed,
        MixToggle::RgMaster,
        MixToggle::Crossfade,
    ] {
        vm.set_toggle(master, true);
    }
    for toggle in TOGGLES {
        let flipped = !vm.is_on(toggle);
        assert!(vm.set_toggle(toggle, flipped), "{toggle:?}");
        assert_eq!(
            engine_toggle(&services, toggle),
            flipped,
            "{toggle:?} engine"
        );
        assert_eq!(
            stored_toggle(&stored(&services), toggle),
            flipped,
            "{toggle:?} stored"
        );
        vm.set_toggle(toggle, !flipped);
        assert_eq!(
            stored_toggle(&stored(&services), toggle),
            !flipped,
            "{toggle:?} stored back"
        );
    }
    assert!(services.ctx.shutdown());
}

#[test]
fn every_value_is_applied_and_stored_the_moment_it_changes() {
    let (vm, services) = fixture();
    for value in MixValue::ALL {
        let range = value.range();
        let to = (range.start() + range.end()) / 2.0;
        vm.value_changed(value, to);
        assert_eq!(vm.value(value).get(), to, "{value:?}");
        assert_eq!(engine_value(&services, value), to, "{value:?} engine");
        assert_eq!(
            stored_value(&stored(&services), value),
            to,
            "{value:?} stored"
        );
    }
    assert!(services.ctx.shutdown());
}

#[test]
fn the_crossfade_group_is_stored_under_its_own_keys() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Crossfade, true);
    vm.value_changed(MixValue::CrossfadeAutoMs, 400.0);
    let before = stored(&services);
    assert!(before.crossfade.enabled);
    assert_eq!(before.crossfade.auto_ms, 400.0);
    vm.set_toggle(MixToggle::Fades, true);
    let after = stored(&services);
    assert_eq!(after.crossfade, before.crossfade);
    assert!(after.mix.fades_enabled);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_value_is_clamped_to_its_steppers_range() {
    let (vm, services) = fixture();
    vm.value_changed(MixValue::FadeInMs, 99_999.0);
    assert_eq!(vm.value(MixValue::FadeInMs).get(), 10_000.0);
    vm.value_changed(MixValue::SilenceDurationMs, 5.0);
    assert_eq!(vm.value(MixValue::SilenceDurationMs).get(), 100.0);
    vm.value_changed(MixValue::SilenceThresholdDb, 12.0);
    assert_eq!(vm.value(MixValue::SilenceThresholdDb).get(), 0.0);
    vm.value_changed(MixValue::RgFixedDb, -99.0);
    assert_eq!(vm.value(MixValue::RgFixedDb).get(), -30.0);
    let s = stored(&services);
    assert_eq!(stored_value(&s, MixValue::FadeInMs), 10_000.0);
    assert_eq!(stored_value(&s, MixValue::RgFixedDb), -30.0);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_reset_goes_back_to_the_value_of_the_iced_stepper() {
    let (vm, services) = fixture();
    let resets = [
        (MixValue::FadeInMs, 1000.0),
        (MixValue::FadeOutMs, 2000.0),
        (MixValue::SilenceDurationMs, 1000.0),
        (MixValue::SilenceThresholdDb, -47.0),
        (MixValue::RgFixedDb, 0.0),
        (MixValue::RgOffsetAlbumDb, 0.0),
        (MixValue::RgOffsetTrackDb, 0.0),
        (MixValue::RgOffsetRtDb, 0.0),
        (MixValue::CrossfadeManualMs, 1000.0),
        (MixValue::CrossfadeAutoMs, 250.0),
    ];
    for (value, expected) in resets {
        vm.value_changed(value, expected + 1.5);
        vm.reset_value(value);
        assert_eq!(vm.value(value).get(), expected, "{value:?}");
        assert_eq!(engine_value(&services, value), expected, "{value:?} engine");
        assert_eq!(
            stored_value(&stored(&services), value),
            expected,
            "{value:?} stored"
        );
    }
    assert!(services.ctx.shutdown());
}

// ---- the locks ----

#[test]
fn a_group_is_locked_while_its_master_is_off() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Fades, false);
    for locked in [
        MixToggle::SmoothVolume,
        MixToggle::FadeIn,
        MixToggle::FadeOut,
    ] {
        assert!(!vm.is_unlocked(locked));
        assert!(!vm.unlocked(locked).get());
        assert!(!vm.set_toggle(locked, true), "{locked:?} ignores the click");
        assert!(!vm.is_on(locked), "{locked:?} stays as it was");
        assert!(!vm.checked(locked).get());
        assert!(!vm.lit(locked).get());
    }
    assert!(!vm.value_enabled(MixValue::FadeInMs).get());
    assert!(!vm.value_enabled(MixValue::FadeOutMs).get());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_locked_switch_is_drawn_unchecked_and_comes_back_with_its_master() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Fades, true);
    vm.set_toggle(MixToggle::SmoothVolume, true);
    assert!(vm.checked(MixToggle::SmoothVolume).get());
    vm.set_toggle(MixToggle::Fades, false);
    assert!(
        !vm.checked(MixToggle::SmoothVolume).get(),
        "unchecked while locked"
    );
    assert!(
        vm.is_on(MixToggle::SmoothVolume),
        "but the engine keeps it, as Iced does"
    );
    vm.set_toggle(MixToggle::Fades, true);
    assert!(
        vm.checked(MixToggle::SmoothVolume).get(),
        "back with its master"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_stepper_needs_its_master_and_its_own_switch() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Fades, true);
    vm.set_toggle(MixToggle::FadeIn, false);
    assert!(
        !vm.value_enabled(MixValue::FadeInMs).get(),
        "own switch off"
    );
    vm.set_toggle(MixToggle::FadeIn, true);
    assert!(vm.value_enabled(MixValue::FadeInMs).get());
    vm.set_toggle(MixToggle::Fades, false);
    assert!(!vm.value_enabled(MixValue::FadeInMs).get(), "master off");
    assert!(!vm.value_lit(MixValue::FadeInMs).get());
    assert!(services.ctx.shutdown());
}

#[test]
fn the_silence_lengths_and_the_preamp_level_depend_on_the_master_only() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Silence, true);
    assert!(vm.value_enabled(MixValue::SilenceDurationMs).get());
    assert!(vm.value_enabled(MixValue::SilenceThresholdDb).get());
    vm.set_toggle(MixToggle::Silence, false);
    assert!(!vm.value_enabled(MixValue::SilenceThresholdDb).get());
    vm.set_toggle(MixToggle::RgFixed, false);
    assert!(!vm.value_enabled(MixValue::RgFixedDb).get());
    vm.set_toggle(MixToggle::RgFixed, true);
    assert!(vm.value_enabled(MixValue::RgFixedDb).get());
    assert!(services.ctx.shutdown());
}

#[test]
fn the_three_replay_gain_rows_and_the_two_crossfades_follow_their_masters() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::RgMaster, true);
    for (toggle, value) in [
        (MixToggle::RgAlbum, MixValue::RgOffsetAlbumDb),
        (MixToggle::RgTrack, MixValue::RgOffsetTrackDb),
        (MixToggle::RgAnalyzeRt, MixValue::RgOffsetRtDb),
    ] {
        vm.set_toggle(toggle, true);
        assert!(vm.value_enabled(value).get(), "{value:?}");
        vm.set_toggle(MixToggle::RgMaster, false);
        assert!(!vm.value_enabled(value).get(), "{value:?} locked");
        vm.set_toggle(MixToggle::RgMaster, true);
    }
    vm.set_toggle(MixToggle::Crossfade, true);
    vm.set_toggle(MixToggle::CrossfadeManual, true);
    vm.set_toggle(MixToggle::CrossfadeAuto, false);
    assert!(vm.value_enabled(MixValue::CrossfadeManualMs).get());
    assert!(!vm.value_enabled(MixValue::CrossfadeAutoMs).get());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_click_on_a_label_flips_an_unlocked_switch_and_nothing_else() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Fades, true);
    assert!(vm.flip(MixToggle::FadeIn));
    assert!(vm.is_on(MixToggle::FadeIn));
    assert!(vm.flip(MixToggle::FadeIn));
    assert!(!vm.is_on(MixToggle::FadeIn));
    assert!(vm.flip(MixToggle::Fades), "a title flips its master");
    assert!(!vm.is_on(MixToggle::Fades));
    assert!(!vm.flip(MixToggle::FadeIn), "locked again");
    assert!(!vm.is_on(MixToggle::FadeIn));
    assert!(services.ctx.shutdown());
}

#[test]
fn a_label_is_lit_while_its_control_is_active() {
    let (vm, services) = fixture();
    vm.set_toggle(MixToggle::Fades, true);
    vm.set_toggle(MixToggle::FadeOut, false);
    assert!(!vm.lit(MixToggle::FadeOut).get());
    vm.set_toggle(MixToggle::FadeOut, true);
    assert!(vm.lit(MixToggle::FadeOut).get());
    assert!(services.ctx.shutdown());
}
