//! The Audio Center's configuration tab: the pending device selections, Apply,
//! "reset to defaults", the tab's read-only state, and the sound-server calls.
//!
//! Apply reproduces the Iced `ApplySettings` handler (`src/gui/audio_center.rs`,
//! `AudioCenterManager::update`) in the same order, as one job on the engine
//! worker:
//!
//! 1. resolve the sound-server rate and quantum ([`resolve_system_clock`]);
//! 2. with PipeWire, force the clock, wait
//!    [`PIPEWIRE_SETTLE`](crate::frontend::engine_worker::PIPEWIRE_SETTLE) and persist the
//!    PipeWire configuration; with PulseAudio, persist its configuration
//!    ([`EngineContext::apply_system_clock`](crate::frontend::engine_worker::EngineContext::apply_system_clock));
//! 3. rebuild the stream with the selected device settings;
//! 4. persist the device keys exactly as Iced does, whether or not the stream
//!    could be rebuilt.
//!
//! Two Teksilo-only changes (see `docs/teksilo-port-quirk-fixes.md`):
//!
//! - a stream error comes back as `Err`, so the UI can show it (quirk fix 8);
//!   Iced only logs it;
//! - "Automatic" sound-server rate and quantum follow one rule, shared with
//!   the startup restore (quirk fix 17, Iced defect D18).

#![warn(missing_docs)]

use crate::audio::engine::{AudioDeviceInfo, AudioSettings, BitDepth, ChannelConfig};
use crate::frontend::app_context::AppContext;
use crate::frontend::engine_worker::SoundServer;
use crate::frontend::error::FrontendError;
use crate::frontend::events::DeviceSnapshot;
use crate::frontend::reply::Reply;
use crate::frontend::settings::{self, DeviceSettings, PersistedSettings};
use crate::logic::audio_settings::{self as rules, SystemSelection};

/// The rate and quantum forced on the sound server; `None` leaves the
/// server's own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemClock {
    /// The graph rate, in Hz.
    pub rate: Option<u32>,
    /// The graph quantum, in frames.
    pub quantum: Option<u32>,
}

/// Resolves the sound-server clock from the selections and the player's
/// configured sample rate (`None` = automatic). Apply and the startup restore
/// both use this rule (quirk fix 17):
///
/// - **Rate.** Automatic follows the rate the player stream opens at: the
///   configured rate, or [`rules::DEFAULT_SAMPLE_RATE`] (48 kHz) when the
///   player is automatic, since that is the rate the engine picks for an
///   automatic stream.
/// - **Quantum.** The quantum is counted in graph frames, so Automatic
///   estimates it ([`rules::estimate_quantum`], 10 ms) from the graph rate:
///   the rate forced above, or 48 kHz (the sound server's default) when the
///   rate is left to the server.
///
/// This is the Iced startup rule (`src/gui/app.rs`, `AudoxidyApp::new`). The
/// Iced Apply instead leaves the rate to the server when the player is
/// automatic, and estimates the quantum from the player rate even when the
/// graph runs at another rate.
pub fn resolve_system_clock(
    system_rate: SystemSelection<u32>,
    system_quantum: SystemSelection<u32>,
    player_rate: Option<u32>,
) -> SystemClock {
    let player_rate = player_rate.unwrap_or(rules::DEFAULT_SAMPLE_RATE);
    let rate = rules::resolve_system_rate(system_rate, Some(player_rate));
    let quantum =
        rules::resolve_system_quantum(system_quantum, rate.unwrap_or(rules::DEFAULT_SAMPLE_RATE));
    SystemClock { rate, quantum }
}

/// The selections of the configuration tab, applied by [`apply`].
#[derive(Debug, Clone, PartialEq)]
pub struct PendingAudioSettings {
    /// The audio host (`None` = system default).
    pub host: Option<String>,
    /// The output device name.
    pub device: Option<String>,
    /// The stable id of the output device. Set it together with the name
    /// through [`PendingAudioSettings::select_device`].
    pub device_id: Option<String>,
    /// The player sample rate (`None` = automatic).
    pub sample_rate: Option<u32>,
    /// The output bit depth.
    pub bit_depth: BitDepth,
    /// The output channel count.
    pub channels: u16,
    /// The output buffer size (`None` = automatic).
    pub buffer_size: Option<u32>,
    /// The sound-server rate selection.
    pub system_rate: SystemSelection<u32>,
    /// The sound-server quantum selection.
    pub system_quantum: SystemSelection<u32>,
}

