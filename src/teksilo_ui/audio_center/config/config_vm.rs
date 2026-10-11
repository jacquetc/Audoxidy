//! [`ConfigViewModel`]: the state of the Audio Center's configuration tab.
//!
//! Port of the configuration part of `AudioCenterManager`
//! (`src/gui/audio_center.rs:151-296`, `update` `:1193-1560`,
//! `sync_from_engine` `:806`, `refresh_system_status_cache` `:298`):
//!
//! - the **selections** are the manager's `selected_*` and `system_*` fields.
//!   They start from what the facade computes for Iced's first open
//!   ([`audio_settings::initial_pending`]), read when the tab is first shown
//!   and not again, so a reopened panel keeps the user's pending choices.
//!   Choosing anything enables Apply (`apply_enabled`);
//! - **Predeterminado** replaces the selections with the defaults
//!   ([`audio_settings::reset_to_defaults`]) and enables Apply;
//! - **Aplicar** runs [`audio_settings::apply`] on the engine worker, in
//!   Iced's order. While it runs the button is disabled (the Iced UI is
//!   frozen instead). Whatever the outcome, Apply is disabled again after it
//!   (Iced's `apply_enabled = false`); a stream error shows an error toast as
//!   well as the log line (quirk fix 8; Iced only logs it);
//! - **Reiniciar** restarts the sound-server services on the engine worker;
//! - the **information column** is [`InfoRows`]: the current track from the
//!   database (re-read when the track changes), and the output stream from
//!   the engine (re-read on every device change);
//! - the **sound-server status** is polled every 3 s by the tab while it is
//!   shown ([`ConfigViewModel::poll`]), one request at a time, on the engine
//!   worker (quirk fix 5: Iced runs `pw-metadata` on the UI thread).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use teksilo::prelude::*;
use teksilo::widgets::{EventContextToastExt, Toast};

use crate::audio::engine::{AudioDeviceInfo, BitDepth};
use crate::frontend::commands::audio_settings::{
    self, ApplyOutcome, InputInfo, OutputInfo, PendingAudioSettings, SystemStatus,
};
use crate::frontend::{AppContext, FrontendError, Latest, Reply};
use crate::logic::audio_settings::{self as rules, SystemSelection};
use crate::teksilo_ui::app::services::{AppServices, AudioCenterLink, audio_center_tab};
use crate::teksilo_ui::models::AudioDevicesModel;
use crate::teksilo_ui::models::load::finish_tracked;
use crate::teksilo_ui::singles::SingleNowPlaying;

/// How often the tab asks the sound server for its status
/// (`refresh_system_status_cache`, `audio_center.rs:298`).
pub const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// What the information column shows, as data; the tab formats it.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoRows {
    /// The current track's title; empty when nothing is loaded.
    pub title: String,
    /// The current track's container format.
    pub format: Option<String>,
    /// The file's sample rate, in Hz.
    pub input_rate: Option<u32>,
    /// The file's bits per sample.
    pub input_depth: Option<u32>,
    /// The file's channel count.
    pub input_channels: Option<u16>,
    /// The output stream's sample rate, in Hz (`None` before the first load).
    pub output_rate: Option<u32>,
    /// The output stream's sample format, as the engine describes it,
    /// normalised for display.
    pub output_depth: Option<String>,
    /// The output stream's channel count.
    pub output_channels: Option<u16>,
    /// Track ReplayGain, in dB.
    pub track_gain: Option<f64>,
    /// Album ReplayGain, in dB.
    pub album_gain: Option<f64>,
    /// The buffer size the output callback last published; 0 is Auto.
    pub buffer_size: u32,
}

impl InfoRows {
    /// The rows for `input` and `output` (`None` before the first load).
    pub fn new(input: &InputInfo, output: Option<&OutputInfo>) -> Self {
        Self {
            title: input.title.clone(),
            format: input.format.clone().filter(|format| !format.is_empty()),
            input_rate: input.sample_rate,
            input_depth: input.bit_depth,
            input_channels: input.channels,
            output_rate: output.map(|o| o.sample_rate),
            output_depth: output.map(|o| o.bit_depth.clone()),
            output_channels: output.map(|o| o.channels),
            track_gain: input.track_gain,
            album_gain: input.album_gain,
            buffer_size: output.map_or(0, |o| o.buffer_size),
        }
    }

