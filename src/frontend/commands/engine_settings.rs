//! The engine half of the startup restore: puts every persisted audio setting
//! back into the engine, once, on the facade's workers.
//!
//! [`restore_persisted`] follows the order of the Iced UI's
//! `AudoxidyApp::new` (`src/gui/app.rs`), with four Teksilo-only changes
//! recorded in `docs/teksilo-port-quirk-fixes.md`:
//!
//! 1. the sound-server clock (PipeWire: force, settle, persist; PulseAudio:
//!    persist), **before** the stream is rebuilt, as Apply does (quirk fix 19,
//!    Iced defect D24), with the same "Automatic" rule as Apply (quirk fix 17,
//!    Iced defect D18);
//! 2. the output stream, from the saved device settings when there are any.
//!    The engine falls back to the default device when the saved one is gone
//!    and sets its notice, exactly as in Iced. When only the device name was
//!    saved, the resolved stable id is persisted (Iced's backfill);
//! 3. the volume;
//! 4. the volume-and-mix, crossfade and meter settings (engine state);
//! 5. the equalizer: mode, on/off, preamp, both band sets;
//! 6. the effects, with the parameters the compressor derives from its
//!    intensity and the reverb from its room size (quirk fix 27, Iced defect
//!    D25: Iced only assigns the two fields, so they wait for a stream
//!    rebuild);
//! 7. the downmix settings (quirk fix 9: Iced applies them only on the first
//!    Audio Center open).
//!
//! Iced reads the volume-and-mix keys a second time at startup; that second
//! pass changes nothing and is not repeated (quirk fix 6). The session restore
//! (playlist, track, position) runs after this one and is not part of it.

#![warn(missing_docs)]

use super::audio_settings::{SystemClock, resolve_system_clock};
use crate::frontend::app_context::AppContext;
use crate::frontend::engine_worker::SoundServer;
use crate::frontend::error::FrontendError;
use crate::frontend::events::DeviceSnapshot;
use crate::frontend::reply::Reply;
use crate::frontend::settings::{self, PersistedSettings};

/// What happened to the output stream during the restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamRestore {
    /// No output configuration was saved (fresh install): the stream the
    /// engine opened at start-up is kept, as in Iced.
    NotSaved,
    /// The stream was rebuilt with the saved configuration.
    Applied,
    /// The stream could not be rebuilt; the engine's notice says why. The
    /// rest of the restore still ran, as in Iced.
    Failed(String),
}

/// What [`restore_persisted`] put back.
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreReport {
    /// Every persisted setting, as read for the restore (the view-models can
    /// start from it: selected preset, hidden presets, meter display...).
    pub settings: PersistedSettings,
    /// The sound server that took the clock.
    pub sound_server: SoundServer,
    /// The clock forced on it.
    pub clock: SystemClock,
    /// What happened to the output stream.
    pub stream: StreamRestore,
    /// The stable device id persisted because only the device name was saved.
    pub device_id_backfilled: Option<String>,
    /// The output device and stream format after the restore.
    pub device: DeviceSnapshot,
    /// The engine's notice (for example, the saved device is gone and the
    /// default one is used).
    pub audio_notice: Option<String>,
}

/// Restores every persisted audio setting into the engine; see the module
/// documentation for the order.
///
/// The settings are read on the database worker and applied in one job on
/// the engine worker; nothing runs on the caller's thread. It runs once per
/// context (quirk fix 6): a second call, or a call after the session restore
/// (`session::restore`, which runs this restore itself), resolves to
/// [`FrontendError::InvalidInput`] without touching the engine.
pub fn restore_persisted(ctx: &AppContext) -> Reply<RestoreReport> {
    if !ctx.claim_startup_restore() {
        return Reply::ready(Err(FrontendError::InvalidInput(
            "the persisted settings were already restored on this context".to_string(),
        )));
    }
    restore_engine_settings(ctx)
}

