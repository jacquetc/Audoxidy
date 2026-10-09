//! Deterministic stand-ins for the backend, for tests and the `teksilo-mocks`
//! build.
//!
//! [`FakePlayer`] keeps its own [`AudioState`], [`DspChain`] and [`MeterData`]
//! and moves only when told to ([`FakePlayer::advance`],
//! [`FakePlayer::finish_track`]). It reproduces the two decoder rules the UI
//! logic depends on (`src/audio/decoder.rs`):
//!
//! - when a track ends and a preload is ready, the preloaded track is promoted:
//!   it becomes current, playback continues, and `eof_reached` is set so the UI
//!   advances its playlist;
//! - a `Load` of the path that is already current and playing (and not at its
//!   end) is skipped, so the load that confirms a promotion does not reopen the
//!   file.
//!
//! Every fake records the calls it receives, so tests can assert what the
//! facade asked the backend to do, and in which order.

#![warn(missing_docs)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::{Mutex, RwLock};

use super::app_context::{AppContext, Parts};
use super::db_worker::SharedDatabase;
use super::error::FrontendError;
use super::monitor::{MemoryProbe, MemorySample};
use super::ports::{MediaControlsPort, PlayerPort, ReplayGains, SystemAudioPort, TrackRequest};
use crate::audio::AudioError;
use crate::audio::dsp::DspChain;
use crate::audio::engine::{AudioDeviceInfo, AudioSettings, AudioState, ChannelConfig};
use crate::audio::meter::MeterData;
use crate::audio::preset::EqPreset;
use crate::db::Database;
use crate::db::scanner::Scanner;
use crate::integrations::control::{ExternalControlSender, external_control_channel};
use crate::utils::config::AppConfig;

/// The duration a [`FakePlayer`] gives a track it has no duration for.
pub const DEFAULT_FAKE_DURATION_SECS: f64 = 180.0;

/// The output rate the fake reports when settings ask for "automatic".
const FAKE_DEFAULT_RATE: u32 = 48_000;

/// [`AudioSettings`] as recorded by [`FakePlayer`], comparable field by field.
#[derive(Debug, Clone)]
pub struct RecordedSettings(pub AudioSettings);

impl PartialEq for RecordedSettings {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.0, &other.0);
        a.host_id == b.host_id
            && a.device_name == b.device_name
            && a.device_id == b.device_id
            && a.sample_rate == b.sample_rate
            && a.bit_depth == b.bit_depth
            && a.channels == b.channels
            && a.buffer_size == b.buffer_size
    }
}

/// A call received by [`FakePlayer`].
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerCall {
    /// `play()`.
    Play,
    /// `pause()`.
    Pause,
    /// `set_playing(playing)`.
    SetPlaying(bool),
    /// `stop()`.
    Stop,
    /// `toggle_play_pause()`.
    TogglePlayPause,
    /// `seek(position)`.
    Seek(f64),
    /// A load that made the track current.
    Load {
        /// The loaded path.
        path: String,
        /// The gains it was loaded with.
        gains: ReplayGains,
    },
    /// A load skipped because the path is already current and playing.
    LoadSkipped {
        /// The path of the skipped load.
        path: String,
    },
    /// `preload_next(track, gains)`.
    Preload {
        /// The preloaded path.
        path: String,
        /// The gains it was preloaded with.
        gains: ReplayGains,
    },
    /// `purge_preload()`.
    PurgePreload,
    /// `crossfade_next(ms)`.
    CrossfadeNext(f64),
    /// `set_volume(volume)`.
    SetVolume(f32),
    /// `set_preamp_gain(db)`.
    SetPreampGain(f32),
    /// `set_eq_enabled(enabled)`.
    SetEqEnabled(bool),
    /// `set_eq_mode(bands)`.
    SetEqMode(usize),
    /// `set_eq_band_gain(index, db)`.
    SetEqBandGain(usize, f32),
    /// `apply_eq_preset(preset)`, by preset name.
    ApplyEqPreset(String),
    /// `apply_eq_preset_gains(..)`.
    ApplyEqPresetGains,
    /// `reset_dsp_defaults()`.
    ResetDspDefaults,
    /// `update_dsp(..)`.
    UpdateDsp,
    /// `devices()`.
    Devices,
    /// `apply_audio_settings(settings)`.
    ApplyAudioSettings(RecordedSettings),
    /// `purge_buffers()`.
    PurgeBuffers,
}