impl Default for PendingAudioSettings {
    /// The Iced Audio Center's initial selections.
    fn default() -> Self {
        Self {
            host: None,
            device: None,
            device_id: None,
            sample_rate: None,
            bit_depth: rules::DEFAULT_BIT_DEPTH,
            channels: rules::DEFAULT_CHANNELS,
            buffer_size: None,
            system_rate: SystemSelection::Default,
            system_quantum: SystemSelection::Default,
        }
    }
}

impl PendingAudioSettings {
    /// Selects `device`: its name, and its stable id (which Apply persists).
    pub fn select_device(&mut self, device: &AudioDeviceInfo) {
        self.device = Some(device.name.clone());
        self.device_id = device.id.clone();
    }

    /// The stream configuration Apply builds.
    pub fn to_audio_settings(&self) -> AudioSettings {
        AudioSettings {
            host_id: self.host.clone(),
            device_name: self.device.clone(),
            device_id: self.device_id.clone(),
            sample_rate: self.sample_rate,
            bit_depth: Some(self.bit_depth),
            channels: ChannelConfig::Manual(self.channels),
            buffer_size: self.buffer_size,
        }
    }

    /// The sound-server clock Apply forces.
    pub fn system_clock(&self) -> SystemClock {
        resolve_system_clock(self.system_rate, self.system_quantum, self.sample_rate)
    }

    /// The device keys as Apply persists them.
    pub fn to_device_settings(&self) -> DeviceSettings {
        DeviceSettings {
            host: self.host.clone(),
            device: self.device.clone(),
            device_id: self.device_id.clone(),
            sample_rate: self.sample_rate,
            bit_depth: Some(self.bit_depth),
            channels: self.channels,
            buffer_size: self.buffer_size,
            system_rate: self.system_rate,
            system_quantum: self.system_quantum,
        }
    }
}

/// The stable id of the first device called `name` (how Iced resolves the
/// selected device at Apply time).
pub fn device_id_for(devices: &[AudioDeviceInfo], name: Option<&str>) -> Option<String> {
    let name = name?;
    devices
        .iter()
        .find(|d| d.name == name)
        .and_then(|d| d.id.clone())
}

/// The selections the tab opens with, as Iced's `sync_from_engine` computes
/// them on the first open:
///
/// - host: the saved one if it is listed, else "ALSA" if listed, else the
///   first host (kept as saved when no host is listed);
/// - device: the saved name, else the preferred device of
///   [`rules::default_device`]; its id is looked up by name;
/// - bit depth, buffer size and sound-server selections: the saved ones;
/// - sample rate and channels: the stream's current values.
pub fn initial_pending(
    saved: &DeviceSettings,
    hosts: &[String],
    devices: &[AudioDeviceInfo],
    output_sample_rate: u32,
    output_channels: u16,
) -> PendingAudioSettings {
    let mut host = saved.host.clone();
    if !hosts.is_empty() && !host.as_ref().is_some_and(|h| hosts.contains(h)) {
        host = rules::default_host(hosts).map(str::to_string);
    }
    let device = saved
        .device
        .clone()
        .or_else(|| rules::default_device(devices).map(|d| d.name.clone()));
    PendingAudioSettings {
        host,
        device_id: device_id_for(devices, device.as_deref()),
        device,
        sample_rate: Some(output_sample_rate),
        bit_depth: saved.bit_depth.unwrap_or(rules::DEFAULT_BIT_DEPTH),
        channels: output_channels,
        buffer_size: saved.buffer_size,
        system_rate: saved.system_rate,
        system_quantum: saved.system_quantum,
    }
}

/// The selections after "Predeterminado" (Iced `ResetToDefaults`): 48 kHz,
/// 32-bit float, automatic buffer, stereo, server defaults, the preferred host
/// and device. The host and the device are kept when the lists are empty.
pub fn reset_to_defaults(
    current: &PendingAudioSettings,
    hosts: &[String],
    devices: &[AudioDeviceInfo],
) -> PendingAudioSettings {
    let mut reset = PendingAudioSettings {
        sample_rate: Some(rules::DEFAULT_SAMPLE_RATE),
        bit_depth: rules::DEFAULT_BIT_DEPTH,
        buffer_size: rules::DEFAULT_BUFFER_SIZE,
        channels: rules::DEFAULT_CHANNELS,
        system_rate: SystemSelection::Default,
        system_quantum: SystemSelection::Default,
        ..current.clone()
    };
    if let Some(host) = rules::default_host(hosts) {
        reset.host = Some(host.to_string());
    }
    if let Some(device) = rules::default_device(devices) {
        reset.select_device(device);
    }
    reset
}

