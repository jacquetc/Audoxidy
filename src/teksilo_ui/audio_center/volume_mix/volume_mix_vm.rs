//! [`VolumeMixViewModel`]: the state of the volume-and-mix tab and what its
//! controls do.
//!
//! Port of the `Volumen*` and `Crossfade*` handlers of `AudioCenterManager`
//! (`src/gui/audio_center.rs:1706-1810`) and of the locking rules of the view
//! (`:3708-4600`):
//!
//! - a **group** has a master switch (volume fades, silence removal, the fixed
//!   pre-amp, ReplayGain, crossfade) that is always available;
//! - what is in a group is **locked** while its master is off: its switch is
//!   drawn unchecked (whatever it holds, which is kept for when the master
//!   comes back on) and ignores clicks on it and on its label, and its value
//!   is disabled and dimmed;
//! - a value that has a switch of its own (fade in and out, the three
//!   ReplayGain offsets, the two crossfades) is also disabled while that
//!   switch is off; the silence lengths and the pre-amp level depend on the
//!   master only;
//! - a label is lit (primary) when the control it names is active and dim
//!   (secondary) otherwise;
//! - every change is applied to the engine and stored by the facade
//!   ([`mix::set_toggle`], [`mix::set_value`]): `vol_*` keys for the first
//!   four groups, `crossfade_*` for the last, so there is nothing to save when
//!   the Audio Center closes. A value is clamped to its stepper's range; a
//!   secondary click on a stepper resets it to Iced's value
//!   ([`MixValue::reset_value`]).
//!
//! The tab shows the engine's values (quirk fix 21): they are read when the
//! view-model is built and every time the tab is shown, so a fresh install
//! shows the fade lengths the engine really uses.

use std::rc::Rc;

use teksilo::prelude::{Signal, tr};

use crate::frontend::AppContext;
use crate::frontend::commands::mix::{self, MixToggle, MixValue};
use crate::teksilo_ui::app::services::{AppServices, AudioCenterLink, audio_center_tab};
use crate::teksilo_ui::models::load::finish_reply;
use teksilo::prelude::AsyncRuntimeHandle;

/// Every switch of the tab, group by group.
pub const TOGGLES: [MixToggle; 14] = [
    MixToggle::Fades,
    MixToggle::SmoothVolume,
    MixToggle::FadeIn,
    MixToggle::FadeOut,
    MixToggle::Silence,
    MixToggle::SilenceEdgeTrim,
    MixToggle::RgFixed,
    MixToggle::RgMaster,
    MixToggle::RgAlbum,
    MixToggle::RgTrack,
    MixToggle::RgAnalyzeRt,
    MixToggle::Crossfade,
    MixToggle::CrossfadeManual,
    MixToggle::CrossfadeAuto,
];

/// The master switch that locks `toggle`, or `None` for a master switch.
pub fn master_of(toggle: MixToggle) -> Option<MixToggle> {
    match toggle {
        MixToggle::SmoothVolume | MixToggle::FadeIn | MixToggle::FadeOut => Some(MixToggle::Fades),
        MixToggle::SilenceEdgeTrim => Some(MixToggle::Silence),
        MixToggle::RgAlbum | MixToggle::RgTrack | MixToggle::RgAnalyzeRt => {
            Some(MixToggle::RgMaster)
        }
        MixToggle::CrossfadeManual | MixToggle::CrossfadeAuto => Some(MixToggle::Crossfade),
        MixToggle::Fades
        | MixToggle::Silence
        | MixToggle::RgFixed
        | MixToggle::RgMaster
        | MixToggle::Crossfade => None,
    }
}

/// What enables a stepper: its group's master switch, and the switch of its
/// own when it has one.
pub fn gate_of(value: MixValue) -> (MixToggle, Option<MixToggle>) {
    match value {
        MixValue::FadeInMs => (MixToggle::Fades, Some(MixToggle::FadeIn)),
        MixValue::FadeOutMs => (MixToggle::Fades, Some(MixToggle::FadeOut)),
        MixValue::SilenceDurationMs | MixValue::SilenceThresholdDb => (MixToggle::Silence, None),
        MixValue::RgFixedDb => (MixToggle::RgFixed, None),
        MixValue::RgOffsetAlbumDb => (MixToggle::RgMaster, Some(MixToggle::RgAlbum)),
        MixValue::RgOffsetTrackDb => (MixToggle::RgMaster, Some(MixToggle::RgTrack)),
        MixValue::RgOffsetRtDb => (MixToggle::RgMaster, Some(MixToggle::RgAnalyzeRt)),
        MixValue::CrossfadeManualMs => (MixToggle::Crossfade, Some(MixToggle::CrossfadeManual)),
        MixValue::CrossfadeAutoMs => (MixToggle::Crossfade, Some(MixToggle::CrossfadeAuto)),
    }
}

