//! The object-safe interfaces between the facade and the backend.
//!
//! The facade never names `AudioManager`, `SystemMediaControls` or the
//! `integrations::system_audio` functions outside the adapters: commands, the
//! workers and the monitor talk to these traits. The real implementations live
//! in [`adapters`](super::adapters); the fakes used by tests and by the
//! `teksilo-mocks` build live in `fakes`. This is what lets the whole facade run
//! without an audio device or a sound server.

#![warn(missing_docs)]

use std::sync::Arc;

use parking_lot::RwLock;

use crate::audio::AudioError;
use crate::audio::dsp::DspChain;
use crate::audio::engine::{AudioDeviceInfo, AudioSettings, AudioState};
use crate::audio::meter::MeterData;
use crate::audio::preset::EqPreset;

/// The track a load or preload asks the engine to play.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackRequest {
    /// Path of the audio file.
    pub path: String,
    /// Title shown while it plays.
    pub title: String,
    /// Artist shown while it plays.
    pub artist: String,
    /// Album shown while it plays.
    pub album: String,
    /// Cached cover, if any.
    pub cover_path: Option<String>,
}

/// The ReplayGain values of a track, as stored in the database.
///
/// Callers fetch them through the database worker and always pass them
/// explicitly: the facade never relies on `AudioManager`'s own database
/// lookup.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReplayGains {
    /// Track gain, in dB.
    pub track: Option<f64>,
    /// Album gain, in dB.
    pub album: Option<f64>,
}

/// What a UI needs from the audio engine.
///
/// Everything here is quick (state locks, a command on the decoder channel)
/// except [`PlayerPort::devices`], [`PlayerPort::apply_audio_settings`] and
/// [`PlayerPort::purge_buffers`], which enumerate devices or rebuild the stream
/// and must go through the
/// [`EngineWorker`](super::engine_worker::EngineWorker).
pub trait PlayerPort: Send + Sync {
    /// Starts or resumes playback.
    fn play(&self);
    /// Pauses playback.
    fn pause(&self);
    /// Sets the play state.
    fn set_playing(&self, playing: bool);
    /// Stops playback and rewinds.
    fn stop(&self);
    /// Toggles between playing and paused.
    fn toggle_play_pause(&self);
    /// Whether the engine is playing.
    fn is_playing(&self) -> bool;
    /// Seeks to `position_secs`.
    fn seek(&self, position_secs: f64);
    /// Returns whether the engine flagged the end of a track, and clears the
    /// flag, under one write lock: it returns `true` once per end of track.
    fn take_eof(&self) -> bool;
    /// Loads `track` and makes it current, with its ReplayGain values.
    ///
    /// Like the engine, a load of the track that is already playing (not at
    /// its end) is ignored: that is how a load that confirms a gapless
    /// promotion is skipped.
    fn load_file(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError>;
    /// Preloads `track` as the next one, for a gapless transition.
    fn preload_next(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError>;
    /// Frees the preload's memory without interrupting playback.
    fn purge_preload(&self) -> Result<(), AudioError>;
    /// Starts a manual crossfade of `ms` milliseconds into the preloaded track.
    fn crossfade_next(&self, ms: f64) -> Result<(), AudioError>;
    /// Sets the volume (0.0 to 1.0).
    fn set_volume(&self, volume: f32);

    /// The preamplifier gain, in dB.
    fn preamp_gain(&self) -> f32;
    /// Sets the preamplifier gain, in dB.
    fn set_preamp_gain(&self, db: f32);
    /// Whether the equalizer is enabled.
    fn eq_enabled(&self) -> bool;
    /// Enables or disables the equalizer.
    fn set_eq_enabled(&self, enabled: bool);
    /// The number of equalizer bands (20 or 31).
    fn eq_band_count(&self) -> usize;
    /// Switches the equalizer between 20 and 31 bands.
    fn set_eq_mode(&self, num_bands: usize);
    /// The `(frequency, gain_db)` of band `index`.
    fn eq_band(&self, index: usize) -> Option<(f32, f32)>;
    /// Sets the gain of band `index`, in dB.
    fn set_eq_band_gain(&self, index: usize, db: f32);
    /// Applies a full preset (preamp and every band) atomically.
    fn apply_eq_preset(&self, preset: &EqPreset);
    /// Applies both band sets (20 and 31) without touching the preamp.
    fn apply_eq_preset_gains(&self, bands_20: &[f32], bands_31: &[f32]);
    /// Resets the preamp to 0 dB and flattens the equalizer.
    fn reset_dsp_defaults(&self);
    /// Runs `f` with write access to the DSP chain.
    fn update_dsp(&self, f: &mut dyn FnMut(&mut DspChain));
    /// Runs `f` with read access to the DSP chain.
    fn read_dsp(&self, f: &mut dyn FnMut(&DspChain));

    /// The engine's shared state (also used to set the mix settings).
    fn state(&self) -> Arc<RwLock<AudioState>>;
    /// A copy of the engine's state.
    fn snapshot(&self) -> AudioState;
    /// The lock-free meter levels written by the decoder.
    fn meter(&self) -> Arc<MeterData>;
    /// The buffer size the output callback last published, in frames.
    fn buffer_size(&self) -> u32;

    /// The audio hosts (ALSA, PipeWire, WASAPI...).
    fn available_hosts(&self) -> Vec<String>;
    /// The output devices. Slow: call it through the engine worker.
    fn devices(&self) -> Vec<AudioDeviceInfo>;
    /// Applies a device configuration and rebuilds the stream. Slow: call it
    /// through the engine worker.
    fn apply_audio_settings(&self, settings: AudioSettings) -> Result<(), AudioError>;
    /// Drops the stream buffers and rebuilds the stream. Slow: call it through
    /// the engine worker.
    fn purge_buffers(&self) -> Result<(), AudioError>;
}

/// The system media controls (MPRIS on Linux, SMTC on Windows).
pub trait MediaControlsPort: Send + Sync {
    /// Reflects the engine state (play state, position, metadata).
    fn update(&self, state: &AudioState);
    /// Reflects the volume the application applied.
    fn set_volume(&self, volume: f64);
}

/// The system sound server (PipeWire or PulseAudio).
///
/// Every call starts a subprocess or writes a configuration file, so it must
/// go through the [`EngineWorker`](super::engine_worker::EngineWorker), never
/// run on the UI thread.
pub trait SystemAudioPort: Send + Sync {
    /// Whether PipeWire is the active user service.
    fn is_pipewire_active(&self) -> bool;
    /// Forces the PipeWire graph clock (rate and quantum; `None` = default).
    fn apply_pipewire_clock(&self, rate: Option<u32>, quantum: Option<u32>);
    /// Persists the clock in `pipewire.conf.d`.
    fn persist_pipewire_conf(&self, rate: Option<u32>, quantum: Option<u32>);
    /// Persists the clock in PulseAudio's `daemon.conf`.
    fn persist_pulse_conf(&self, rate: Option<u32>, quantum: Option<u32>);
    /// Restarts PipeWire, its PulseAudio bridge and WirePlumber.
    fn restart_audio_services(&self);
    /// The sound server's `(status, latency)` description.
    fn system_sound_status(&self) -> (String, String);
}