/// What [`apply`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyOutcome {
    /// The sound server that took the clock.
    pub sound_server: SoundServer,
    /// The clock forced on it.
    pub clock: SystemClock,
    /// The output device and stream format after the rebuild.
    pub device: DeviceSnapshot,
    /// The engine's notice (for example, the device fell back to the default).
    pub audio_notice: Option<String>,
}

/// Applies the configuration tab's selections; see the module documentation
/// for the sequence. Runs as one job on the engine worker.
///
/// The device keys are persisted even when the stream cannot be rebuilt, as in
/// Iced. The reply is then the stream error ([`FrontendError::Audio`]), so the
/// UI can show it; a persistence failure is the error otherwise.
pub fn apply(ctx: &AppContext, pending: PendingAudioSettings) -> Reply<ApplyOutcome> {
    let db = ctx.db.clone();
    ctx.engine.run(move |engine| {
        let clock = pending.system_clock();
        let sound_server = engine.apply_system_clock(clock.rate, clock.quantum);
        let applied = engine
            .player
            .apply_audio_settings(pending.to_audio_settings());
        if let Err(e) = &applied {
            tracing::error!(
                "the audio settings could not be applied ({e}): the device may not support \
                 this sample rate and channel combination; playback stays stopped until a \
                 valid configuration is applied"
            );
        }
        let persisted =
            settings::write_entries(&db, pending.to_device_settings().entries()).recv_blocking();
        applied?;
        persisted?;
        let state = engine.player.snapshot();
        Ok(ApplyOutcome {
            sound_server,
            clock,
            device: DeviceSnapshot::from_state(&state),
            audio_notice: state.audio_notice,
        })
    })
}

/// The output side of the information column, from the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputInfo {
    /// The stream's sample rate, in Hz.
    pub sample_rate: u32,
    /// The stream's sample format, normalised for display
    /// ([`rules::bit_depth_display`]).
    pub bit_depth: String,
    /// The stream's channel count.
    pub channels: u16,
    /// The buffer size the output callback last published, in frames; 0 is
    /// shown as "Auto".
    pub buffer_size: u32,
    /// The engine's notice, if any.
    pub notice: Option<String>,
}

/// The input side of the information column: the current track, with its
/// format and technical tags from the database. A value of 0 in the database
/// is `None` (shown as "--").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputInfo {
    /// The current track's title, as the engine reports it.
    pub title: String,
    /// The current track's path (empty when nothing is loaded).
    pub path: String,
    /// The container format ("FLAC", "MP3"...).
    pub format: Option<String>,
    /// The file's sample rate, in Hz.
    pub sample_rate: Option<u32>,
    /// The file's bits per sample.
    pub bit_depth: Option<u32>,
    /// The file's channel count.
    pub channels: Option<u16>,
    /// Track ReplayGain, in dB.
    pub track_gain: Option<f64>,
    /// Album ReplayGain, in dB.
    pub album_gain: Option<f64>,
}

/// Everything the configuration tab shows.
#[derive(Debug, Clone)]
pub struct AudioCenterState {
    /// The selections the tab opens with ([`initial_pending`]).
    pub pending: PendingAudioSettings,
    /// The audio hosts.
    pub hosts: Vec<String>,
    /// The output devices.
    pub devices: Vec<AudioDeviceInfo>,
    /// The output stream.
    pub output: OutputInfo,
    /// The current track.
    pub input: InputInfo,
    /// Every persisted setting, as read for this state.
    pub persisted: PersistedSettings,
}