/// The restore itself, without the run-once guard: the session restore calls
/// it after claiming the guard.
pub(crate) fn restore_engine_settings(ctx: &AppContext) -> Reply<RestoreReport> {
    let persisted = settings::load_all(&ctx.db);
    let db = ctx.db.clone();
    ctx.engine.run(move |engine| {
        let settings = persisted.recv_blocking()?;
        let player = &engine.player;

        let device = &settings.device;
        let clock = resolve_system_clock(
            device.system_rate,
            device.system_quantum,
            device.sample_rate,
        );
        let sound_server = engine.apply_system_clock(clock.rate, clock.quantum);

        let mut device_id_backfilled = None;
        let stream = if device.has_saved_output() {
            match player.apply_audio_settings(device.to_audio_settings()) {
                Ok(()) => {
                    if device.device_id.is_none()
                        && let Some(resolved) = player.snapshot().device_id
                    {
                        match settings::write_audio_device_id(&db, &resolved).recv_blocking() {
                            Ok(()) => {
                                tracing::info!("persisted the stable id of the output device");
                                device_id_backfilled = Some(resolved);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "the stable output device id could not be persisted: {e}"
                                );
                            }
                        }
                    }
                    StreamRestore::Applied
                }
                Err(e) => {
                    tracing::warn!("the saved audio settings could not be applied at startup: {e}");
                    StreamRestore::Failed(e.to_string())
                }
            }
        } else {
            StreamRestore::NotSaved
        };

        if let Some(volume) = settings.player.volume {
            player.set_volume(volume);
        }

        {
            let state = player.state();
            let mut state = state.write();
            settings.mix.apply_to(&mut state);
            settings.crossfade.apply_to(&mut state);
            settings.meter.apply_to(&mut state);
        }

        let eq = &settings.eq;
        player.set_eq_mode(if eq.bands_31 { 31 } else { 20 });
        player.set_eq_enabled(eq.enabled);
        player.set_preamp_gain(eq.preamp_gain);
        player.apply_eq_preset_gains(&eq.band_gains_20, &eq.band_gains_31);

        player.update_dsp(&mut |dsp| {
            settings.dsp.apply_to(dsp);
            // Iced assigns the two fields and stops there (defect D25); compute
            // the parameters the effects actually use (quirk fix 27).
            dsp.compressor.update_intensity_params();
            let room_size = dsp.reverb.room_size;
            dsp.reverb.set_room_size(room_size);
        });

        settings.downmix.apply_to(&mut player.state().write());

        let state = player.snapshot();
        Ok(RestoreReport {
            sound_server,
            clock,
            stream,
            device_id_backfilled,
            device: DeviceSnapshot::from_state(&state),
            audio_notice: state.audio_notice,
            settings,
        })
    })
}

/// A context whose ports record, in one ordered log, the calls whose order
/// the settings and session commands promise.
#[cfg(test)]
pub(crate) mod rig {
    use std::sync::Arc;
    use std::time::Instant;

    use parking_lot::{Mutex, RwLock};

    use crate::audio::AudioError;
    use crate::audio::dsp::DspChain;
    use crate::audio::engine::{AudioDeviceInfo, AudioSettings, AudioState};
    use crate::audio::meter::MeterData;
    use crate::audio::preset::EqPreset;
    use crate::db::Database;
    use crate::db::scanner::Scanner;
    use crate::frontend::app_context::{AppContext, Parts};
    use crate::frontend::db_worker::SharedDatabase;
    use crate::frontend::fakes::{FakeMediaControls, FakeMemoryProbe, FakePlayer, FakeSystemAudio};
    use crate::frontend::ports::{PlayerPort, ReplayGains, SystemAudioPort, TrackRequest};
    use crate::frontend::settings::keys;
    use crate::integrations::control::{ExternalControlSender, external_control_channel};
    use crate::utils::config::AppConfig;

    /// One recorded call.
    #[derive(Debug, Clone)]
    pub(crate) enum Step {
        /// A sound-server call.
        System(&'static str),
        /// `apply_audio_settings`, with what the database held for
        /// `audio_sample_rate` when it ran.
        Stream {
            settings: AudioSettings,
            persisted_sample_rate: Option<String>,
        },
        /// A player call that changes a setting.
        Player(&'static str),
    }

    impl Step {
        pub(crate) fn name(&self) -> &'static str {
            match self {
                Step::System(name) | Step::Player(name) => name,
                Step::Stream { .. } => "apply_audio_settings",
            }
        }
    }

    type Log = Arc<Mutex<Vec<(Instant, Step)>>>;

    struct RecordingSystemAudio {
        inner: Arc<FakeSystemAudio>,
        log: Log,
    }

    impl RecordingSystemAudio {
        fn record(&self, name: &'static str) {
            self.log.lock().push((Instant::now(), Step::System(name)));
        }
    }