    /// The format, or `--` (`:2562`).
    pub fn format_text(&self) -> String {
        self.format.clone().unwrap_or_else(|| "--".to_string())
    }

    /// The file's frequency: "44100 Hz", or `--` (`:2370`).
    pub fn input_rate_text(&self) -> String {
        self.input_rate
            .map_or_else(|| "--".to_string(), |rate| format!("{rate} Hz"))
    }

    /// The file's depth: "16 bits", or `--` (`:2376`).
    pub fn input_depth_text(&self) -> String {
        self.input_depth
            .map_or_else(|| "--".to_string(), |bits| format!("{bits} bits"))
    }

    /// The output's frequency: "48000 Hz" (`:2490`); `--` before the first
    /// load.
    pub fn output_rate_text(&self) -> String {
        self.output_rate
            .map_or_else(|| "--".to_string(), |rate| format!("{rate} Hz"))
    }

    /// The output's depth, or `--` before the first load.
    pub fn output_depth_text(&self) -> String {
        self.output_depth
            .clone()
            .unwrap_or_else(|| "--".to_string())
    }

    /// The gains: "Album | Canción" with two decimals, `--` for a missing one
    /// (`:2406-2411`).
    pub fn gain_text(&self) -> String {
        match (self.album_gain, self.track_gain) {
            (Some(album), Some(track)) => format!("{album:.2} dB | {track:.2} dB"),
            (Some(album), None) => format!("{album:.2} dB | --"),
            (None, Some(track)) => format!("-- | {track:.2} dB"),
            (None, None) => "-- | --".to_string(),
        }
    }

    /// The quantum Audoxidy runs at: the frames, or "Auto" (`:2425`).
    pub fn quantum_text(&self) -> String {
        if self.buffer_size > 0 {
            self.buffer_size.to_string()
        } else {
            "Auto".to_string()
        }
    }
}

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    devices: AudioDevicesModel,
    now_playing: SingleNowPlaying,
    link: AudioCenterLink,

    host: Signal<Option<String>>,
    device: Signal<Option<String>>,
    sample_rate: Signal<Option<u32>>,
    bit_depth: Signal<Option<BitDepth>>,
    channels: Signal<Option<u16>>,
    buffer_size: Signal<Option<Option<u32>>>,
    system_rate: Signal<Option<SystemSelection<u32>>>,
    system_quantum: Signal<Option<SystemSelection<u32>>>,
    apply_enabled: Signal<bool>,
    busy: Signal<bool>,

    /// Whether the selections were adopted from the engine's first answer.
    initialised: Cell<bool>,
    /// Whether the first show asked the devices model to reload.
    load_requested: Cell<bool>,
    /// The model version the first show's answer will reach.
    first_version: Cell<u64>,

    input: RefCell<InputInfo>,
    input_latest: Latest,
    info: Signal<InfoRows>,

    status: Signal<SystemStatus>,
    status_in_flight: Cell<bool>,
    next_poll: Cell<Option<Instant>>,
}

/// The configuration tab's view-model. UI thread only; clones share one
/// state.
#[derive(Clone)]
pub struct ConfigViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for ConfigViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigViewModel")
            .field("pending", &self.pending())
            .field("apply_enabled", &self.inner.apply_enabled.get())
            .field("busy", &self.inner.busy.get())
            .finish_non_exhaustive()
    }
}