/// Reads the input side for the track at `path` (on the database worker).
fn read_input(
    db: &crate::db::Database,
    title: String,
    path: String,
) -> Result<InputInfo, FrontendError> {
    let mut info = InputInfo {
        title,
        path,
        ..InputInfo::default()
    };
    if info.path.is_empty() {
        return Ok(info);
    }
    if let Some((rate, depth, channels)) = db.get_song_technical_meta_by_path(&info.path)? {
        info.sample_rate = (rate > 0).then_some(rate);
        info.bit_depth = (depth > 0).then_some(depth);
        info.channels = (channels > 0).then(|| u16::try_from(channels).unwrap_or(u16::MAX));
    }
    info.format = db.get_song_format_by_path(&info.path)?;
    let (track, album) = db.get_replay_gain_by_path(&info.path)?;
    info.track_gain = track;
    info.album_gain = album;
    Ok(info)
}

/// The input side for the track the engine has loaded, read on the database
/// worker. For refreshing the column when the track changes.
pub fn input_info(ctx: &AppContext) -> Reply<InputInfo> {
    let player = std::sync::Arc::clone(&ctx.player);
    ctx.db.read(move |db| {
        let state = player.snapshot();
        read_input(db, state.title, state.path)
    })
}

/// Everything the configuration tab shows: the persisted selections, the live
/// hosts and devices (enumerated on the engine worker), the output stream and
/// the current track.
///
/// It only reads. Iced's first open also pushed the saved EQ, effects, downmix
/// and mix settings into the engine (`sync_from_engine`); here the startup
/// restore does that ([`restore_persisted`](super::engine_settings::restore_persisted),
/// quirk fix 9).
pub fn audio_center_state(ctx: &AppContext) -> Reply<AudioCenterState> {
    let persisted = settings::load_all(&ctx.db);
    let input = input_info(ctx);
    ctx.engine.run(move |engine| {
        let hosts = engine.player.available_hosts();
        let devices = engine.player.devices();
        let state = engine.player.snapshot();
        let output = OutputInfo {
            sample_rate: state.device_sample_rate,
            bit_depth: rules::bit_depth_display(&state.bit_depth_display),
            channels: state.channels,
            buffer_size: engine.player.buffer_size(),
            notice: state.audio_notice.clone(),
        };
        let persisted = persisted.recv_blocking()?;
        let input = input.recv_blocking()?;
        let pending = initial_pending(
            &persisted.device,
            &hosts,
            &devices,
            state.device_sample_rate,
            state.channels,
        );
        Ok(AudioCenterState {
            pending,
            hosts,
            devices,
            output,
            input,
            persisted,
        })
    })
}

/// The sound server's status line and latency, as the tab shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    /// The status ("PipeWire 48000 Hz"...).
    pub status: String,
    /// The latency description.
    pub latency: String,
}

/// Queries the sound server on the engine worker (the tab polls it every 3 s
/// while it is visible).
pub fn system_status(ctx: &AppContext) -> Reply<SystemStatus> {
    ctx.engine.run(|engine| {
        let (status, latency) = engine.system_audio.system_sound_status();
        Ok(SystemStatus { status, latency })
    })
}