    impl SystemAudioPort for RecordingSystemAudio {
        fn is_pipewire_active(&self) -> bool {
            self.record("is_pipewire_active");
            self.inner.is_pipewire_active()
        }
        fn apply_pipewire_clock(&self, rate: Option<u32>, quantum: Option<u32>) {
            self.record("apply_pipewire_clock");
            self.inner.apply_pipewire_clock(rate, quantum);
        }
        fn persist_pipewire_conf(&self, rate: Option<u32>, quantum: Option<u32>) {
            self.record("persist_pipewire_conf");
            self.inner.persist_pipewire_conf(rate, quantum);
        }
        fn persist_pulse_conf(&self, rate: Option<u32>, quantum: Option<u32>) {
            self.record("persist_pulse_conf");
            self.inner.persist_pulse_conf(rate, quantum);
        }
        fn restart_audio_services(&self) {
            self.record("restart_audio_services");
            self.inner.restart_audio_services();
        }
        fn system_sound_status(&self) -> (String, String) {
            self.inner.system_sound_status()
        }
    }

    struct RecordingPlayer {
        inner: Arc<FakePlayer>,
        log: Log,
        database: SharedDatabase,
        resolved_device_id: Mutex<Option<String>>,
    }

    impl RecordingPlayer {
        fn record(&self, name: &'static str) {
            self.log.lock().push((Instant::now(), Step::Player(name)));
        }
    }

    impl PlayerPort for RecordingPlayer {
        fn play(&self) {
            self.inner.play();
        }
        fn pause(&self) {
            self.inner.pause();
        }
        fn set_playing(&self, playing: bool) {
            self.record("set_playing");
            self.inner.set_playing(playing);
        }
        fn stop(&self) {
            self.inner.stop();
        }
        fn toggle_play_pause(&self) {
            self.inner.toggle_play_pause();
        }
        fn is_playing(&self) -> bool {
            self.inner.is_playing()
        }
        fn seek(&self, position_secs: f64) {
            self.record("seek");
            self.inner.seek(position_secs);
        }
        fn take_eof(&self) -> bool {
            self.inner.take_eof()
        }
        fn load_file(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
            self.record("load_file");
            self.inner.load_file(track, gains)
        }
        fn preload_next(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
            self.inner.preload_next(track, gains)
        }
        fn purge_preload(&self) -> Result<(), AudioError> {
            self.inner.purge_preload()
        }
        fn crossfade_next(&self, ms: f64) -> Result<(), AudioError> {
            self.inner.crossfade_next(ms)
        }
        fn set_volume(&self, volume: f32) {
            self.record("set_volume");
            self.inner.set_volume(volume);
        }
        fn preamp_gain(&self) -> f32 {
            self.inner.preamp_gain()
        }
        fn set_preamp_gain(&self, db: f32) {
            self.record("set_preamp_gain");
            self.inner.set_preamp_gain(db);
        }
        fn eq_enabled(&self) -> bool {
            self.inner.eq_enabled()
        }
        fn set_eq_enabled(&self, enabled: bool) {
            self.record("set_eq_enabled");
            self.inner.set_eq_enabled(enabled);
        }
        fn eq_band_count(&self) -> usize {
            self.inner.eq_band_count()
        }
        fn set_eq_mode(&self, num_bands: usize) {
            self.record("set_eq_mode");
            self.inner.set_eq_mode(num_bands);
        }
        fn eq_band(&self, index: usize) -> Option<(f32, f32)> {
            self.inner.eq_band(index)
        }
        fn set_eq_band_gain(&self, index: usize, db: f32) {
            self.inner.set_eq_band_gain(index, db);
        }
        fn apply_eq_preset(&self, preset: &EqPreset) {
            self.inner.apply_eq_preset(preset);
        }
        fn apply_eq_preset_gains(&self, bands_20: &[f32], bands_31: &[f32]) {
            self.record("apply_eq_preset_gains");
            self.inner.apply_eq_preset_gains(bands_20, bands_31);
        }
        fn reset_dsp_defaults(&self) {
            self.inner.reset_dsp_defaults();
        }
        fn update_dsp(&self, f: &mut dyn FnMut(&mut DspChain)) {
            self.record("update_dsp");
            self.inner.update_dsp(f);
        }
        fn read_dsp(&self, f: &mut dyn FnMut(&DspChain)) {
            self.inner.read_dsp(f);
        }
        fn state(&self) -> Arc<RwLock<AudioState>> {
            self.inner.state()
        }
        fn snapshot(&self) -> AudioState {
            self.inner.snapshot()
        }
        fn meter(&self) -> Arc<MeterData> {
            self.inner.meter()
        }
        fn buffer_size(&self) -> u32 {
            self.inner.buffer_size()
        }
        fn available_hosts(&self) -> Vec<String> {
            self.inner.available_hosts()
        }
        fn devices(&self) -> Vec<AudioDeviceInfo> {
            self.inner.devices()
        }
        fn apply_audio_settings(&self, settings: AudioSettings) -> Result<(), AudioError> {
            let persisted_sample_rate = match self.database.lock() {
                Ok(db) => db.get_setting(keys::AUDIO_SAMPLE_RATE),
                Err(poisoned) => poisoned.into_inner().get_setting(keys::AUDIO_SAMPLE_RATE),
            };
            self.log.lock().push((
                Instant::now(),
                Step::Stream {
                    settings: settings.clone(),
                    persisted_sample_rate,
                },
            ));
            let result = self.inner.apply_audio_settings(settings);
            // Like the engine, resolve the device to a stable id.
            if result.is_ok()
                && let Some(id) = self.resolved_device_id.lock().clone()
            {
                let state = self.inner.state();
                let mut state = state.write();
                if state.device_id.is_none() {
                    state.device_id = Some(id);
                }
            }
            result
        }
        fn purge_buffers(&self) -> Result<(), AudioError> {
            self.inner.purge_buffers()
        }
    }