struct FakePlayerInner {
    calls: Vec<PlayerCall>,
    preload: Option<(TrackRequest, ReplayGains)>,
    durations: HashMap<String, f64>,
    default_duration: f64,
    hosts: Vec<String>,
    devices: Vec<AudioDeviceInfo>,
    buffer_size: u32,
    apply_error: Option<String>,
}

impl FakePlayerInner {
    fn duration_for(&self, path: &str) -> f64 {
        self.durations
            .get(path)
            .copied()
            .unwrap_or(self.default_duration)
    }
}

/// A [`PlayerPort`] with no audio device; see the module documentation.
pub struct FakePlayer {
    state: Arc<RwLock<AudioState>>,
    dsp: RwLock<DspChain>,
    meter: Arc<MeterData>,
    inner: Mutex<FakePlayerInner>,
}

impl Default for FakePlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl FakePlayer {
    /// A stopped player with the engine's default state, one host
    /// (`"Fake"`) and one device (`"Fake Output"`).
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AudioState::default())),
            dsp: RwLock::new(DspChain::default()),
            meter: Arc::new(MeterData::new()),
            inner: Mutex::new(FakePlayerInner {
                calls: Vec::new(),
                preload: None,
                durations: HashMap::new(),
                default_duration: DEFAULT_FAKE_DURATION_SECS,
                hosts: vec!["Fake".to_string()],
                devices: vec![AudioDeviceInfo {
                    name: "Fake Output".to_string(),
                    id: Some("fake-output".to_string()),
                }],
                buffer_size: 512,
                apply_error: None,
            }),
        }
    }

    /// The calls received so far, oldest first.
    pub fn calls(&self) -> Vec<PlayerCall> {
        self.inner.lock().calls.clone()
    }

    /// Returns the calls received so far and forgets them.
    pub fn take_calls(&self) -> Vec<PlayerCall> {
        std::mem::take(&mut self.inner.lock().calls)
    }

    /// The path of the pending preload, if any.
    pub fn preloaded_path(&self) -> Option<String> {
        self.inner
            .lock()
            .preload
            .as_ref()
            .map(|(track, _)| track.path.clone())
    }

    /// Gives `path` a duration of `secs` (other tracks get
    /// [`DEFAULT_FAKE_DURATION_SECS`]).
    pub fn set_track_duration(&self, path: &str, secs: f64) {
        self.inner.lock().durations.insert(path.to_string(), secs);
    }

    /// Sets the hosts returned by `available_hosts`.
    pub fn set_hosts(&self, hosts: Vec<String>) {
        self.inner.lock().hosts = hosts;
    }

    /// Sets the devices returned by `devices`.
    pub fn set_devices(&self, devices: Vec<AudioDeviceInfo>) {
        self.inner.lock().devices = devices;
    }

    /// Sets the value returned by `buffer_size`.
    pub fn set_buffer_size(&self, frames: u32) {
        self.inner.lock().buffer_size = frames;
    }

    /// Makes every later `apply_audio_settings` fail with `message` (as a
    /// stream error, with the engine's notice set), or succeed again with
    /// `None`.
    pub fn fail_apply_audio_settings(&self, message: Option<String>) {
        self.inner.lock().apply_error = message;
    }

    /// Writes meter levels, in dBFS, as the decoder would.
    pub fn set_meter(&self, peak_l: f32, rms_l: f32, peak_r: f32, rms_r: f32) {
        self.meter.write_peak_rms(peak_l, rms_l, peak_r, rms_r);
    }

    /// Moves the position forward by `secs` while playing. Reaching the end of
    /// the track calls [`FakePlayer::finish_track`]; the return value says
    /// whether that happened.
    pub fn advance(&self, secs: f64) -> bool {
        let reached_end = {
            let mut state = self.state.write();
            if !state.is_playing {
                return false;
            }
            state.current_pos_sec += secs.max(0.0);
            state.total_duration_sec > 0.0 && state.current_pos_sec >= state.total_duration_sec
        };
        if reached_end {
            self.finish_track();
        }
        reached_end
    }

    /// Ends the current track as the decoder does: with a preload, the
    /// preloaded track is promoted and keeps playing, with `eof_reached` set;
    /// without one, playback stops at the end with `eof_reached` set.
    pub fn finish_track(&self) {
        let mut inner = self.inner.lock();
        if let Some((track, gains)) = inner.preload.take() {
            let duration = inner.duration_for(&track.path);
            let mut state = self.state.write();
            apply_track(&mut state, &track, gains, duration);
            state.eof_reached = true;
        } else {
            let mut state = self.state.write();
            state.is_playing = false;
            state.current_pos_sec = state.total_duration_sec;
            state.eof_reached = true;
        }
    }

    fn record(&self, call: PlayerCall) {
        self.inner.lock().calls.push(call);
    }
}

