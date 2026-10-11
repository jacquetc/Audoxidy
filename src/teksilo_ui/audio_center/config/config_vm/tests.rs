use std::time::Duration;

use super::*;
use crate::audio::engine::AudioDeviceInfo;
use crate::frontend::fakes::{InMemoryFakes, PlayerCall, RecordedSettings, SystemAudioCall};
use crate::frontend::ports::{PlayerPort, ReplayGains, TrackRequest};
use crate::frontend::settings::keys;
use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};
use crate::teksilo_ui::audio_center::shell::testing::{services_with_fakes, tree_for};
use crate::teksilo_ui::test_support::{layout, press, run_until};

const TIMEOUT: Duration = Duration::from_secs(10);

fn device(name: &str, id: &str) -> AudioDeviceInfo {
    AudioDeviceInfo {
        name: name.to_string(),
        id: Some(id.to_string()),
    }
}

/// A view-model over fakes with two hosts and two devices. The tab is not
/// mounted: the tests call what the tab calls.
fn fixture() -> (ConfigViewModel, AppServices, InMemoryFakes) {
    let (services, fakes) = services_with_fakes();
    fakes
        .player
        .set_hosts(vec!["Fake".to_string(), "ALSA".to_string()]);
    fakes.player.set_devices(vec![
        device("Fake Output", "fake-output"),
        device("USB DAC", "usb-dac"),
    ]);
    let vm = ConfigViewModel::new(&services);
    (vm, services, fakes)
}

/// Shows the tab and waits for the engine's state to be adopted, as the tab's
/// `build` does.
fn show(vm: &ConfigViewModel, services: &AppServices) {
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::CONFIG));
    vm.on_shown();
    let devices = services.audio_devices.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || devices
        .state()
        .get()
        .is_some()));
    vm.adopt_state();
}

// ---- the selections ----

#[test]
fn the_selections_start_as_iceds_manager_does_with_apply_disabled() {
    let (vm, services, _fakes) = fixture();
    assert_eq!(vm.pending(), PendingAudioSettings::default());
    assert!(!vm.apply_enabled().get());
    assert!(!vm.busy().get());
    assert!(services.ctx.shutdown());
}

#[test]
fn the_first_show_adopts_the_engines_state_as_iceds_first_open_does() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    let pending = vm.pending();
    assert_eq!(pending.host.as_deref(), Some("ALSA"), "ALSA when listed");
    assert_eq!(pending.device.as_deref(), Some("Fake Output"));
    assert_eq!(pending.device_id.as_deref(), Some("fake-output"));
    assert_eq!(pending.sample_rate, Some(48_000), "the stream's rate");
    assert_eq!(pending.channels, 2, "the stream's channels");
    assert!(!vm.apply_enabled().get(), "adopting enables nothing");
    assert!(services.ctx.shutdown());
}

#[test]
fn the_selections_are_read_once_and_a_later_answer_keeps_the_users_choices() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    vm.host().set(Some("Fake".to_string()));
    vm.selection_changed();
    services.audio_devices.reload();
    let version = services.audio_devices.version().get();
    let devices = services.audio_devices.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || devices
        .version()
        .get()
        > version));
    vm.adopt_state();
    assert_eq!(vm.pending().host.as_deref(), Some("Fake"));
    assert!(vm.apply_enabled().get());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_state_that_was_loaded_before_the_first_show_is_not_the_first_open() {
    // The devices model loads when the app starts. The selections wait for
    // the answer to the request of the first show, which reads the saved
    // settings at that moment.
    let (vm, services, _fakes) = fixture();
    services.audio_devices.reload();
    let devices = services.audio_devices.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || devices
        .state()
        .get()
        .is_some()));
    vm.adopt_state();
    assert_eq!(vm.pending(), PendingAudioSettings::default());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_saved_device_that_is_not_listed_stays_selected_like_iceds_pick_list() {
    let (services, fakes) = services_with_fakes();
    LibraryFixture::new()
        .setting(keys::AUDIO_DEVICE, "Unplugged DAC")
        .seed_on(&services.ctx.db)
        .recv_blocking()
        .unwrap();
    fakes
        .player
        .set_devices(vec![device("Fake Output", "fake-output")]);
    let vm = ConfigViewModel::new(&services);
    show(&vm, &services);
    assert_eq!(vm.pending().device.as_deref(), Some("Unplugged DAC"));
    assert_eq!(vm.pending().device_id, None);
    assert!(services.ctx.shutdown());
}

