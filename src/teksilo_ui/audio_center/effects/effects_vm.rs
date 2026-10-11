//! `EffectsViewModel`: what the effects tab shows and what its controls do.
//!
//! A `Clone` handle of `Signal`s over the engine's effect chain. Each effect's
//! on/off flag, each slider's value (in the units the slider shows), the
//! expander's Natural / Surround switch and the three downmix cards (flag and
//! gain) are signals; the widgets bind to them and write them when the user
//! acts, and the methods here forward each change to the facade
//! ([`crate::frontend::commands::dsp`]), which applies it to the engine and
//! persists the changed key. The Iced UI persisted the whole chain once, when
//! the Audio Center closed (`save_dsp_settings_to_db`,
//! `src/gui/audio_center.rs:395`); the facade persists per change, so a crash
//! loses nothing.
//!
//! - **Secondary click**: the widget writes the slider's default and calls
//!   [`EffectsViewModel::set_value`]; a value equal to the default goes through
//!   the facade's `reset_value` / `reset_downmix_mix`, which write the
//!   default (`audio_center.rs:3322`, `3405`).
//! - **Opening the tab**: [`EffectsViewModel::refresh`] reads the engine back,
//!   so the tab shows what the engine holds (the restored settings, applied
//!   at startup: quirk fix 9 and 27) and not a copy that went stale.
//! - **Errors (quirk fix 8)**: the engine change is synchronous; a failed
//!   database write shows an error toast as well as the log line.

use std::rc::Rc;

use teksilo::prelude::*;

use crate::audio::dsp::ExpanderMode;
use crate::frontend::Reply;
use crate::frontend::commands::dsp::{self, DspEffect, DspParam};
use crate::frontend::settings::DownmixChannel;
use crate::teksilo_ui::app::AppServices;

struct Inner {
    services: AppServices,
    enabled: Vec<(DspEffect, Signal<bool>)>,
    values: Vec<(DspParam, Signal<f32>)>,
    surround: Signal<bool>,
    downmix_enabled: Vec<(DownmixChannel, Signal<bool>)>,
    downmix_mix: Vec<(DownmixChannel, Signal<f32>)>,
}

/// The effects tab's view-model; see the module documentation.
#[derive(Clone)]
pub struct EffectsViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for EffectsViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectsViewModel")
            .field("surround", &self.inner.surround.get())
            .finish_non_exhaustive()
    }
}

/// Every effect with an on/off switch, in Iced's order of cards.
pub const EFFECTS: [DspEffect; 9] = [
    DspEffect::SubBass,
    DspEffect::NoiseGate,
    DspEffect::Compressor,
    DspEffect::MidBass,
    DspEffect::StereoExpander,
    DspEffect::Limiter,
    DspEffect::VoiceBoost,
    DspEffect::StereoBalance,
    DspEffect::Reverb,
];

impl EffectsViewModel {
    /// The view-model over `services`, showing what the engine holds now.
    pub fn new(services: &AppServices) -> Self {
        let vm = Self {
            inner: Rc::new(Inner {
                services: services.clone(),
                enabled: EFFECTS
                    .iter()
                    .map(|&effect| (effect, Signal::new(false)))
                    .collect(),
                values: DspParam::ALL
                    .iter()
                    .map(|&param| (param, Signal::new(param.default_value())))
                    .collect(),
                surround: Signal::new(false),
                downmix_enabled: DownmixChannel::ALL
                    .iter()
                    .map(|&channel| (channel, Signal::new(false)))
                    .collect(),
                downmix_mix: DownmixChannel::ALL
                    .iter()
                    .map(|&channel| (channel, Signal::new(dsp::downmix_default(channel))))
                    .collect(),
            }),
        };
        vm.refresh();
        vm
    }

    /// Reads the engine's effects and downmix settings into the signals.
    pub fn refresh(&self) {
        let ctx = &self.inner.services.ctx;
        let settings = dsp::current(ctx);
        for (effect, signal) in &self.inner.enabled {
            signal.set(effect.is_enabled(&settings));
        }
        for (param, signal) in &self.inner.values {
            signal.set(param.value(&settings));
        }
        self.inner
            .surround
            .set(settings.stereo_expander_mode == ExpanderMode::Surround);
        let downmix = dsp::current_downmix(ctx);
        for (channel, signal) in &self.inner.downmix_enabled {
            signal.set(downmix.enabled(*channel));
        }
        for (channel, signal) in &self.inner.downmix_mix {
            signal.set(downmix.mix(*channel));
        }
    }