/// Makes `track` the current one, from position 0.
fn apply_track(state: &mut AudioState, track: &TrackRequest, gains: ReplayGains, duration: f64) {
    state.title = track.title.clone();
    state.artist = track.artist.clone();
    state.album = track.album.clone();
    state.cover_path = track.cover_path.clone();
    state.path = track.path.clone();
    state.replay_gain_track = gains.track.map(|g| g as f32);
    state.replay_gain_album = gains.album.map(|g| g as f32);
    state.total_duration_sec = duration;
    state.current_pos_sec = 0.0;
}

impl PlayerPort for FakePlayer {
    fn play(&self) {
        self.record(PlayerCall::Play);
        self.state.write().is_playing = true;
    }

    fn pause(&self) {
        self.record(PlayerCall::Pause);
        self.state.write().is_playing = false;
    }

    fn set_playing(&self, playing: bool) {
        self.record(PlayerCall::SetPlaying(playing));
        self.state.write().is_playing = playing;
    }

    fn stop(&self) {
        self.record(PlayerCall::Stop);
        let mut state = self.state.write();
        state.is_playing = false;
        state.current_pos_sec = 0.0;
    }

    fn toggle_play_pause(&self) {
        self.record(PlayerCall::TogglePlayPause);
        let mut state = self.state.write();
        state.is_playing = !state.is_playing;
    }

    fn is_playing(&self) -> bool {
        self.state.read().is_playing
    }

    fn seek(&self, position_secs: f64) {
        self.record(PlayerCall::Seek(position_secs));
        if !position_secs.is_finite() {
            return;
        }
        let mut state = self.state.write();
        let mut target = position_secs.max(0.0);
        if state.total_duration_sec > 0.0 {
            target = target.min(state.total_duration_sec);
        }
        state.current_pos_sec = target;
    }

    fn take_eof(&self) -> bool {
        std::mem::take(&mut self.state.write().eof_reached)
    }

    fn load_file(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
        let mut inner = self.inner.lock();
        let mut state = self.state.write();
        if state.path == track.path && !state.eof_reached && state.is_playing {
            inner.calls.push(PlayerCall::LoadSkipped {
                path: track.path.clone(),
            });
            return Ok(());
        }
        inner.calls.push(PlayerCall::Load {
            path: track.path.clone(),
            gains,
        });
        // A real load discards the pending preload and any mix.
        inner.preload = None;
        let duration = inner.duration_for(&track.path);
        apply_track(&mut state, track, gains, duration);
        state.eof_reached = false;
        Ok(())
    }

    fn preload_next(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
        let mut inner = self.inner.lock();
        inner.calls.push(PlayerCall::Preload {
            path: track.path.clone(),
            gains,
        });
        inner.preload = Some((track.clone(), gains));
        Ok(())
    }

    fn purge_preload(&self) -> Result<(), AudioError> {
        let mut inner = self.inner.lock();
        inner.calls.push(PlayerCall::PurgePreload);
        inner.preload = None;
        Ok(())
    }

    fn crossfade_next(&self, ms: f64) -> Result<(), AudioError> {
        let mut inner = self.inner.lock();
        inner.calls.push(PlayerCall::CrossfadeNext(ms));
        // A manual crossfade promotes the preload at once, without flagging the
        // end of a track: the UI has already advanced.
        if ms.is_finite()
            && ms > 0.0
            && let Some((track, gains)) = inner.preload.take()
        {
            let duration = inner.duration_for(&track.path);
            let mut state = self.state.write();
            apply_track(&mut state, &track, gains, duration);
            state.eof_reached = false;
        }
        Ok(())
    }

    fn set_volume(&self, volume: f32) {
        self.record(PlayerCall::SetVolume(volume));
        self.state.write().volume = volume.clamp(0.0, 1.0);
    }

    fn preamp_gain(&self) -> f32 {
        self.dsp.read().get_preamp_db()
    }