    /// A context over the recording ports, with the fakes behind them.
    pub(crate) struct Rig {
        pub(crate) ctx: AppContext,
        pub(crate) player: Arc<FakePlayer>,
        pub(crate) system: Arc<FakeSystemAudio>,
        log: Log,
        recording_player: Arc<RecordingPlayer>,
        _external: ExternalControlSender,
    }

    impl Rig {
        pub(crate) fn new() -> Self {
            let database: SharedDatabase =
                Arc::new(std::sync::Mutex::new(Database::new_memory().unwrap()));
            let log: Log = Arc::new(Mutex::new(Vec::new()));
            let player = Arc::new(FakePlayer::new());
            let system = Arc::new(FakeSystemAudio::new());
            let recording_player = Arc::new(RecordingPlayer {
                inner: Arc::clone(&player),
                log: Arc::clone(&log),
                database: Arc::clone(&database),
                resolved_device_id: Mutex::new(None),
            });
            let (external_tx, external_rx) = external_control_channel();
            let ctx = AppContext::assemble(Parts {
                scanner: Arc::new(Scanner::new(Arc::clone(&database))),
                database,
                player: recording_player.clone(),
                media: Arc::new(FakeMediaControls::new()),
                system_audio: Arc::new(RecordingSystemAudio {
                    inner: Arc::clone(&system),
                    log: Arc::clone(&log),
                }),
                external: external_rx,
                probe: Arc::new(FakeMemoryProbe::new()),
                config: AppConfig::default(),
            })
            .unwrap();
            Self {
                ctx,
                player,
                system,
                log,
                recording_player,
                _external: external_tx,
            }
        }

        /// Makes every successful stream rebuild resolve the device to `id`
        /// when the settings carry no id, as the engine does.
        pub(crate) fn resolve_device_id_to(&self, id: &str) {
            *self.recording_player.resolved_device_id.lock() = Some(id.to_string());
        }

        /// The recorded calls, oldest first.
        pub(crate) fn steps(&self) -> Vec<(Instant, Step)> {
            self.log.lock().clone()
        }

        /// The names of the recorded calls, oldest first.
        pub(crate) fn step_names(&self) -> Vec<&'static str> {
            self.log
                .lock()
                .iter()
                .map(|(_, step)| step.name())
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rig::{Rig, Step};
    use super::*;
    use crate::audio::dsp::ExpanderMode;
    use crate::audio::engine::{BitDepth, ChannelConfig};
    use crate::frontend::engine_worker::PIPEWIRE_SETTLE;
    use crate::frontend::fakes::{PlayerCall, SystemAudioCall};
    use crate::frontend::ports::PlayerPort;
    use crate::frontend::settings::keys;
    use crate::frontend::test_fixtures::LibraryFixture;
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};

    fn seed(ctx: &AppContext, pairs: &[(&str, &str)]) {
        let mut fixture = LibraryFixture::new();
        for (key, value) in pairs {
            fixture = fixture.setting(key, value);
        }
        fixture.seed_on(&ctx.db).recv_blocking().unwrap();
    }