#[test]
fn choosing_a_selection_enables_apply() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    vm.sample_rate().set(Some(96_000));
    vm.selection_changed();
    assert!(vm.apply_enabled().get());
    assert_eq!(vm.pending().sample_rate, Some(96_000));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_device_id_is_looked_up_by_name_when_the_pending_settings_are_read() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    vm.device().set(Some("USB DAC".to_string()));
    assert_eq!(vm.pending().device_id.as_deref(), Some("usb-dac"));
    assert!(services.ctx.shutdown());
}

#[test]
fn predeterminado_selects_the_defaults_and_enables_apply() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    vm.host().set(Some("Fake".to_string()));
    vm.device().set(Some("USB DAC".to_string()));
    vm.sample_rate().set(Some(192_000));
    vm.bit_depth().set(Some(BitDepth::Bits16));
    vm.channels().set(Some(6));
    vm.buffer_size().set(Some(Some(2048)));
    vm.system_rate().set(Some(SystemSelection::Fixed(44_100)));
    vm.system_quantum().set(Some(SystemSelection::Automatic));
    assert!(!vm.apply_enabled().get());

    vm.reset_to_defaults();

    let pending = vm.pending();
    assert_eq!(pending.sample_rate, Some(rules::DEFAULT_SAMPLE_RATE));
    assert_eq!(pending.bit_depth, rules::DEFAULT_BIT_DEPTH);
    assert_eq!(pending.buffer_size, rules::DEFAULT_BUFFER_SIZE);
    assert_eq!(pending.channels, rules::DEFAULT_CHANNELS);
    assert_eq!(pending.system_rate, SystemSelection::Default);
    assert_eq!(pending.system_quantum, SystemSelection::Default);
    assert_eq!(pending.host.as_deref(), Some("ALSA"), "the preferred host");
    assert_eq!(pending.device.as_deref(), Some("Fake Output"));
    assert_eq!(pending.device_id.as_deref(), Some("fake-output"));
    assert!(vm.apply_enabled().get());
    assert!(services.ctx.shutdown());
}

// ---- Apply ----

/// Delivers `result` to `finish_apply` with a real event context.
fn finish(
    vm: &ConfigViewModel,
    tree: &mut teksilo::core::WidgetTree,
    result: Result<ApplyOutcome, FrontendError>,
) {
    let slot = std::rc::Rc::new(RefCell::new(Some(result)));
    let vm = vm.clone();
    press(tree, move |ctx| {
        if let Some(result) = slot.borrow_mut().take() {
            vm.finish_apply(result, ctx);
        }
    });
}

fn apply_and_wait(vm: &ConfigViewModel) -> Result<ApplyOutcome, FrontendError> {
    vm.begin_apply()
        .expect("apply is available")
        .recv_blocking()
}