    fn set_preamp_gain(&self, db: f32) {
        self.record(PlayerCall::SetPreampGain(db));
        self.dsp.write().set_preamp_db(db);
    }

    fn eq_enabled(&self) -> bool {
        self.dsp.read().equalizer.enabled
    }

    fn set_eq_enabled(&self, enabled: bool) {
        self.record(PlayerCall::SetEqEnabled(enabled));
        self.dsp.write().equalizer.enabled = enabled;
    }

    fn eq_band_count(&self) -> usize {
        self.dsp.read().equalizer.bands.len()
    }

    fn set_eq_mode(&self, num_bands: usize) {
        self.record(PlayerCall::SetEqMode(num_bands));
        let mut dsp = self.dsp.write();
        if dsp.equalizer.bands.len() != num_bands {
            dsp.equalizer.set_mode(num_bands);
        }
    }

    fn eq_band(&self, index: usize) -> Option<(f32, f32)> {
        self.dsp
            .read()
            .equalizer
            .bands
            .get(index)
            .map(|band| (band.frequency, band.gain))
    }

    fn set_eq_band_gain(&self, index: usize, db: f32) {
        self.record(PlayerCall::SetEqBandGain(index, db));
        if let Some(band) = self.dsp.write().equalizer.bands.get_mut(index) {
            band.set_gain(db);
        }
    }

    fn apply_eq_preset(&self, preset: &EqPreset) {
        self.record(PlayerCall::ApplyEqPreset(preset.name.clone()));
        let mut dsp = self.dsp.write();
        dsp.set_preamp_db(preset.preamp_gain);
        dsp.equalizer
            .apply_preset_gains(&preset.get_gains_20(), &preset.get_gains_31());
    }

    fn apply_eq_preset_gains(&self, bands_20: &[f32], bands_31: &[f32]) {
        self.record(PlayerCall::ApplyEqPresetGains);
        self.dsp
            .write()
            .equalizer
            .apply_preset_gains(bands_20, bands_31);
    }

    fn reset_dsp_defaults(&self) {
        self.record(PlayerCall::ResetDspDefaults);
        let mut dsp = self.dsp.write();
        dsp.set_preamp_db(0.0);
        dsp.equalizer.reset_all();
    }

    fn update_dsp(&self, f: &mut dyn FnMut(&mut DspChain)) {
        self.record(PlayerCall::UpdateDsp);
        f(&mut self.dsp.write());
    }

    fn read_dsp(&self, f: &mut dyn FnMut(&DspChain)) {
        f(&self.dsp.read());
    }

    fn state(&self) -> Arc<RwLock<AudioState>> {
        Arc::clone(&self.state)
    }

    fn snapshot(&self) -> AudioState {
        self.state.read().clone()
    }

    fn meter(&self) -> Arc<MeterData> {
        Arc::clone(&self.meter)
    }

    fn buffer_size(&self) -> u32 {
        self.inner.lock().buffer_size
    }

    fn available_hosts(&self) -> Vec<String> {
        self.inner.lock().hosts.clone()
    }

    fn devices(&self) -> Vec<AudioDeviceInfo> {
        let mut inner = self.inner.lock();
        inner.calls.push(PlayerCall::Devices);
        inner.devices.clone()
    }

    fn apply_audio_settings(&self, settings: AudioSettings) -> Result<(), AudioError> {
        let mut inner = self.inner.lock();
        inner
            .calls
            .push(PlayerCall::ApplyAudioSettings(RecordedSettings(
                settings.clone(),
            )));
        let mut state = self.state.write();
        if let Some(message) = inner.apply_error.clone() {
            state.audio_notice = Some(message.clone());
            return Err(AudioError::StreamError(message));
        }
        state.device_sample_rate = settings.sample_rate.unwrap_or(FAKE_DEFAULT_RATE);
        if let ChannelConfig::Manual(channels) = settings.channels {
            state.channels = channels;
        }
        state.config_channels = settings.channels;
        state.device_id = settings.device_id;
        state.audio_notice = None;
        Ok(())
    }

    fn purge_buffers(&self) -> Result<(), AudioError> {
        self.record(PlayerCall::PurgeBuffers);
        Ok(())
    }
}