impl ConfigViewModel {
    /// The tab's state over the devices model and the playing track of
    /// `services`. The selections start as Iced's manager does
    /// ([`PendingAudioSettings::default`]) until the first show reads the
    /// engine.
    pub fn new(services: &AppServices) -> Self {
        let pending = PendingAudioSettings::default();
        let input = InputInfo::default();
        let vm = Self {
            inner: Rc::new(Inner {
                ctx: services.ctx.clone(),
                runtime: services.runtime.clone(),
                devices: services.audio_devices.clone(),
                now_playing: services.now_playing.clone(),
                link: services.links.audio_center.clone(),
                host: Signal::new(pending.host.clone()),
                device: Signal::new(pending.device.clone()),
                sample_rate: Signal::new(pending.sample_rate),
                bit_depth: Signal::new(Some(pending.bit_depth)),
                channels: Signal::new(Some(pending.channels)),
                buffer_size: Signal::new(Some(pending.buffer_size)),
                system_rate: Signal::new(Some(pending.system_rate)),
                system_quantum: Signal::new(Some(pending.system_quantum)),
                apply_enabled: Signal::new(false),
                busy: Signal::new(false),
                initialised: Cell::new(false),
                load_requested: Cell::new(false),
                first_version: Cell::new(0),
                info: Signal::new(InfoRows::new(&input, None)),
                input: RefCell::new(input),
                input_latest: Latest::new(),
                status: Signal::new(SystemStatus {
                    status: "--".to_string(),
                    latency: "--".to_string(),
                }),
                status_in_flight: Cell::new(false),
                next_poll: Cell::new(None),
            }),
        };
        vm.adopt_state();
        vm
    }

    // ---- what the tab shows ----

    /// Whether the tab is on screen: the Audio Center is open on it.
    pub fn shown(&self) -> Signal<bool> {
        self.inner
            .link
            .open
            .zip(&self.inner.link.tab)
            .map(|(open, tab)| *open && *tab == audio_center_tab::CONFIG)
    }

    /// The audio host.
    pub fn host(&self) -> Signal<Option<String>> {
        self.inner.host.clone()
    }

    /// The output device, by name.
    pub fn device(&self) -> Signal<Option<String>> {
        self.inner.device.clone()
    }

    /// The player's sample rate.
    pub fn sample_rate(&self) -> Signal<Option<u32>> {
        self.inner.sample_rate.clone()
    }

    /// The output bit depth.
    pub fn bit_depth(&self) -> Signal<Option<BitDepth>> {
        self.inner.bit_depth.clone()
    }

    /// The output channel count.
    pub fn channels(&self) -> Signal<Option<u16>> {
        self.inner.channels.clone()
    }

    /// The buffer size (`Some(None)` is automatic).
    pub fn buffer_size(&self) -> Signal<Option<Option<u32>>> {
        self.inner.buffer_size.clone()
    }

    /// The sound server's rate selection.
    pub fn system_rate(&self) -> Signal<Option<SystemSelection<u32>>> {
        self.inner.system_rate.clone()
    }

    /// The sound server's quantum selection.
    pub fn system_quantum(&self) -> Signal<Option<SystemSelection<u32>>> {
        self.inner.system_quantum.clone()
    }

    /// Whether Apply can be pressed: a selection changed since the last
    /// Apply, and none is running.
    pub fn apply_enabled(&self) -> Signal<bool> {
        self.inner
            .apply_enabled
            .zip(&self.inner.busy)
            .map(|(enabled, busy)| *enabled && !*busy)
    }

    /// Whether an Apply is running on the engine worker.
    pub fn busy(&self) -> Signal<bool> {
        self.inner.busy.clone()
    }

    /// The engine's non-modal notice (the saved output is gone and the
    /// default is used), if any.
    pub fn notice(&self) -> Signal<Option<String>> {
        self.inner.now_playing.notice()
    }

    /// The information column's rows.
    pub fn info(&self) -> Signal<InfoRows> {
        self.inner.info.clone()
    }

    /// The sound server's status and latency, as last polled.
    pub fn system_status(&self) -> Signal<SystemStatus> {
        self.inner.status.clone()
    }

    /// Bumps whenever the host or device lists change.
    pub fn lists_version(&self) -> Signal<u64> {
        self.inner.devices.version()
    }

    /// The audio hosts, in the order the engine lists them.
    pub fn host_names(&self) -> Vec<String> {
        let hosts = self.inner.devices.hosts();
        (0..hosts.len())
            .filter_map(|index| hosts.with_item(index, Clone::clone))
            .collect()
    }