/// Restarts PipeWire, its PulseAudio bridge and WirePlumber on the engine
/// worker ("Reiniciar").
pub fn restart_audio_services(ctx: &AppContext) -> Reply<()> {
    ctx.engine.restart_audio_services()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::commands::engine_settings::rig::{Rig, Step};
    use crate::frontend::engine_worker::PIPEWIRE_SETTLE;
    use crate::frontend::fakes::{PlayerCall, RecordedSettings, SystemAudioCall};
    use crate::frontend::ports::{PlayerPort, ReplayGains, TrackRequest};
    use crate::frontend::settings::keys;
    use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};

    fn device(name: &str, id: &str) -> AudioDeviceInfo {
        AudioDeviceInfo {
            name: name.to_string(),
            id: Some(id.to_string()),
        }
    }

    fn pending_96k() -> PendingAudioSettings {
        PendingAudioSettings {
            host: Some("ALSA".into()),
            device: Some("Fake Output".into()),
            device_id: Some("fake-output".into()),
            sample_rate: Some(96_000),
            bit_depth: BitDepth::Bits24,
            channels: 2,
            buffer_size: Some(2048),
            system_rate: SystemSelection::Automatic,
            system_quantum: SystemSelection::Automatic,
        }
    }

    fn raw(ctx: &AppContext, key: &'static str) -> Option<String> {
        ctx.db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    #[test]
    fn apply_forces_the_clock_waits_persists_the_conf_then_rebuilds_the_stream_then_persists() {
        with_timeout(TEST_TIMEOUT, || {
            let rig = Rig::new();
            let outcome = apply(&rig.ctx, pending_96k()).recv_blocking().unwrap();
            assert_eq!(outcome.sound_server, SoundServer::PipeWire);
            assert_eq!(
                outcome.clock,
                SystemClock {
                    rate: Some(96_000),
                    quantum: Some(1024)
                }
            );

            let steps = rig.steps();
            let names: Vec<&str> = steps.iter().map(|(_, step)| step.name()).collect();
            assert_eq!(
                names,
                [
                    "is_pipewire_active",
                    "apply_pipewire_clock",
                    "persist_pipewire_conf",
                    "apply_audio_settings"
                ]
            );
            let clock_at = steps[1].0;
            let conf_at = steps[2].0;
            assert!(
                conf_at.duration_since(clock_at) >= PIPEWIRE_SETTLE,
                "the conf is persisted after the settle wait"
            );
            match &steps[3].1 {
                Step::Stream {
                    persisted_sample_rate,
                    ..
                } => assert_eq!(
                    *persisted_sample_rate, None,
                    "the device keys are persisted after the stream is rebuilt"
                ),
                other => panic!("unexpected step {other:?}"),
            }
            assert_eq!(
                raw(&rig.ctx, keys::AUDIO_SAMPLE_RATE).as_deref(),
                Some("96000")
            );
        });
    }

    #[test]
    fn apply_persists_the_device_keys_in_the_legacy_ui_format() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = AppContext::in_memory_with_fakes().unwrap();
            let pending = PendingAudioSettings {
                host: None,
                system_rate: SystemSelection::Fixed(44_100),
                system_quantum: SystemSelection::Default,
                sample_rate: None,
                buffer_size: None,
                ..pending_96k()
            };
            apply(&ctx, pending).recv_blocking().unwrap();
            let expected = [
                (keys::AUDIO_HOST, ""),
                (keys::AUDIO_DEVICE, "Fake Output"),
                (keys::AUDIO_DEVICE_ID, "fake-output"),
                (keys::AUDIO_SAMPLE_RATE, "auto"),
                (keys::AUDIO_BIT_DEPTH, "24"),
                (keys::AUDIO_CHANNELS, "2"),
                (keys::AUDIO_BUFFER_SIZE, "auto"),
                (keys::AUDIO_SYSTEM_RATE, "fixed:44100"),
                (keys::AUDIO_SYSTEM_QUANTUM, "default"),
            ];
            for (key, value) in expected {
                assert_eq!(raw(&ctx, key).as_deref(), Some(value), "{key}");
            }
        });
    }

    #[test]
    fn apply_rebuilds_the_stream_with_the_pending_selections() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            let outcome = apply(&ctx, pending_96k()).recv_blocking().unwrap();
            assert_eq!(
                fakes.player.calls(),
                vec![PlayerCall::ApplyAudioSettings(RecordedSettings(
                    pending_96k().to_audio_settings()
                ))]
            );
            assert_eq!(outcome.device.sample_rate, 96_000);
            assert_eq!(outcome.device.device_id.as_deref(), Some("fake-output"));
        });
    }

    #[test]
    fn a_stream_error_comes_back_as_err_and_the_keys_are_still_persisted() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            fakes
                .player
                .fail_apply_audio_settings(Some("device busy".into()));
            let result = apply(&ctx, pending_96k()).recv_blocking();
            assert!(
                matches!(result, Err(FrontendError::Audio(_))),
                "the UI gets an error to show"
            );
            assert_eq!(raw(&ctx, keys::AUDIO_BUFFER_SIZE).as_deref(), Some("2048"));
        });
    }

    #[test]
    fn with_pulseaudio_apply_only_persists_the_clock() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            fakes.system_audio.set_pipewire_active(false);
            let outcome = apply(&ctx, pending_96k()).recv_blocking().unwrap();
            assert_eq!(outcome.sound_server, SoundServer::PulseAudio);
            assert_eq!(
                fakes.system_audio.calls(),
                vec![
                    SystemAudioCall::IsPipewireActive,
                    SystemAudioCall::PersistPulseConf(Some(96_000), Some(1024)),
                ]
            );
        });
    }

    #[test]
    fn automatic_follows_the_player_rate_and_estimates_the_quantum_from_the_graph_rate() {
        let auto = SystemSelection::Automatic;
        assert_eq!(
            resolve_system_clock(auto, auto, Some(96_000)),
            SystemClock {
                rate: Some(96_000),
                quantum: Some(1024)
            }
        );
        assert_eq!(
            resolve_system_clock(auto, auto, None),
            SystemClock {
                rate: Some(48_000),
                quantum: Some(512)
            },
            "an automatic player opens at 48 kHz"
        );
        assert_eq!(
            resolve_system_clock(SystemSelection::Fixed(192_000), auto, Some(44_100)),
            SystemClock {
                rate: Some(192_000),
                quantum: Some(2048)
            },
            "the quantum counts frames of the forced graph rate"
        );
        assert_eq!(
            resolve_system_clock(SystemSelection::Default, auto, Some(96_000)),
            SystemClock {
                rate: None,
                quantum: Some(512)
            },
            "with the server's own rate, the quantum assumes its 48 kHz default"
        );
        assert_eq!(
            resolve_system_clock(SystemSelection::Default, SystemSelection::Fixed(256), None),
            SystemClock {
                rate: None,
                quantum: Some(256)
            }
        );
    }

    #[test]
    fn no_selectable_rate_lands_on_a_quantum_boundary() {
        // `estimate_quantum` moves a value equal to a boundary up to the next
        // one (D18's rounding remark); no rate the UI offers produces one.
        for rate in rules::SAMPLE_RATE_OPTIONS
            .iter()
            .chain(rules::SYSTEM_RATE_OPTIONS.iter())
        {
            let frames = (*rate as f64 * 10.0 / 1000.0) as u32;
            assert!(
                !rules::BUFFER_SIZE_OPTIONS.contains(&frames),
                "{rate} Hz gives exactly {frames} frames"
            );
        }
    }

    #[test]
    fn reset_to_defaults_picks_the_preferred_host_and_device() {
        let current = pending_96k();
        let hosts = vec!["JACK".to_string(), "ALSA".to_string()];
        let devices = vec![
            device("hw:0", "alsa:hw0"),
            device("PipeWire Sound Server", "alsa:pipewire"),
        ];
        let reset = reset_to_defaults(&current, &hosts, &devices);
        assert_eq!(
            reset,
            PendingAudioSettings {
                host: Some("ALSA".into()),
                device: Some("PipeWire Sound Server".into()),
                device_id: Some("alsa:pipewire".into()),
                sample_rate: Some(48_000),
                bit_depth: BitDepth::Bits32Float,
                channels: 2,
                buffer_size: None,
                system_rate: SystemSelection::Default,
                system_quantum: SystemSelection::Default,
            }
        );
    }

    #[test]
    fn reset_to_defaults_keeps_the_host_and_device_when_none_are_listed() {
        let reset = reset_to_defaults(&pending_96k(), &[], &[]);
        assert_eq!(reset.host.as_deref(), Some("ALSA"));
        assert_eq!(reset.device.as_deref(), Some("Fake Output"));
        assert_eq!(reset.device_id.as_deref(), Some("fake-output"));
        assert_eq!(reset.sample_rate, Some(48_000));
    }

    #[test]
    fn the_initial_selections_follow_the_legacy_first_open() {
        let hosts = vec!["JACK".to_string(), "ALSA".to_string()];
        let devices = vec![device("pulse", "p"), device("hw:1", "h")];
        let saved = DeviceSettings {
            host: Some("Gone".into()),
            buffer_size: Some(512),
            bit_depth: Some(BitDepth::Bits16),
            system_quantum: SystemSelection::Fixed(1024),
            ..DeviceSettings::default()
        };
        let pending = initial_pending(&saved, &hosts, &devices, 44_100, 6);
        assert_eq!(
            pending.host.as_deref(),
            Some("ALSA"),
            "an unlisted host is replaced"
        );
        assert_eq!(pending.device.as_deref(), Some("pulse"));
        assert_eq!(pending.device_id.as_deref(), Some("p"));
        assert_eq!(pending.sample_rate, Some(44_100), "the stream's rate");
        assert_eq!(pending.channels, 6, "the stream's channels");
        assert_eq!(pending.bit_depth, BitDepth::Bits16);
        assert_eq!(pending.buffer_size, Some(512));
        assert_eq!(pending.system_quantum, SystemSelection::Fixed(1024));

        let saved = DeviceSettings {
            host: Some("JACK".into()),
            device: Some("hw:1".into()),
            ..DeviceSettings::default()
        };
        let pending = initial_pending(&saved, &hosts, &devices, 48_000, 2);
        assert_eq!(pending.host.as_deref(), Some("JACK"));
        assert_eq!(pending.device.as_deref(), Some("hw:1"));
        assert_eq!(pending.device_id.as_deref(), Some("h"));
        assert_eq!(pending.bit_depth, BitDepth::Bits32Float);
    }

    #[test]
    fn the_audio_center_state_reads_everything_and_changes_nothing() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            let song = FixtureSong {
                format: "FLAC".into(),
                bit_depth: 24,
                sample_rate: 96_000,
                channels: 2,
                track_gain: Some(-7.5),
                album_gain: Some(-6.25),
                ..FixtureSong::new("/m/a.flac", "Song A", "Artist", "Album")
            };
            LibraryFixture::new()
                .song(song)
                .setting(keys::AUDIO_HOST, "Fake")
                .setting(keys::AUDIO_BUFFER_SIZE, "1024")
                .setting(keys::AUDIO_SYSTEM_RATE, "auto")
                .setting(keys::EQ_ENABLED, "1")
                .setting(keys::AUDIO_DOWNMIX_CENTER_ENABLED, "1")
                .seed_on(&ctx.db)
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
            fakes.player.take_calls();
            let before = ctx
                .db
                .read(|db| Ok(settings::PersistedSettings::read(db)))
                .recv_blocking()
                .unwrap();

            let state = audio_center_state(&ctx).recv_blocking().unwrap();

            assert_eq!(state.hosts, vec!["Fake".to_string()]);
            assert_eq!(state.devices.len(), 1);
            assert_eq!(state.pending.host.as_deref(), Some("Fake"));
            assert_eq!(state.pending.device.as_deref(), Some("Fake Output"));
            assert_eq!(state.pending.device_id.as_deref(), Some("fake-output"));
            assert_eq!(state.pending.buffer_size, Some(1024));
            assert_eq!(state.pending.system_rate, SystemSelection::Automatic);
            assert_eq!(state.output.sample_rate, 48_000);
            assert_eq!(state.output.bit_depth, "32 bits Float");
            assert_eq!(state.output.buffer_size, 512);
            assert_eq!(state.input.title, "Song A");
            assert_eq!(state.input.format.as_deref(), Some("FLAC"));
            assert_eq!(state.input.sample_rate, Some(96_000));
            assert_eq!(state.input.bit_depth, Some(24));
            assert_eq!(state.input.channels, Some(2));
            assert_eq!(state.input.track_gain, Some(-7.5));
            assert_eq!(state.input.album_gain, Some(-6.25));
            assert!(state.persisted.eq.enabled);

            assert_eq!(
                fakes.player.calls(),
                vec![PlayerCall::Devices],
                "no setting was pushed into the engine"
            );
            assert!(!fakes.player.eq_enabled());
            assert!(!fakes.player.snapshot().downmix_center_enabled);
            let after = ctx
                .db
                .read(|db| Ok(settings::PersistedSettings::read(db)))
                .recv_blocking()
                .unwrap();
            assert_eq!(before, after);
        });
    }

    #[test]
    fn the_input_side_is_empty_when_nothing_is_loaded() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = AppContext::in_memory_with_fakes().unwrap();
            let input = input_info(&ctx).recv_blocking().unwrap();
            assert_eq!(input.path, "");
            assert_eq!(input.format, None);
            assert_eq!(input.sample_rate, None);
        });
    }

    #[test]
    fn the_status_and_the_restart_go_through_the_sound_server() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
            fakes.system_audio.set_status("PulseAudio 44100 Hz", "512");
            let status = system_status(&ctx).recv_blocking().unwrap();
            assert_eq!(
                status,
                SystemStatus {
                    status: "PulseAudio 44100 Hz".into(),
                    latency: "512".into()
                }
            );
            restart_audio_services(&ctx).recv_blocking().unwrap();
            assert!(
                fakes
                    .system_audio
                    .calls()
                    .contains(&SystemAudioCall::RestartAudioServices)
            );
        });
    }
}