/// The place of `toggle` in [`TOGGLES`].
pub fn toggle_index(toggle: MixToggle) -> usize {
    match toggle {
        MixToggle::Fades => 0,
        MixToggle::SmoothVolume => 1,
        MixToggle::FadeIn => 2,
        MixToggle::FadeOut => 3,
        MixToggle::Silence => 4,
        MixToggle::SilenceEdgeTrim => 5,
        MixToggle::RgFixed => 6,
        MixToggle::RgMaster => 7,
        MixToggle::RgAlbum => 8,
        MixToggle::RgTrack => 9,
        MixToggle::RgAnalyzeRt => 10,
        MixToggle::Crossfade => 11,
        MixToggle::CrossfadeManual => 12,
        MixToggle::CrossfadeAuto => 13,
    }
}

/// The place of `value` in [`MixValue::ALL`].
pub fn value_index(value: MixValue) -> usize {
    match value {
        MixValue::FadeInMs => 0,
        MixValue::FadeOutMs => 1,
        MixValue::SilenceDurationMs => 2,
        MixValue::SilenceThresholdDb => 3,
        MixValue::RgFixedDb => 4,
        MixValue::RgOffsetAlbumDb => 5,
        MixValue::RgOffsetTrackDb => 6,
        MixValue::RgOffsetRtDb => 7,
        MixValue::CrossfadeManualMs => 8,
        MixValue::CrossfadeAutoMs => 9,
    }
}

struct ToggleState {
    toggle: MixToggle,
    /// What the engine holds.
    actual: Signal<bool>,
    /// What the check box shows: `actual` while unlocked, unchecked while
    /// locked. The check box writes it when clicked.
    shown: Signal<bool>,
}

struct ValueState {
    value: MixValue,
    signal: Signal<f64>,
}

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    link: AudioCenterLink,
    toggles: [ToggleState; 14],
    values: [ValueState; 10],
}