#[test]
fn apply_forces_the_clock_then_rebuilds_the_stream_then_persists_in_iceds_order() {
    let (vm, services, fakes) = fixture();
    show(&vm, &services);
    vm.device().set(Some("USB DAC".to_string()));
    vm.sample_rate().set(Some(96_000));
    vm.system_rate().set(Some(SystemSelection::Automatic));
    vm.system_quantum().set(Some(SystemSelection::Automatic));
    vm.selection_changed();
    fakes.player.take_calls();

    let outcome = apply_and_wait(&vm).unwrap();

    assert_eq!(outcome.clock.rate, Some(96_000));
    let names: Vec<_> = fakes.system_audio.calls();
    let clock_calls: Vec<_> = names
        .iter()
        .filter(|call| !matches!(call, SystemAudioCall::SystemSoundStatus))
        .cloned()
        .collect();
    assert_eq!(
        clock_calls,
        vec![
            SystemAudioCall::IsPipewireActive,
            SystemAudioCall::ApplyPipewireClock(Some(96_000), Some(1024)),
            SystemAudioCall::PersistPipewireConf(Some(96_000), Some(1024)),
        ],
        "the sound server first, so the stream connects to the forced clock"
    );
    let calls = fakes.player.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let PlayerCall::ApplyAudioSettings(RecordedSettings(settings)) = &calls[0] else {
        panic!("{calls:?}");
    };
    assert_eq!(settings.device_name.as_deref(), Some("USB DAC"));
    assert_eq!(settings.device_id.as_deref(), Some("usb-dac"));
    assert_eq!(settings.sample_rate, Some(96_000));
    let stored = services
        .ctx
        .db
        .read(|db| Ok(db.get_setting(keys::AUDIO_DEVICE_ID)))
        .recv_blocking()
        .unwrap();
    assert_eq!(stored.as_deref(), Some("usb-dac"), "the keys are saved");
    assert!(services.ctx.shutdown());
}

#[test]
fn apply_is_busy_while_it_runs_and_disabled_after_it() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    vm.selection_changed();
    let enabled = vm.apply_enabled();
    assert!(enabled.get());

    let reply = vm.begin_apply().expect("apply is available");
    assert!(vm.busy().get());
    assert!(!enabled.get(), "the button is disabled while it runs");
    assert!(vm.begin_apply().is_none(), "a second apply does not start");

    let outcome = reply.recv_blocking();
    let (mut tree, _toasts) = tree_for(&services);
    finish(&vm, &mut tree, outcome);
    assert!(!vm.busy().get());
    assert!(!enabled.get(), "apply is disabled again until a change");
    assert!(services.ctx.shutdown());
}