    /// The output devices' names, in the order the engine lists them.
    pub fn device_names(&self) -> Vec<String> {
        let devices = self.inner.devices.devices();
        (0..devices.len())
            .filter_map(|index| devices.with_item(index, |row| row.name.clone()))
            .collect()
    }

    /// The selections as Apply takes them. The device's stable id is looked up
    /// by name at this moment, as Iced does (`:1264`).
    pub fn pending(&self) -> PendingAudioSettings {
        let inner = &self.inner;
        let devices = self.engine_devices();
        let device = inner.device.get();
        PendingAudioSettings {
            host: inner.host.get(),
            device_id: audio_settings::device_id_for(&devices, device.as_deref()),
            device,
            sample_rate: inner.sample_rate.get(),
            bit_depth: inner.bit_depth.get().unwrap_or(rules::DEFAULT_BIT_DEPTH),
            channels: inner.channels.get().unwrap_or(rules::DEFAULT_CHANNELS),
            buffer_size: inner.buffer_size.get().unwrap_or(None),
            system_rate: inner.system_rate.get().unwrap_or(SystemSelection::Default),
            system_quantum: inner
                .system_quantum
                .get()
                .unwrap_or(SystemSelection::Default),
        }
    }

    fn engine_devices(&self) -> Vec<AudioDeviceInfo> {
        self.inner
            .devices
            .state()
            .get()
            .map(|state| state.devices.clone())
            .unwrap_or_default()
    }

    fn engine_hosts(&self) -> Vec<String> {
        self.inner
            .devices
            .state()
            .get()
            .map(|state| state.hosts.clone())
            .unwrap_or_default()
    }

    // ---- following the engine ----