    fn saved_session() -> Vec<(&'static str, String)> {
        let mut pairs: Vec<(&'static str, String)> = vec![
            (keys::PLAYER_VOLUME, "0.4200".into()),
            (keys::AUDIO_HOST, "Fake".into()),
            (keys::AUDIO_DEVICE, "Fake Output".into()),
            (keys::AUDIO_DEVICE_ID, "fake-output".into()),
            (keys::AUDIO_SAMPLE_RATE, "96000".into()),
            (keys::AUDIO_BIT_DEPTH, "24".into()),
            (keys::AUDIO_CHANNELS, "2".into()),
            (keys::AUDIO_BUFFER_SIZE, "auto".into()),
            (keys::AUDIO_SYSTEM_RATE, "auto".into()),
            (keys::AUDIO_SYSTEM_QUANTUM, "auto".into()),
            (keys::EQ_ENABLED, "1".into()),
            (keys::EQ_BANDS_31, "1".into()),
            (keys::PREAMP_GAIN, "-2.5".into()),
            (keys::EQ_BAND_GAINS_20, vec!["1.0"; 20].join(",")),
            (keys::EQ_BAND_GAINS_31, vec!["2.0"; 31].join(",")),
            (keys::DSP_COMPRESSOR_ENABLED, "1".into()),
            (keys::DSP_COMPRESSOR_INTENSITY, "0.8".into()),
            (keys::DSP_STEREO_EXPANDER_MODE, "surround".into()),
            (keys::DSP_REVERB_WET, "0.25".into()),
            (keys::AUDIO_DOWNMIX_CENTER_ENABLED, "1".into()),
            (keys::AUDIO_DOWNMIX_LFE, "1.20".into()),
            (keys::VOL_FADE_IN_MS, "500".into()),
            (keys::VOL_SILENCE_ENABLED, "0".into()),
            (keys::CROSSFADE_ENABLED, "1".into()),
            (keys::CROSSFADE_AUTO_MS, "800".into()),
            (keys::METER_ENABLED, "1".into()),
            (keys::METER_HOLD_TIME_MS, "2000".into()),
        ];
        pairs.sort_by_key(|(key, _)| *key);
        pairs
    }

    fn seed_session(ctx: &AppContext) {
        let pairs = saved_session();
        let pairs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        seed(ctx, &pairs);
    }