    /// Whether `effect` is on.
    pub fn enabled(&self, effect: DspEffect) -> Signal<bool> {
        pick(&self.inner.enabled, &effect)
    }

    /// The value of `param`, in the units its slider shows.
    pub fn value(&self, param: DspParam) -> Signal<f32> {
        pick(&self.inner.values, &param)
    }

    /// Whether the stereo expander is in Surround mode (off: Natural).
    pub fn surround(&self) -> Signal<bool> {
        self.inner.surround.clone()
    }

    /// Whether the downmix of `channel` is on.
    pub fn downmix_enabled(&self, channel: DownmixChannel) -> Signal<bool> {
        pick(&self.inner.downmix_enabled, &channel)
    }

    /// The downmix gain of `channel`.
    pub fn downmix_mix(&self, channel: DownmixChannel) -> Signal<f32> {
        pick(&self.inner.downmix_mix, &channel)
    }

    /// The user turned `effect` on or off (`DspToggle`, `audio_center.rs:1620`).
    pub fn set_enabled(&self, effect: DspEffect, on: bool, ctx: &mut EventContext) {
        self.enabled(effect).set(on);
        let reply = dsp::set_enabled(&self.inner.services.ctx, effect, on);
        self.watch(reply, ctx);
    }

    /// The user moved `param` to `value` (`DspValueChanged`,
    /// `audio_center.rs:1648`). The default goes through the facade's reset.
    pub fn set_value(&self, param: DspParam, value: f32, ctx: &mut EventContext) {
        self.value(param).set(value);
        let services = &self.inner.services.ctx;
        let reply = if value == param.default_value() {
            dsp::reset_value(services, param)
        } else {
            dsp::set_value(services, param, value)
        };
        self.watch(reply, ctx);
    }

    /// The user flipped the Natural / Surround switch
    /// (`StereoExpanderModeToggled`, `audio_center.rs:1694`).
    pub fn set_surround(&self, surround: bool, ctx: &mut EventContext) {
        self.inner.surround.set(surround);
        let mode = if surround {
            ExpanderMode::Surround
        } else {
            ExpanderMode::Hybrid
        };
        let reply = dsp::set_expander_mode(&self.inner.services.ctx, mode);
        self.watch(reply, ctx);
    }

    /// The user turned the downmix of `channel` on or off
    /// (`AudioStateToggle`, `audio_center.rs:1676`).
    pub fn set_downmix_enabled(&self, channel: DownmixChannel, on: bool, ctx: &mut EventContext) {
        self.downmix_enabled(channel).set(on);
        let reply = dsp::set_downmix_enabled(&self.inner.services.ctx, channel, on);
        self.watch(reply, ctx);
    }

    /// The user moved the downmix gain of `channel`
    /// (`AudioStateValueChanged`, `audio_center.rs:1685`). The default goes
    /// through the facade's reset.
    pub fn set_downmix_mix(&self, channel: DownmixChannel, value: f32, ctx: &mut EventContext) {
        self.downmix_mix(channel).set(value);
        let services = &self.inner.services.ctx;
        let reply = if value == dsp::downmix_default(channel) {
            dsp::reset_downmix_mix(services, channel)
        } else {
            dsp::set_downmix_mix(services, channel, value)
        };
        self.watch(reply, ctx);
    }

    /// Awaits the database write of a change; a failure is logged and shown
    /// as an error toast (quirk fix 8). The toast needs a window's
    /// `EventContext`; a tree without a window (a headless test) logs only.
    fn watch(&self, reply: Reply<()>, ctx: &mut EventContext) {
        if ctx.window().is_some() {
            ctx.spawn_local_with(async move { reply.recv().await }, |result, ctx| {
                if let Err(error) = result {
                    tracing::error!("the audio effect could not be saved: {error}");
                    ctx.show_toast(
                        Toast::error(tr!(ac_effects_save_failed())).body(lit!(error.to_string())),
                    );
                }
            })
            .detach();
        } else {
            self.inner
                .services
                .runtime
                .spawn_local(async move {
                    if let Err(error) = reply.recv().await {
                        tracing::error!("the audio effect could not be saved: {error}");
                    }
                })
                .detach();
        }
    }
}

/// The signal registered for `key`.
fn pick<K: PartialEq, V: Clone>(entries: &[(K, V)], key: &K) -> V {
    entries
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| unreachable!("every key is registered when the view-model is built"))
}

#[cfg(test)]
mod tests;