#[test]
fn a_failed_apply_shows_an_error_toast_and_still_disables_apply() {
    // Quirk fix 8 (Apply): Iced only logs the error.
    let (vm, services, fakes) = fixture();
    show(&vm, &services);
    vm.selection_changed();
    fakes
        .player
        .fail_apply_audio_settings(Some("the device refused 8 channels".to_string()));
    let result = apply_and_wait(&vm);
    assert!(result.is_err());

    let (mut tree, toasts) = tree_for(&services);
    finish(&vm, &mut tree, result);

    let shown = toasts.shown();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].contains("could not be applied"), "{}", shown[0]);
    assert!(
        shown[0].contains("the device refused 8 channels"),
        "{}",
        shown[0]
    );
    assert!(!vm.busy().get());
    assert!(
        !vm.apply_enabled().get(),
        "Iced disables apply after any apply"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_successful_apply_shows_no_toast() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    vm.selection_changed();
    let result = apply_and_wait(&vm);
    let (mut tree, toasts) = tree_for(&services);
    finish(&vm, &mut tree, result);
    assert!(toasts.shown().is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn apply_does_nothing_while_it_is_disabled() {
    let (vm, services, fakes) = fixture();
    show(&vm, &services);
    assert!(vm.begin_apply().is_none());
    assert!(
        !fakes
            .player
            .calls()
            .iter()
            .any(|call| matches!(call, PlayerCall::ApplyAudioSettings(_)))
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn reiniciar_restarts_the_sound_server_services() {
    let (vm, services, fakes) = fixture();
    vm.restart_service();
    let system_audio = fakes.system_audio.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || system_audio
        .calls()
        .contains(&SystemAudioCall::RestartAudioServices)));
    assert!(services.ctx.shutdown());
}

// ---- the poll ----

fn status_calls(fakes: &InMemoryFakes) -> usize {
    fakes
        .system_audio
        .calls()
        .iter()
        .filter(|call| **call == SystemAudioCall::SystemSoundStatus)
        .count()
}

#[test]
fn the_status_starts_as_dashes() {
    let (vm, services, _fakes) = fixture();
    let status = vm.system_status().get();
    assert_eq!(
        (status.status.as_str(), status.latency.as_str()),
        ("--", "--")
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_poll_asks_the_engine_worker_and_shows_the_answer() {
    let (vm, services, fakes) = fixture();
    fakes
        .system_audio
        .set_status("PipeWire 96000 Hz", "2048 / 96000");
    vm.poll(Instant::now());
    let status = vm.system_status();
    assert!(run_until(&services.runtime, TIMEOUT, || status
        .get()
        .status
        != "--"));
    assert_eq!(status.get().status, "PipeWire 96000 Hz");
    assert_eq!(status.get().latency, "2048 / 96000");
    assert_eq!(status_calls(&fakes), 1);
    assert!(services.ctx.shutdown());
}

#[test]
fn polls_are_three_seconds_apart_whatever_the_frame_rate() {
    let (vm, services, fakes) = fixture();
    let start = Instant::now();
    // Frames others request tick at 60 Hz: only the clock counts.
    for frame in 0..300 {
        vm.poll(start + Duration::from_millis(frame * 16));
        run_until(&services.runtime, Duration::from_millis(1), || false);
    }
    // 300 frames of 16 ms are 4.8 s: the polls at 0 s and 3 s.
    assert!(run_until(&services.runtime, TIMEOUT, || status_calls(
        &fakes
    ) >= 2));
    assert_eq!(status_calls(&fakes), 2);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_slow_sound_server_is_never_asked_twice_at_once() {
    let (vm, services, fakes) = fixture();
    let start = Instant::now();
    vm.poll(start);
    // Before the first answer is applied (the executor has not run), the
    // next poll comes due.
    vm.poll(start + POLL_INTERVAL);
    vm.poll(start + POLL_INTERVAL * 2);
    assert!(run_until(&services.runtime, TIMEOUT, || status_calls(
        &fakes
    ) >= 1
        && !vm.inner.status_in_flight.get()));
    assert_eq!(
        status_calls(&fakes),
        1,
        "one request while one is in flight"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn showing_the_tab_again_polls_at_once() {
    let (vm, services, fakes) = fixture();
    let start = Instant::now();
    vm.poll(start);
    assert!(run_until(&services.runtime, TIMEOUT, || status_calls(
        &fakes
    ) == 1
        && !vm.inner.status_in_flight.get()));
    vm.on_shown();
    vm.poll(start + Duration::from_millis(100));
    assert!(run_until(&services.runtime, TIMEOUT, || status_calls(
        &fakes
    ) == 2));
    assert!(services.ctx.shutdown());
}

// ---- the information column ----

fn flac() -> FixtureSong {
    FixtureSong {
        format: "FLAC".into(),
        bit_depth: 24,
        sample_rate: 96_000,
        channels: 6,
        track_gain: Some(-7.5),
        album_gain: Some(-6.25),
        ..FixtureSong::new("/m/a.flac", "Song A", "Artist", "Album")
    }
}

#[test]
fn the_rows_describe_the_track_and_the_output_stream() {
    let (services, fakes) = services_with_fakes();
    LibraryFixture::new()
        .song(flac())
        .seed_on(&services.ctx.db)
        .recv_blocking()
        .unwrap();
    fakes
        .player
        .load_file(
            &TrackRequest {
                path: "/m/a.flac".into(),
                title: "Song A".into(),
                ..TrackRequest::default()
            },
            ReplayGains::default(),
        )
        .unwrap();
    let vm = ConfigViewModel::new(&services);
    show(&vm, &services);
    let rows = vm.info().get();
    assert_eq!(rows.title, "Song A");
    assert_eq!(rows.format_text(), "FLAC");
    assert_eq!(rows.input_rate_text(), "96000 Hz");
    assert_eq!(rows.input_depth_text(), "24 bits");
    assert_eq!(rows.input_channels, Some(6));
    assert_eq!(rows.output_rate_text(), "48000 Hz");
    assert_eq!(rows.output_depth_text(), "32 bits Float");
    assert_eq!(rows.output_channels, Some(2));
    assert_eq!(rows.gain_text(), "-6.25 dB | -7.50 dB");
    assert_eq!(
        rows.quantum_text(),
        "512",
        "the buffer the callback published"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn nothing_loaded_shows_dashes_and_auto() {
    let (vm, services, _fakes) = fixture();
    show(&vm, &services);
    let rows = vm.info().get();
    // The engine's title before any track is loaded (Iced shows "Sin
    // reproducción" only for an empty one).
    assert_eq!(rows.title, "Sin título");
    assert_eq!(rows.format_text(), "--");
    assert_eq!(rows.input_rate_text(), "--");
    assert_eq!(rows.input_depth_text(), "--");
    assert_eq!(rows.gain_text(), "-- | --");
    assert!(services.ctx.shutdown());
}

#[test]
fn before_the_first_load_the_output_side_shows_dashes() {
    let rows = InfoRows::new(&InputInfo::default(), None);
    assert_eq!(rows.output_rate_text(), "--");
    assert_eq!(rows.output_depth_text(), "--");
    assert_eq!(rows.quantum_text(), "Auto");
}

#[test]
fn a_missing_gain_is_a_dash_on_its_side() {
    let rows = |album, track| InfoRows {
        album_gain: album,
        track_gain: track,
        ..InfoRows::new(&InputInfo::default(), None)
    };
    assert_eq!(rows(Some(-6.0), None).gain_text(), "-6.00 dB | --");
    assert_eq!(rows(None, Some(2.5)).gain_text(), "-- | 2.50 dB");
}

#[test]
fn a_new_track_reads_its_tags_again() {
    let (services, fakes) = services_with_fakes();
    LibraryFixture::new()
        .song(flac())
        .seed_on(&services.ctx.db)
        .recv_blocking()
        .unwrap();
    let vm = ConfigViewModel::new(&services);
    show(&vm, &services);
    assert_eq!(vm.info().get().title, "Sin título");
    fakes
        .player
        .load_file(
            &TrackRequest {
                path: "/m/a.flac".into(),
                title: "Song A".into(),
                ..TrackRequest::default()
            },
            ReplayGains::default(),
        )
        .unwrap();
    vm.refresh_input();
    let info = vm.info();
    assert!(run_until(&services.runtime, TIMEOUT, || info.get().title == "Song A"));
    assert_eq!(info.get().input_rate_text(), "96000 Hz");
    assert_eq!(
        info.get().output_rate_text(),
        "48000 Hz",
        "the output side is kept"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn the_tab_is_shown_only_while_the_panel_is_open_on_the_first_tab() {
    let (vm, services, _fakes) = fixture();
    let shown = vm.shown();
    assert!(!shown.get());
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::CONFIG));
    assert!(shown.get());
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::EFFECTS));
    assert!(!shown.get());
    let mut tree = tree_for(&services).0;
    layout(&mut tree);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_apply_handler_starts_the_job_on_the_engine_worker_and_marks_the_tab_busy() {
    // `apply` is `begin_apply` plus `spawn_local_with`, which delivers the
    // outcome to `finish_apply` once the event loop runs the completion (a
    // headless tree has none, so this pins the first half).
    let (vm, services, fakes) = fixture();
    show(&vm, &services);
    vm.selection_changed();
    let (mut tree, _toasts) = tree_for(&services);
    let started = vm.clone();
    press(&mut tree, move |ctx| started.apply(ctx));
    assert!(vm.busy().get());
    let player = fakes.player.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || player
        .calls()
        .iter()
        .any(|call| matches!(call, PlayerCall::ApplyAudioSettings(_)))));
    assert!(services.ctx.shutdown());
}