    /// Follows the devices model and the playing track. Call it from the
    /// tab's `build`; the effects live as long as the tab.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let vm = self.clone();
        ctx.effect(&self.inner.devices.version(), move |_| vm.adopt_state());
        let vm = self.clone();
        ctx.effect(&self.inner.now_playing.path(), move |_| vm.refresh_input());
        self.adopt_state();
    }

    /// The tab appeared. The first time, it asks for the engine's state, which
    /// the selections adopt (Iced's `sync_from_engine` on the first open); it
    /// also polls the sound server at once.
    pub fn on_shown(&self) {
        let inner = &self.inner;
        if !inner.load_requested.replace(true) {
            inner.first_version.set(inner.devices.version().get() + 1);
            inner.devices.reload();
        }
        self.inner.next_poll.set(None);
    }

    /// Takes in the devices model's answer: the first one asked for by the
    /// first show provides the selections, every one provides the output
    /// stream and the track.
    fn adopt_state(&self) {
        let inner = &self.inner;
        let Some(state) = inner.devices.state().get() else {
            return;
        };
        if !inner.initialised.get()
            && inner.load_requested.get()
            && inner.devices.version().get() >= inner.first_version.get()
        {
            inner.initialised.set(true);
            self.set_pending(&state.pending);
        }
        *inner.input.borrow_mut() = state.input.clone();
        inner
            .info
            .set_if_changed(InfoRows::new(&state.input, Some(&state.output)));
    }

    /// The playing track changed: reads its technical tags again.
    fn refresh_input(&self) {
        let inner = &self.inner;
        let reply = inner
            .input_latest
            .track(audio_settings::input_info(&inner.ctx));
        let vm = self.clone();
        finish_tracked(
            &inner.runtime,
            "the audio information",
            reply,
            move |input| {
                let Some(input) = input else {
                    return;
                };
                let output = vm.inner.devices.state().get();
                vm.inner.info.set_if_changed(InfoRows::new(
                    &input,
                    output.as_deref().map(|state| &state.output),
                ));
                *vm.inner.input.borrow_mut() = input;
            },
        );
    }

    fn set_pending(&self, pending: &PendingAudioSettings) {
        let inner = &self.inner;
        inner.host.set_if_changed(pending.host.clone());
        inner.device.set_if_changed(pending.device.clone());
        inner.sample_rate.set_if_changed(pending.sample_rate);
        inner.bit_depth.set_if_changed(Some(pending.bit_depth));
        inner.channels.set_if_changed(Some(pending.channels));
        inner.buffer_size.set_if_changed(Some(pending.buffer_size));
        inner.system_rate.set_if_changed(Some(pending.system_rate));
        inner
            .system_quantum
            .set_if_changed(Some(pending.system_quantum));
    }

    // ---- the user's actions ----

    /// A selection was chosen: Apply becomes available. The combo boxes write
    /// the signals themselves; this is their `on_select`.
    pub fn selection_changed(&self) {
        self.inner.apply_enabled.set(true);
    }

    /// "Predeterminado" (`ResetToDefaults`, `:1230`).
    pub fn reset_to_defaults(&self) {
        let reset = audio_settings::reset_to_defaults(
            &self.pending(),
            &self.engine_hosts(),
            &self.engine_devices(),
        );
        self.set_pending(&reset);
        self.inner.apply_enabled.set(true);
    }

    /// "Reiniciar Servicio de Audio" (`RestartService`, `:1330`): restarts the
    /// sound server's services on the engine worker.
    pub fn restart_service(&self) {
        let reply = audio_settings::restart_audio_services(&self.inner.ctx);
        crate::teksilo_ui::models::load::finish_reply(
            &self.inner.runtime,
            "the audio service restart",
            reply,
            |_| {},
        );
    }

    /// Starts Apply when it is available: marks the tab busy and queues the
    /// job on the engine worker. `None` when Apply is disabled or already
    /// running. Finish with [`finish_apply`](Self::finish_apply).
    pub fn begin_apply(&self) -> Option<Reply<ApplyOutcome>> {
        let inner = &self.inner;
        if !inner.apply_enabled.get() || inner.busy.get() {
            return None;
        }
        inner.busy.set(true);
        Some(audio_settings::apply(&inner.ctx, self.pending()))
    }

    /// Takes in the outcome of Apply. Apply is disabled again whatever the
    /// outcome, as Iced does; the device keys were saved either way. A stream
    /// error is shown as an error toast (quirk fix 8).
    pub fn finish_apply(
        &self,
        result: Result<ApplyOutcome, FrontendError>,
        ctx: &mut EventContext,
    ) {
        self.inner.busy.set(false);
        self.inner.apply_enabled.set(false);
        if let Err(error) = result {
            tracing::error!("the audio settings could not be applied: {error}");
            ctx.show_toast(
                Toast::error(tr!(ac_config_apply_failed()))
                    .body(tr!(ac_config_apply_failed_body(error = error.to_string()))),
            );
        }
    }

    /// "Aplicar" (`ApplySettings`, `:1240`): [`begin_apply`](Self::begin_apply),
    /// then [`finish_apply`](Self::finish_apply) once the engine worker
    /// answers.
    pub fn apply(&self, ctx: &mut EventContext) {
        let Some(reply) = self.begin_apply() else {
            return;
        };
        let vm = self.clone();
        ctx.spawn_local_with(async move { reply.recv().await }, move |result, ctx| {
            vm.finish_apply(result, ctx)
        })
        .detach();
    }

    // ---- the sound-server status ----

    /// Called on every frame the tab is shown: when `now` is due, asks the
    /// engine worker for the sound server's status (one request at a time)
    /// and schedules the next for [`POLL_INTERVAL`] later. Frames something
    /// else requested call it too, so it reads the clock and counts nothing.
    pub fn poll(&self, now: Instant) {
        let inner = &self.inner;
        if inner.next_poll.get().is_some_and(|due| now < due) {
            return;
        }
        inner.next_poll.set(Some(now + POLL_INTERVAL));
        if inner.status_in_flight.replace(true) {
            return;
        }
        let reply = audio_settings::system_status(&inner.ctx);
        let vm = self.clone();
        crate::teksilo_ui::models::load::finish_reply(
            &inner.runtime,
            "the sound server status",
            reply,
            move |status| {
                vm.inner.status_in_flight.set(false);
                if let Some(status) = status {
                    vm.inner.status.set_if_changed(status);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests;