/// The volume-and-mix tab's view-model. UI thread only; clones share one
/// state.
#[derive(Clone)]
pub struct VolumeMixViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for VolumeMixViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolumeMixViewModel")
            .field(
                "toggles",
                &self
                    .inner
                    .toggles
                    .iter()
                    .map(|t| (t.toggle, t.actual.get()))
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl VolumeMixViewModel {
    /// The tab's state, read from the engine.
    pub fn new(services: &AppServices) -> Self {
        let state = services.ctx.player.snapshot();
        let vm = Self {
            inner: Rc::new(Inner {
                ctx: services.ctx.clone(),
                runtime: services.runtime.clone(),
                link: services.links.audio_center.clone(),
                toggles: TOGGLES.map(|toggle| ToggleState {
                    toggle,
                    actual: Signal::new(toggle.get(&state)),
                    shown: Signal::new(false),
                }),
                values: MixValue::ALL.map(|value| ValueState {
                    value,
                    signal: Signal::new(value.get(&state)),
                }),
            }),
        };
        vm.refresh_shown();
        vm
    }

    /// Whether the tab is on screen: the Audio Center is open on it.
    pub fn shown(&self) -> Signal<bool> {
        self.inner
            .link
            .open
            .zip(&self.inner.link.tab)
            .map(|(open, tab)| *open && *tab == audio_center_tab::VOLUME_MIX)
    }

    /// The tab appeared: reads the engine's values again (they cannot change
    /// behind the tab's back, but the startup restore may have run since the
    /// view-model was built).
    pub fn on_shown(&self) {
        let state = self.inner.ctx.player.snapshot();
        for entry in &self.inner.toggles {
            entry.actual.set_if_changed(entry.toggle.get(&state));
        }
        for entry in &self.inner.values {
            entry.signal.set_if_changed(entry.value.get(&state));
        }
        self.refresh_shown();
    }

    fn toggle_entry(&self, toggle: MixToggle) -> &ToggleState {
        &self.inner.toggles[toggle_index(toggle)]
    }

    fn value_entry(&self, value: MixValue) -> &ValueState {
        &self.inner.values[value_index(value)]
    }

    /// What the engine holds for `toggle`.
    pub fn is_on(&self, toggle: MixToggle) -> bool {
        self.toggle_entry(toggle).actual.get()
    }

    /// Whether `toggle` can be clicked: a master switch always, anything else
    /// while its master is on.
    pub fn is_unlocked(&self, toggle: MixToggle) -> bool {
        master_of(toggle).is_none_or(|master| self.is_on(master))
    }

    /// The signal behind `toggle`'s check box: what it shows, and what the
    /// check box writes when clicked.
    pub fn checked(&self, toggle: MixToggle) -> Signal<bool> {
        self.toggle_entry(toggle).shown.clone()
    }

    /// Whether `toggle`'s check box and label take clicks.
    pub fn unlocked(&self, toggle: MixToggle) -> Signal<bool> {
        match master_of(toggle) {
            None => Signal::new(true),
            Some(master) => self.toggle_entry(master).actual.clone(),
        }
    }

    /// Whether `toggle`'s label is lit: the switch is unlocked and on.
    pub fn lit(&self, toggle: MixToggle) -> Signal<bool> {
        let own = self.toggle_entry(toggle).actual.clone();
        match master_of(toggle) {
            None => own,
            Some(master) => self
                .toggle_entry(master)
                .actual
                .zip(&own)
                .map(|(master, own)| *master && *own),
        }
    }

    /// The signal behind `value`'s stepper. The stepper writes it, then calls
    /// [`value_changed`](Self::value_changed).
    pub fn value(&self, value: MixValue) -> Signal<f64> {
        self.value_entry(value).signal.clone()
    }

    /// Whether `value`'s stepper takes input: its group's master is on and so
    /// is its own switch, if it has one.
    pub fn value_enabled(&self, value: MixValue) -> Signal<bool> {
        let (master, own) = gate_of(value);
        let master = self.toggle_entry(master).actual.clone();
        match own {
            None => master,
            Some(own) => master
                .zip(&self.toggle_entry(own).actual)
                .map(|(master, own)| *master && *own),
        }
    }

    /// Whether `value`'s label is lit: the stepper is enabled.
    pub fn value_lit(&self, value: MixValue) -> Signal<bool> {
        self.value_enabled(value)
    }

    fn refresh_shown(&self) {
        for entry in &self.inner.toggles {
            entry
                .shown
                .set_if_changed(entry.actual.get() && self.is_unlocked(entry.toggle));
        }
    }

    fn persist(&self, what: &'static str, reply: crate::frontend::Reply<()>) {
        finish_reply(&self.inner.runtime, what, reply, |_| {});
    }

    // ---- the user's actions ----

    /// Turns `toggle` on or off and stores it. A locked switch ignores it
    /// (Iced draws such a check box without a handler). Returns whether it
    /// was applied.
    pub fn set_toggle(&self, toggle: MixToggle, on: bool) -> bool {
        if !self.is_unlocked(toggle) {
            self.refresh_shown();
            return false;
        }
        self.toggle_entry(toggle).actual.set_if_changed(on);
        self.refresh_shown();
        self.persist(
            "the mix settings",
            mix::set_toggle(&self.inner.ctx, toggle, on),
        );
        true
    }

    /// A click on `toggle`'s label (UAT round 10): flips an unlocked switch.
    pub fn flip(&self, toggle: MixToggle) -> bool {
        self.set_toggle(toggle, !self.is_on(toggle))
    }

    /// The stepper of `value` produced `to`: stores it, clamped to the
    /// stepper's range.
    pub fn value_changed(&self, value: MixValue, to: f64) {
        let range = value.range();
        let to = to.clamp(*range.start(), *range.end());
        self.value_entry(value).signal.set_if_changed(to);
        self.persist(
            "the mix settings",
            mix::set_value(&self.inner.ctx, value, to),
        );
    }

    /// Resets `value` to Iced's value (a secondary click on its stepper) and
    /// stores it.
    pub fn reset_value(&self, value: MixValue) {
        self.value_entry(value)
            .signal
            .set_if_changed(value.reset_value());
        self.persist("the mix settings", mix::reset_value(&self.inner.ctx, value));
    }
}

/// The accessible name of `value`'s stepper.
pub fn value_name(value: MixValue) -> teksilo::prelude::LocalizedString {
    match value {
        MixValue::FadeInMs => tr!(ac_volume_mix_fade_in_value()),
        MixValue::FadeOutMs => tr!(ac_volume_mix_fade_out_value()),
        MixValue::SilenceDurationMs => tr!(ac_volume_mix_silence_duration_value()),
        MixValue::SilenceThresholdDb => tr!(ac_volume_mix_silence_threshold_value()),
        MixValue::RgFixedDb => tr!(ac_volume_mix_preamp_value()),
        MixValue::RgOffsetAlbumDb => tr!(ac_volume_mix_rg_album_value()),
        MixValue::RgOffsetTrackDb => tr!(ac_volume_mix_rg_track_value()),
        MixValue::RgOffsetRtDb => tr!(ac_volume_mix_rg_untagged_value()),
        MixValue::CrossfadeManualMs => tr!(ac_volume_mix_crossfade_manual_value()),
        MixValue::CrossfadeAutoMs => tr!(ac_volume_mix_crossfade_auto_value()),
    }
}

#[cfg(test)]
mod tests;