    #[test]
    fn the_clock_is_applied_before_the_stream_is_rebuilt() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = Rig::new();
            seed_session(&rig.ctx);
            let report = restore_persisted(&rig.ctx).recv_blocking().unwrap();
            assert_eq!(report.stream, StreamRestore::Applied);
            assert_eq!(
                rig.step_names(),
                [
                    "is_pipewire_active",
                    "apply_pipewire_clock",
                    "persist_pipewire_conf",
                    "apply_audio_settings",
                    "set_volume",
                    "set_eq_mode",
                    "set_eq_enabled",
                    "set_preamp_gain",
                    "apply_eq_preset_gains",
                    "update_dsp",
                ]
            );
            let steps = rig.steps();
            assert!(steps[2].0.duration_since(steps[1].0) >= PIPEWIRE_SETTLE);
            match &steps[3].1 {
                Step::Stream { settings, .. } => {
                    assert_eq!(settings.host_id.as_deref(), Some("Fake"));
                    assert_eq!(settings.device_id.as_deref(), Some("fake-output"));
                    assert_eq!(settings.sample_rate, Some(96_000));
                    assert_eq!(settings.bit_depth, Some(BitDepth::Bits24));
                    assert_eq!(settings.channels, ChannelConfig::Manual(2));
                    assert_eq!(settings.buffer_size, None);
                }
                other => panic!("unexpected step {other:?}"),
            }
            assert_eq!(
                rig.system.calls()[1],
                SystemAudioCall::ApplyPipewireClock(Some(96_000), Some(1024))
            );
        });
    }

    #[test]
    fn every_persisted_setting_reaches_the_engine() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = Rig::new();
            seed_session(&rig.ctx);
            restore_persisted(&rig.ctx).recv_blocking().unwrap();
            let player = &rig.player;
            let state = player.snapshot();
            assert_eq!(state.volume, 0.42);
            assert_eq!(state.device_sample_rate, 96_000);
            assert_eq!(state.fade_in_ms, 500.0);
            assert_eq!(state.fade_out_ms, 3000.0, "the Iced startup default");
            assert!(!state.silence_enabled);
            assert!(state.crossfade_enabled);
            assert_eq!(state.crossfade_auto_ms, 800.0);
            assert!(state.meter_enabled);
            assert_eq!(state.meter_hold_time_ms, 2000.0);
            assert!(state.downmix_center_enabled, "quirk fix 9");
            assert_eq!(state.downmix_lfe, 1.2);
            assert!(player.eq_enabled());
            assert_eq!(player.eq_band_count(), 31);
            assert!((player.preamp_gain() + 2.5).abs() < 1e-4);
            assert_eq!(player.eq_band(0).map(|b| b.1), Some(2.0));
            player.read_dsp(&mut |dsp| {
                assert!(dsp.compressor.enabled);
                assert_eq!(dsp.compressor.intensity, 0.8);
                assert_eq!(dsp.stereo_expander.mode, ExpanderMode::Surround);
                assert_eq!(dsp.reverb.wet, 0.25);
                // The 20-band set is restored too.
                let mut other = dsp.equalizer.clone();
                other.set_mode(20);
                assert_eq!(other.bands[0].gain, 1.0);
            });
        });
    }

    #[test]
    fn the_restore_applies_each_setting_once_and_runs_once_per_context() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = Rig::new();
            seed_session(&rig.ctx);
            let report = restore_persisted(&rig.ctx).recv_blocking().unwrap();
            let names = rig.step_names();
            for name in [
                "apply_pipewire_clock",
                "apply_audio_settings",
                "set_volume",
                "update_dsp",
            ] {
                assert_eq!(names.iter().filter(|n| **n == name).count(), 1, "{name}");
            }
            assert!(matches!(
                restore_persisted(&rig.ctx.clone()).recv_blocking(),
                Err(FrontendError::InvalidInput(_))
            ));
            assert_eq!(rig.step_names(), names, "the second call touched nothing");
            assert_eq!(report.settings.player.volume, Some(0.42));

            let other = Rig::new();
            assert!(
                restore_persisted(&other.ctx).recv_blocking().is_ok(),
                "another context restores on its own"
            );
        });
    }

    #[test]
    fn startup_and_apply_resolve_automatic_with_the_same_rule() {
        with_timeout(TEST_TIMEOUT, || {
            use crate::frontend::commands::audio_settings::{PendingAudioSettings, apply};
            use crate::logic::audio_settings::SystemSelection;
            let cases = [
                (SystemSelection::Automatic, SystemSelection::Automatic, None),
                (
                    SystemSelection::Automatic,
                    SystemSelection::Automatic,
                    Some(88_200),
                ),
                (
                    SystemSelection::Fixed(192_000),
                    SystemSelection::Automatic,
                    Some(44_100),
                ),
                (
                    SystemSelection::Default,
                    SystemSelection::Automatic,
                    Some(96_000),
                ),
            ];
            for (system_rate, system_quantum, sample_rate) in cases {
                let pending = PendingAudioSettings {
                    sample_rate,
                    system_rate,
                    system_quantum,
                    ..PendingAudioSettings::default()
                };
                let (applied, fakes) = AppContext::in_memory_with_fakes().unwrap();
                apply(&applied, pending.clone()).recv_blocking().unwrap();
                let apply_clock = fakes.system_audio.calls()[1].clone();

                let (restored, restored_fakes) = AppContext::in_memory_with_fakes().unwrap();
                let entries = pending.to_device_settings().entries();
                let pairs: Vec<(&str, &str)> =
                    entries.iter().map(|(k, v)| (*k, v.as_str())).collect();
                seed(&restored, &pairs);
                restore_persisted(&restored).recv_blocking().unwrap();
                let restore_clock = restored_fakes.system_audio.calls()[1].clone();

                assert_eq!(apply_clock, restore_clock, "{pending:?}");
                assert!(matches!(
                    apply_clock,
                    SystemAudioCall::ApplyPipewireClock(..)
                ));
            }
        });
    }

    #[test]
    fn a_fresh_install_resets_the_clock_and_keeps_the_stream() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            let report = restore_persisted(&ctx).recv_blocking().unwrap();
            assert_eq!(report.stream, StreamRestore::NotSaved);
            assert_eq!(
                fakes.system_audio.calls(),
                vec![
                    SystemAudioCall::IsPipewireActive,
                    SystemAudioCall::ApplyPipewireClock(None, None),
                    SystemAudioCall::PersistPipewireConf(None, None),
                ]
            );
            let calls = fakes.player.calls();
            assert!(
                !calls
                    .iter()
                    .any(|c| matches!(c, PlayerCall::ApplyAudioSettings(_)))
            );
            assert!(!calls.iter().any(|c| matches!(c, PlayerCall::SetVolume(_))));
            let state = fakes.player.snapshot();
            assert_eq!(
                state.fade_in_ms, 2000.0,
                "the Iced startup defaults are applied"
            );
            assert!(!state.meter_enabled);
            assert_eq!(fakes.player.eq_band_count(), 20);
        });
    }

    /// The reverb's output for an impulse followed by silence.
    fn impulse_response(reverb: &crate::audio::dsp::Reverb) -> Vec<f64> {
        let mut reverb = reverb.clone();
        let mut out = Vec::new();
        for i in 0..4_000 {
            let mut frame = [if i == 0 { 1.0 } else { 0.0 }; 2];
            reverb.process(&mut frame);
            out.extend(frame);
        }
        out
    }

    #[test]
    fn d25_the_restored_compressor_intensity_and_reverb_room_size_take_effect_at_once() {
        use crate::audio::dsp::{Compressor, Reverb};
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            seed(
                &ctx,
                &[
                    (keys::DSP_COMPRESSOR_INTENSITY, "0.9"),
                    (keys::DSP_REVERB_ENABLED, "1"),
                    (keys::DSP_REVERB_WET, "1.00"),
                    (keys::DSP_REVERB_ROOM_SIZE, "0.95"),
                ],
            );
            restore_persisted(&ctx).recv_blocking().unwrap();

            let mut expected_compressor = Compressor::new();
            expected_compressor.intensity = 0.9;
            expected_compressor.update_intensity_params();
            let mut expected_reverb = Reverb::new();
            expected_reverb.enabled = true;
            expected_reverb.set_wet(1.0);
            expected_reverb.set_room_size(0.95);
            // What Iced leaves until the next stream rebuild: the field only.
            let mut stale_reverb = Reverb::new();
            stale_reverb.enabled = true;
            stale_reverb.set_wet(1.0);
            stale_reverb.room_size = 0.95;
            assert_ne!(
                impulse_response(&expected_reverb),
                impulse_response(&stale_reverb),
                "the room size changes the response"
            );

            fakes.player.read_dsp(&mut |dsp| {
                assert_eq!(dsp.compressor.intensity, 0.9);
                assert_eq!(
                    dsp.compressor.intensity_ratio,
                    expected_compressor.intensity_ratio
                );
                assert_ne!(
                    dsp.compressor.intensity_ratio,
                    Compressor::new().intensity_ratio,
                    "the ratio follows the restored intensity"
                );
                assert_eq!(
                    impulse_response(&dsp.reverb),
                    impulse_response(&expected_reverb)
                );
            });
        });
    }

    #[test]
    fn a_name_only_device_gets_its_resolved_id_persisted() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = Rig::new();
            rig.resolve_device_id_to("alsa:resolved");
            seed(&rig.ctx, &[(keys::AUDIO_DEVICE, "Old Name")]);
            let report = restore_persisted(&rig.ctx).recv_blocking().unwrap();
            assert_eq!(
                report.device_id_backfilled.as_deref(),
                Some("alsa:resolved")
            );
            let stored = rig
                .ctx
                .db
                .read(|db| Ok(db.get_setting(keys::AUDIO_DEVICE_ID)))
                .recv_blocking()
                .unwrap();
            assert_eq!(stored.as_deref(), Some("alsa:resolved"));
        });
    }

    #[test]
    fn a_stream_failure_is_reported_and_the_rest_is_still_restored() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            seed_session(&ctx);
            fakes
                .player
                .fail_apply_audio_settings(Some("the saved device is gone".into()));
            let report = restore_persisted(&ctx).recv_blocking().unwrap();
            assert!(
                matches!(&report.stream, StreamRestore::Failed(message) if message.contains("the saved device is gone")),
                "{:?}",
                report.stream
            );
            assert_eq!(
                report.audio_notice.as_deref(),
                Some("the saved device is gone")
            );
            assert!(fakes.player.eq_enabled(), "the EQ is restored anyway");
            assert_eq!(report.device_id_backfilled, None);
        });
    }
}
