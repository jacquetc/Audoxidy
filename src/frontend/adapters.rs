//! The real implementations of the [ports](super::ports).
//!
//! [`AudioManagerPlayer`] wraps the audio engine, [`SystemMediaControlsAdapter`]
//! the souvlaki bridge, and [`SystemAudioAdapter`] the PipeWire/PulseAudio
//! functions. [`NoopMediaControls`] stands in when the system media controls
//! cannot be created: the Iced UI aborts in that case, the facade logs a
//! warning and carries on without them.

#![warn(missing_docs)]

use std::sync::Arc;

use parking_lot::RwLock;

use super::ports::{MediaControlsPort, PlayerPort, ReplayGains, SystemAudioPort, TrackRequest};
use crate::audio::dsp::DspChain;
use crate::audio::engine::{AudioDeviceInfo, AudioSettings, AudioState};
use crate::audio::meter::MeterData;
use crate::audio::preset::EqPreset;
use crate::audio::{AudioError, AudioManager};
use crate::integrations::media_controls::SystemMediaControls;
use crate::integrations::system_audio;

/// [`PlayerPort`] over the real [`AudioManager`].
///
/// Loads and preloads go through `AudioManager::load_file_with_gains` and
/// `preload_next_with_gains`: the gains the caller passes are used as given,
/// and the manager never queries the database itself.
pub struct AudioManagerPlayer {
    manager: Arc<AudioManager>,
}

impl AudioManagerPlayer {
    /// Wraps `manager`.
    pub fn new(manager: Arc<AudioManager>) -> Self {
        Self { manager }
    }

    /// The wrapped manager.
    pub fn manager(&self) -> &Arc<AudioManager> {
        &self.manager
    }
}

impl PlayerPort for AudioManagerPlayer {
    fn play(&self) {
        self.manager.play();
    }

    fn pause(&self) {
        self.manager.pause();
    }

    fn set_playing(&self, playing: bool) {
        self.manager.set_playing(playing);
    }

    fn stop(&self) {
        self.manager.stop();
    }

    fn toggle_play_pause(&self) {
        self.manager.toggle_play_pause();
    }

    fn is_playing(&self) -> bool {
        self.manager.is_playing()
    }

    fn seek(&self, position_secs: f64) {
        self.manager.seek(position_secs);
    }

    fn take_eof(&self) -> bool {
        let state = self.manager.state();
        let mut state = state.write();
        std::mem::take(&mut state.eof_reached)
    }

    fn load_file(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
        self.manager.load_file_with_gains(
            &track.path,
            track.title.clone(),
            track.artist.clone(),
            track.album.clone(),
            track.cover_path.clone(),
            gains.track,
            gains.album,
        )
    }

    fn preload_next(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
        self.manager.preload_next_with_gains(
            &track.path,
            track.title.clone(),
            track.artist.clone(),
            track.album.clone(),
            track.cover_path.clone(),
            gains.track,
            gains.album,
        )
    }

    fn purge_preload(&self) -> Result<(), AudioError> {
        self.manager.purge_preload()
    }

    fn crossfade_next(&self, ms: f64) -> Result<(), AudioError> {
        self.manager.crossfade_next(ms)
    }

    fn set_volume(&self, volume: f32) {
        self.manager.set_volume(volume);
    }

    fn preamp_gain(&self) -> f32 {
        self.manager.get_preamp_gain()
    }

    fn set_preamp_gain(&self, db: f32) {
        self.manager.set_preamp_gain(db);
    }

    fn eq_enabled(&self) -> bool {
        self.manager.get_eq_enabled()
    }

    fn set_eq_enabled(&self, enabled: bool) {
        self.manager.set_eq_enabled(enabled);
    }

    fn eq_band_count(&self) -> usize {
        self.manager.get_eq_bands_count()
    }

    fn set_eq_mode(&self, num_bands: usize) {
        self.manager.set_eq_mode(num_bands);
    }

    fn eq_band(&self, index: usize) -> Option<(f32, f32)> {
        self.manager.get_eq_band_info(index)
    }

    fn set_eq_band_gain(&self, index: usize, db: f32) {
        self.manager.set_eq_band_gain(index, db);
    }

    fn apply_eq_preset(&self, preset: &EqPreset) {
        self.manager.apply_eq_preset(preset);
    }

    fn apply_eq_preset_gains(&self, bands_20: &[f32], bands_31: &[f32]) {
        self.manager.apply_eq_preset_gains(bands_20, bands_31);
    }

    fn reset_dsp_defaults(&self) {
        self.manager.reset_dsp_defaults();
    }

    fn update_dsp(&self, f: &mut dyn FnMut(&mut DspChain)) {
        self.manager.with_dsp_mut(|dsp| f(dsp));
    }

    fn read_dsp(&self, f: &mut dyn FnMut(&DspChain)) {
        self.manager.with_dsp(|dsp| f(dsp));
    }

    fn state(&self) -> Arc<RwLock<AudioState>> {
        self.manager.state()
    }

    fn snapshot(&self) -> AudioState {
        self.manager.get_state()
    }

    fn meter(&self) -> Arc<MeterData> {
        self.manager.meter()
    }

    fn buffer_size(&self) -> u32 {
        self.manager.buffer_size()
    }

    fn available_hosts(&self) -> Vec<String> {
        self.manager.get_available_hosts()
    }

    fn devices(&self) -> Vec<AudioDeviceInfo> {
        self.manager.get_devices()
    }

    fn apply_audio_settings(&self, settings: AudioSettings) -> Result<(), AudioError> {
        self.manager.apply_audio_settings(settings)
    }

    fn purge_buffers(&self) -> Result<(), AudioError> {
        self.manager.purge_buffers()
    }
}

/// [`MediaControlsPort`] over the souvlaki bridge.
pub struct SystemMediaControlsAdapter {
    controls: SystemMediaControls,
}

impl SystemMediaControlsAdapter {
    /// Wraps `controls`.
    pub fn new(controls: SystemMediaControls) -> Self {
        Self { controls }
    }
}

impl MediaControlsPort for SystemMediaControlsAdapter {
    fn update(&self, state: &AudioState) {
        self.controls.update(state);
    }

    fn set_volume(&self, volume: f64) {
        self.controls.set_volume(volume);
    }
}

/// A [`MediaControlsPort`] that does nothing, used when the system media
/// controls are unavailable.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMediaControls;

impl MediaControlsPort for NoopMediaControls {
    fn update(&self, _state: &AudioState) {}

    fn set_volume(&self, _volume: f64) {}
}

/// [`SystemAudioPort`] over `integrations::system_audio`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemAudioAdapter;

impl SystemAudioPort for SystemAudioAdapter {
    fn is_pipewire_active(&self) -> bool {
        system_audio::is_pipewire_active()
    }

    fn apply_pipewire_clock(&self, rate: Option<u32>, quantum: Option<u32>) {
        system_audio::apply_pipewire_clock(rate, quantum);
    }

    fn persist_pipewire_conf(&self, rate: Option<u32>, quantum: Option<u32>) {
        system_audio::persist_pipewire_conf(rate, quantum);
    }

    fn persist_pulse_conf(&self, rate: Option<u32>, quantum: Option<u32>) {
        system_audio::persist_pulse_conf(rate, quantum);
    }

    fn restart_audio_services(&self) {
        system_audio::restart_audio_services();
    }

    fn system_sound_status(&self) -> (String, String) {
        system_audio::system_sound_status()
    }
}