/// A call received by [`FakeMediaControls`].
#[derive(Debug, Clone, PartialEq)]
pub enum MediaCall {
    /// `update(state)`, with the fields that matter to the system panel.
    Update {
        /// Whether the state was playing.
        is_playing: bool,
        /// The track title.
        title: String,
        /// The position, in seconds.
        position_secs: f64,
    },
    /// `set_volume(volume)`.
    SetVolume(f64),
}

/// A [`MediaControlsPort`] that records its calls.
#[derive(Debug, Default)]
pub struct FakeMediaControls {
    calls: Mutex<Vec<MediaCall>>,
}

impl FakeMediaControls {
    /// A fake with no call recorded.
    pub fn new() -> Self {
        Self::default()
    }

    /// The calls received so far, oldest first.
    pub fn calls(&self) -> Vec<MediaCall> {
        self.calls.lock().clone()
    }

    /// How many times `update` was called.
    pub fn update_count(&self) -> usize {
        self.calls
            .lock()
            .iter()
            .filter(|call| matches!(call, MediaCall::Update { .. }))
            .count()
    }
}

impl MediaControlsPort for FakeMediaControls {
    fn update(&self, state: &AudioState) {
        self.calls.lock().push(MediaCall::Update {
            is_playing: state.is_playing,
            title: state.title.clone(),
            position_secs: state.current_pos_sec,
        });
    }

    fn set_volume(&self, volume: f64) {
        self.calls.lock().push(MediaCall::SetVolume(volume));
    }
}

/// A call received by [`FakeSystemAudio`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemAudioCall {
    /// `is_pipewire_active()`.
    IsPipewireActive,
    /// `apply_pipewire_clock(rate, quantum)`.
    ApplyPipewireClock(Option<u32>, Option<u32>),
    /// `persist_pipewire_conf(rate, quantum)`.
    PersistPipewireConf(Option<u32>, Option<u32>),
    /// `persist_pulse_conf(rate, quantum)`.
    PersistPulseConf(Option<u32>, Option<u32>),
    /// `restart_audio_services()`.
    RestartAudioServices,
    /// `system_sound_status()`.
    SystemSoundStatus,
}

/// A [`SystemAudioPort`] that records its calls and returns canned values.
#[derive(Debug)]
pub struct FakeSystemAudio {
    pipewire: AtomicBool,
    status: Mutex<(String, String)>,
    calls: Mutex<Vec<SystemAudioCall>>,
}

impl Default for FakeSystemAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeSystemAudio {
    /// A fake PipeWire server at 48 kHz.
    pub fn new() -> Self {
        Self {
            pipewire: AtomicBool::new(true),
            status: Mutex::new(("PipeWire 48000 Hz".to_string(), "1024 / 48000".to_string())),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Chooses whether `is_pipewire_active` answers `true` (PipeWire) or
    /// `false` (PulseAudio).
    pub fn set_pipewire_active(&self, active: bool) {
        self.pipewire.store(active, Ordering::Release);
    }

    /// Sets the value returned by `system_sound_status`.
    pub fn set_status(&self, status: impl Into<String>, latency: impl Into<String>) {
        *self.status.lock() = (status.into(), latency.into());
    }

    /// The calls received so far, oldest first.
    pub fn calls(&self) -> Vec<SystemAudioCall> {
        self.calls.lock().clone()
    }

    fn record(&self, call: SystemAudioCall) {
        self.calls.lock().push(call);
    }
}

impl SystemAudioPort for FakeSystemAudio {
    fn is_pipewire_active(&self) -> bool {
        self.record(SystemAudioCall::IsPipewireActive);
        self.pipewire.load(Ordering::Acquire)
    }

    fn apply_pipewire_clock(&self, rate: Option<u32>, quantum: Option<u32>) {
        self.record(SystemAudioCall::ApplyPipewireClock(rate, quantum));
    }

    fn persist_pipewire_conf(&self, rate: Option<u32>, quantum: Option<u32>) {
        self.record(SystemAudioCall::PersistPipewireConf(rate, quantum));
    }

    fn persist_pulse_conf(&self, rate: Option<u32>, quantum: Option<u32>) {
        self.record(SystemAudioCall::PersistPulseConf(rate, quantum));
    }

    fn restart_audio_services(&self) {
        self.record(SystemAudioCall::RestartAudioServices);
    }

    fn system_sound_status(&self) -> (String, String) {
        self.record(SystemAudioCall::SystemSoundStatus);
        self.status.lock().clone()
    }
}

/// A [`MemoryProbe`] that returns a sample set by the test (all quiet by
/// default), so the monitor never reads the real process or system memory.
#[derive(Debug, Default)]
pub struct FakeMemoryProbe {
    sample: Mutex<MemorySample>,
}

impl FakeMemoryProbe {
    /// A probe that reports no memory pressure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the sample returned from now on.
    pub fn set_sample(&self, sample: MemorySample) {
        *self.sample.lock() = sample;
    }
}

impl MemoryProbe for FakeMemoryProbe {
    fn sample(&self) -> MemorySample {
        self.sample.lock().clone()
    }
}

/// The fakes behind an in-memory context, for tests to drive and inspect.
pub struct InMemoryFakes {
    /// The player the context uses.
    pub player: Arc<FakePlayer>,
    /// The media controls the context uses.
    pub media: Arc<FakeMediaControls>,
    /// The sound server the context uses.
    pub system_audio: Arc<FakeSystemAudio>,
    /// The memory probe the monitor reads.
    pub memory: Arc<FakeMemoryProbe>,
    /// Sends events as the system media controls would.
    pub external_control: ExternalControlSender,
}

impl AppContext {
    /// A context over one in-memory database and the fakes: no file, no audio
    /// device, no sound server, no real memory probe.
    ///
    /// The database worker and the scanner share one connection, because two
    /// `:memory:` connections would be two separate databases.
    pub fn in_memory() -> Result<Self, FrontendError> {
        Ok(Self::in_memory_with_fakes()?.0)
    }

    /// [`AppContext::in_memory`], also returning the fakes it uses.
    pub fn in_memory_with_fakes() -> Result<(Self, InMemoryFakes), FrontendError> {
        let database: SharedDatabase = Arc::new(std::sync::Mutex::new(Database::new_memory()?));
        let scanner = Arc::new(Scanner::new(Arc::clone(&database)));
        let player = Arc::new(FakePlayer::new());
        let media = Arc::new(FakeMediaControls::new());
        let system_audio = Arc::new(FakeSystemAudio::new());
        let memory = Arc::new(FakeMemoryProbe::new());
        let (external_tx, external_rx) = external_control_channel();
        let context = Self::assemble(Parts {
            database,
            scanner,
            player: player.clone(),
            media: media.clone(),
            system_audio: system_audio.clone(),
            external: external_rx,
            probe: memory.clone(),
            config: AppConfig::default(),
        })?;
        Ok((
            context,
            InMemoryFakes {
                player,
                media,
                system_audio,
                memory,
                external_control: external_tx,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str) -> TrackRequest {
        TrackRequest {
            path: path.to_string(),
            title: format!("Title of {path}"),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            cover_path: None,
        }
    }

    fn gains(track: f64) -> ReplayGains {
        ReplayGains {
            track: Some(track),
            album: None,
        }
    }

    #[test]
    fn promoting_the_preloaded_track_sets_eof_and_keeps_playing() {
        let player = FakePlayer::new();
        player.load_file(&track("a.flac"), gains(-3.0)).unwrap();
        player.play();
        player.preload_next(&track("b.flac"), gains(-5.0)).unwrap();

        player.finish_track();

        let state = player.snapshot();
        assert!(state.eof_reached);
        assert!(state.is_playing, "a promotion does not stop playback");
        assert_eq!(state.path, "b.flac");
        assert_eq!(state.title, "Title of b.flac");
        assert_eq!(state.current_pos_sec, 0.0);
        assert_eq!(state.replay_gain_track, Some(-5.0));
        assert_eq!(player.preloaded_path(), None);
    }

    #[test]
    fn a_duplicate_load_of_the_promoted_track_is_skipped() {
        let player = FakePlayer::new();
        player.load_file(&track("a.flac"), gains(-3.0)).unwrap();
        player.play();
        player.preload_next(&track("b.flac"), gains(-5.0)).unwrap();
        player.finish_track();
        assert!(player.take_eof());
        player.take_calls();

        player.load_file(&track("b.flac"), gains(-5.0)).unwrap();

        assert_eq!(
            player.calls(),
            vec![PlayerCall::LoadSkipped {
                path: "b.flac".to_string()
            }]
        );
    }

    #[test]
    fn a_load_while_the_end_of_track_is_still_flagged_is_not_skipped() {
        let player = FakePlayer::new();
        player.load_file(&track("a.flac"), gains(-3.0)).unwrap();
        player.play();
        player.preload_next(&track("b.flac"), gains(-5.0)).unwrap();
        player.finish_track();
        player.take_calls();

        // The decoder only skips a load once the end of track has been taken.
        player.load_file(&track("b.flac"), gains(-5.0)).unwrap();

        assert!(matches!(player.calls()[..], [PlayerCall::Load { .. }]));
        assert!(!player.snapshot().eof_reached);
    }

    #[test]
    fn take_eof_returns_true_once_and_then_false() {
        let player = FakePlayer::new();
        player.load_file(&track("a.flac"), gains(0.0)).unwrap();
        player.play();
        player.finish_track();
        assert!(player.take_eof());
        assert!(!player.take_eof());
        assert!(!player.snapshot().eof_reached);
    }

    #[test]
    fn a_track_that_ends_without_a_preload_stops_playback() {
        let player = FakePlayer::new();
        player.set_track_duration("a.flac", 10.0);
        player.load_file(&track("a.flac"), gains(0.0)).unwrap();
        player.play();
        assert!(!player.advance(4.0));
        assert_eq!(player.snapshot().current_pos_sec, 4.0);
        assert!(player.advance(6.0));
        let state = player.snapshot();
        assert!(state.eof_reached);
        assert!(!state.is_playing);
    }

    #[test]
    fn advance_does_nothing_while_paused() {
        let player = FakePlayer::new();
        player.load_file(&track("a.flac"), gains(0.0)).unwrap();
        assert!(!player.advance(5.0));
        assert_eq!(player.snapshot().current_pos_sec, 0.0);
    }

    #[test]
    fn a_manual_crossfade_promotes_the_preload_without_flagging_eof() {
        let player = FakePlayer::new();
        player.load_file(&track("a.flac"), gains(0.0)).unwrap();
        player.play();
        player.preload_next(&track("b.flac"), gains(0.0)).unwrap();
        player.crossfade_next(1000.0).unwrap();
        let state = player.snapshot();
        assert_eq!(state.path, "b.flac");
        assert!(!state.eof_reached);
    }

    #[test]
    fn set_meter_writes_the_levels_the_ui_reads() {
        let player = FakePlayer::new();
        player.set_meter(-6.0, -12.0, -3.0, -9.0);
        let meter = player.meter();
        assert_eq!(meter.read_peak_l(), -6.0);
        assert_eq!(meter.read_rms_l(), -12.0);
        assert_eq!(meter.read_peak_r(), -3.0);
        assert_eq!(meter.read_rms_r(), -9.0);
    }

    #[test]
    fn the_equalizer_calls_change_the_fake_dsp_chain() {
        let player = FakePlayer::new();
        player.set_eq_mode(20);
        assert_eq!(player.eq_band_count(), 20);
        player.set_eq_band_gain(3, 4.5);
        assert_eq!(player.eq_band(3).map(|(_, gain)| gain), Some(4.5));
        player.set_preamp_gain(-2.0);
        assert!((player.preamp_gain() + 2.0).abs() < 1e-3);
        player.reset_dsp_defaults();
        assert_eq!(player.eq_band(3).map(|(_, gain)| gain), Some(0.0));
    }

    #[test]
    fn a_failing_apply_reports_the_error_and_sets_the_notice() {
        let player = FakePlayer::new();
        player.fail_apply_audio_settings(Some("device busy".to_string()));
        let result = player.apply_audio_settings(AudioSettings::default());
        assert!(matches!(result, Err(AudioError::StreamError(_))));
        assert_eq!(
            player.snapshot().audio_notice.as_deref(),
            Some("device busy")
        );
    }

    #[test]
    fn the_media_and_system_fakes_record_their_calls() {
        let media = FakeMediaControls::new();
        media.update(&AudioState::default());
        media.set_volume(0.5);
        assert_eq!(media.update_count(), 1);
        assert_eq!(media.calls().len(), 2);

        let system = FakeSystemAudio::new();
        system.set_pipewire_active(false);
        assert!(!system.is_pipewire_active());
        system.persist_pulse_conf(Some(44_100), None);
        assert_eq!(
            system.calls(),
            vec![
                SystemAudioCall::IsPipewireActive,
                SystemAudioCall::PersistPulseConf(Some(44_100), None)
            ]
        );
    }
}
