//! `EqualizerViewModel`: what the equalizer tab shows and what its controls do.
//!
//! It is a `Clone` handle of signals over an [`EqState`], the Iced
//! `AudioCenterManager`'s equalizer fields (`src/gui/audio_center.rs:174-183`).
//! Each control runs the matching facade command (`commands::eq`), which
//! updates the state and the engine as the Iced handler does
//! (`audio_center.rs:1400-1450`) and persists the six equalizer keys
//! (`save_eq_settings_to_db`, `audio_center.rs:345`, called after every
//! change from `app.rs:4531-4546`); the view-model then copies the state into
//! its signals. The replies of the persistence writes are awaited on the
//! executor and a failure is logged (a slider drag is not an operation the
//! user starts, so it shows no toast).
//!
//! # What the tab binds
//!
//! - [`enabled`](EqualizerViewModel::enabled), the 20 / 31 radios'
//!   [`mode`](EqualizerViewModel::mode) (0 = 20 bands, 1 = 31) and
//!   [`bands_31`](EqualizerViewModel::bands_31);
//! - the preamp and the 31 band gains (the tab shows the first 20 or all 31);
//! - the preset name the tab shows as "Preset: X" ([`preset_name`](EqualizerViewModel::preset_name)):
//!   none while the preset is "Default" or there is none (`audio_center.rs:2813`).
//!
//! # The preset dialogs
//!
//! [`begin_load`](EqualizerViewModel::begin_load) opens the Load dialog's
//! session ([`PresetLoad`]): it captures the state for Cancel
//! (`capture`/`restore`, Iced's `EqStateBackup`, `app.rs:4488-4497` and
//! `2163-2178`), previews the clicked preset, and runs Aceptar, Cancelar,
//! Eliminar, Importar, Exportar and Predeterminado. The Save dialog calls
//! [`save_preset`](EqualizerViewModel::save_preset).
//!
//! # Failures (quirk fix 8)
//!
//! Saving, deleting, restoring, importing and exporting presets are things
//! the user started, so a failure is a log line and an error toast
//! ([`report`]). The toast needs an `EventContext` after an `await`, which only
//! `spawn_local_with` provides, and that needs a window: without one (a
//! headless tree) the failure is logged only.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::path::PathBuf;
use std::rc::Rc;

use teksilo::data::ListModel;
use teksilo::prelude::*;

use crate::frontend::commands::eq::{self, EqSnapshot, EqState, PresetEntry, PresetKind};
use crate::frontend::{AppContext, FrontendError, Reply};
use crate::logic::eq::{DEFAULT_PRESET_NAME, band_frequency_label};
use crate::teksilo_ui::app::AppServices;

/// The most bands the engine has (31-band mode); the view-model keeps a gain
/// signal for each.
pub const MAX_BANDS: usize = 31;

/// The radio value of the 20-band mode.
pub const MODE_20: usize = 0;
/// The radio value of the 31-band mode.
pub const MODE_31: usize = 1;

/// What a preset operation that failed was doing; picks the toast's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqFailure {
    /// Saving the current state as a preset.
    Save,
    /// Deleting (or hiding) a preset.
    Delete,
    /// "Predeterminado": deleting every custom preset.
    RestoreDefaults,
    /// Importing a preset file.
    Import,
    /// Exporting the current state to a file.
    Export,
    /// A file dialog that could not open or failed.
    FileDialog,
}

impl EqFailure {
    /// The error toast for `error`.
    fn toast(self, error: String) -> Toast {
        Toast::error(match self {
            Self::Save => tr!(ac_equalizer_save_failed(error = error)),
            Self::Delete => tr!(ac_equalizer_delete_failed(error = error)),
            Self::RestoreDefaults => tr!(ac_equalizer_defaults_failed(error = error)),
            Self::Import => tr!(ac_equalizer_import_failed(error = error)),
            Self::Export => tr!(ac_equalizer_export_failed(error = error)),
            Self::FileDialog => tr!(ac_equalizer_dialog_failed(error = error)),
        })
    }
}

/// Reports a failed preset operation: the log line and an error toast
/// (quirk fix 8).
pub fn report(ctx: &mut EventContext, failure: EqFailure, error: &dyn std::fmt::Display) {
    tracing::error!("equalizer preset operation {failure:?} failed: {error}");
    ctx.show_toast(failure.toast(error.to_string()));
}

struct Inner {
    services: AppServices,
    state: RefCell<EqState>,
    /// Bumped by every local change, so a `reload` that was asked earlier and
    /// answers later does not overwrite it.
    epoch: Cell<u64>,
    enabled: Signal<bool>,
    bands_31: Signal<bool>,
    mode: Signal<usize>,
    preamp: Signal<f32>,
    gains: Vec<Signal<f32>>,
    preset_name: Signal<Option<String>>,
    preset_shown: Signal<bool>,
}

/// The equalizer tab's view-model; see the module documentation.
#[derive(Clone)]
pub struct EqualizerViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for EqualizerViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.inner.state.borrow();
        f.debug_struct("EqualizerViewModel")
            .field("enabled", &state.enabled)
            .field("bands_31", &state.bands_31)
            .finish_non_exhaustive()
    }
}

/// `signal`'s value set to `value` unless it already is.
fn put<T: Clone + PartialEq + 'static>(signal: &Signal<T>, value: T) {
    if signal.get() != value {
        signal.set(value);
    }
}

impl EqualizerViewModel {
    /// The view-model over `services`, showing the equalizer as the engine has
    /// it now (read on the database worker: the signals hold the defaults
    /// until that answers, which [`reload`](Self::reload) also does). It also
    /// asks the preset list for its rows.
    pub fn new(services: &AppServices) -> Self {
        let state = EqState::default();
        let vm = Self {
            inner: Rc::new(Inner {
                services: services.clone(),
                epoch: Cell::new(0),
                enabled: Signal::new(state.enabled),
                bands_31: Signal::new(state.bands_31),
                mode: Signal::new(MODE_20),
                preamp: Signal::new(state.preamp_gain),
                gains: (0..MAX_BANDS).map(|_| Signal::new(0.0)).collect(),
                preset_name: Signal::new(None),
                preset_shown: Signal::new(false),
                state: RefCell::new(state),
            }),
        };
        vm.reload();
        services.eq_presets.reload();
        vm
    }

    // --- What the tab binds ------------------------------------------------------------------

    /// Whether the equalizer is on.
    pub fn enabled(&self) -> Signal<bool> {
        self.inner.enabled.clone()
    }

    /// Whether the 31-band set is the active one.
    pub fn bands_31(&self) -> Signal<bool> {
        self.inner.bands_31.clone()
    }

    /// The 20 / 31 radios' value ([`MODE_20`] or [`MODE_31`]). The tab calls
    /// [`set_bands_31`](Self::set_bands_31) when a radio changes it.
    pub fn mode(&self) -> Signal<usize> {
        self.inner.mode.clone()
    }

    /// The preamp gain, in dB.
    pub fn preamp(&self) -> Signal<f32> {
        self.inner.preamp.clone()
    }

    /// The gain of band `index` of the active set, in dB (0 for an index past
    /// the 31 bands).
    pub fn band_gain(&self, index: usize) -> Signal<f32> {
        self.inner
            .gains
            .get(index)
            .cloned()
            .unwrap_or_else(|| Signal::new(0.0))
    }

    /// How many bands the active set has (20 or 31).
    pub fn band_count(&self) -> usize {
        self.inner.state.borrow().band_count()
    }

    /// The label of band `index` of the active set: its frequency as Iced
    /// writes it (`logic::eq::band_frequency_label`, `audio_center.rs:2961`).
    pub fn band_label(&self, index: usize) -> String {
        let frequency = self
            .inner
            .services
            .ctx
            .player
            .eq_band(index)
            .map_or(0.0, |(frequency, _)| frequency);
        band_frequency_label(frequency)
    }

    /// The selected preset's name, or `None` when there is none or it is
    /// "Default" (`audio_center.rs:2813`): the tab shows "Preset: X" then.
    pub fn preset_name(&self) -> Signal<Option<String>> {
        self.inner.preset_name.clone()
    }

    /// Whether the "Preset: X" label is shown (the tab rebuilds its preset
    /// row when this changes: Iced puts a zero-width space there otherwise).
    pub fn preset_shown(&self) -> Signal<bool> {
        self.inner.preset_shown.clone()
    }

    /// Where the Audio Center was dragged to, or `None` while it is centred:
    /// the dialogs open relative to it.
    pub fn panel_position(&self) -> Option<(f32, f32)> {
        self.inner.services.links.audio_center.position.get()
    }

    /// A copy of the state, for tests and the dialogs.
    pub fn state(&self) -> EqState {
        self.inner.state.borrow().clone()
    }

    // --- Loading -----------------------------------------------------------------------------

    /// Reads the equalizer again from the engine and the database, and shows
    /// it. A change made meanwhile wins over the (older) answer.
    pub fn reload(&self) {
        let asked = self.inner.epoch.get();
        let reply = eq::load_state(&self.inner.services.ctx);
        let vm = self.clone();
        self.inner
            .services
            .runtime
            .spawn_local(async move {
                match reply.recv().await {
                    Ok(state) => {
                        if vm.inner.epoch.get() == asked {
                            *vm.inner.state.borrow_mut() = state;
                            vm.sync();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("the equalizer state could not be read: {error}");
                    }
                }
            })
            .detach();
    }

    /// Copies the state into the signals.
    fn sync(&self) {
        let inner = &self.inner;
        let state = inner.state.borrow();
        put(&inner.enabled, state.enabled);
        put(&inner.bands_31, state.bands_31);
        put(&inner.mode, if state.bands_31 { MODE_31 } else { MODE_20 });
        put(&inner.preamp, state.preamp_gain);
        for (signal, gain) in inner.gains.iter().zip(&state.gains) {
            put(signal, *gain);
        }
        let name = state
            .selected_preset
            .as_ref()
            .map(|preset| preset.name.clone())
            .filter(|name| name != DEFAULT_PRESET_NAME);
        put(&inner.preset_shown, name.is_some());
        put(&inner.preset_name, name);
    }

    /// Runs a command on the state, shows the result, and follows the
    /// persistence write it returns.
    fn apply(&self, command: impl FnOnce(&AppContext, &mut EqState) -> Reply<()>) {
        let reply = {
            let mut state = self.inner.state.borrow_mut();
            command(&self.inner.services.ctx, &mut state)
        };
        self.inner.epoch.set(self.inner.epoch.get() + 1);
        self.sync();
        self.follow(reply);
    }

    /// Awaits a persistence write and logs its failure.
    fn follow(&self, reply: Reply<()>) {
        self.inner
            .services
            .runtime
            .spawn_local(async move {
                if let Err(error) = reply.recv().await {
                    tracing::warn!("the equalizer settings could not be saved: {error}");
                }
            })
            .detach();
    }

    // --- The controls ------------------------------------------------------------------------

    /// "Activar Ecualizador" (`EqToggleSelected`, `audio_center.rs:1400`).
    pub fn set_enabled(&self, enabled: bool) {
        self.apply(|ctx, state| eq::set_enabled(ctx, state, enabled));
    }

    /// The 20 / 31 radios (`EqBandsSelected`, `audio_center.rs:1407`). Both
    /// band sets are kept; selecting the active mode again does nothing.
    pub fn set_bands_31(&self, bands_31: bool) {
        if self.inner.state.borrow().bands_31 == bands_31 {
            return;
        }
        self.apply(|ctx, state| eq::set_mode(ctx, state, bands_31));
    }

    /// The preamp slider (`EqPreampChanged`, `audio_center.rs:1418`): rounded
    /// to 0.1 dB, clamped to ±9 dB.
    pub fn set_preamp(&self, value: f32) {
        self.apply(|ctx, state| eq::set_preamp(ctx, state, value));
    }

    /// Band `index`'s slider (`EqBandChanged`, `audio_center.rs:1425`).
    pub fn set_band_gain(&self, index: usize, value: f32) {
        self.apply(|ctx, state| eq::set_band_gain(ctx, state, index, value));
    }

    /// The reset icon (`EqPresetIconReset`, `audio_center.rs:1443`): flat
    /// bands, preamp 0 dB, no preset.
    pub fn reset(&self) {
        self.apply(eq::reset);
    }

    // --- The presets -------------------------------------------------------------------------

    /// Opens the Load dialog's session: refreshes the preset list and
    /// captures the state Cancel restores (`app.rs:4488`).
    pub fn begin_load(&self) -> PresetLoad {
        let services = &self.inner.services;
        services.eq_presets.reload();
        let backup = eq::capture(&services.ctx, &self.inner.state.borrow());
        PresetLoad {
            inner: Rc::new(LoadInner {
                vm: self.clone(),
                backup,
                selected: Signal::new(None),
                settled: Cell::new(false),
                cancelled: Cell::new(false),
            }),
        }
    }

    /// The Save dialog's "Guardar": saves the state as the custom preset
    /// `name` (`EqPresetSaveConfirm`, `audio_center.rs:1471`). An empty name
    /// does nothing, as in Iced; a failure is reported (quirk fix 8).
    pub fn save_preset(&self, name: &str, ctx: &mut EventContext) {
        if name.is_empty() {
            return;
        }
        let reply = eq::save_preset(&self.inner.services.ctx, &self.inner.state.borrow(), name);
        let presets = self.inner.services.eq_presets.clone();
        self.spawn_reported(ctx, EqFailure::Save, async move {
            presets.replace_from(&reply.recv().await?);
            Ok(())
        });
    }

    /// Runs `task` on the executor; its failure is reported with `failure`
    /// (see the module documentation for what happens without a window).
    fn spawn_reported(
        &self,
        ctx: &mut EventContext,
        failure: EqFailure,
        task: impl Future<Output = Result<(), FrontendError>> + 'static,
    ) {
        use teksilo::prelude::EventContextAsyncExt;
        if ctx.window().is_some() {
            ctx.spawn_local_with(task, move |result, ctx| {
                if let Err(error) = result {
                    report(ctx, failure, &error);
                }
            })
            .detach();
        } else {
            self.inner
                .services
                .runtime
                .spawn_local(async move {
                    if let Err(error) = task.await {
                        tracing::error!("equalizer preset operation {failure:?} failed: {error}");
                    }
                })
                .detach();
        }
    }
}

struct LoadInner {
    vm: EqualizerViewModel,
    backup: EqSnapshot,
    selected: Signal<Option<usize>>,
    /// Whether Aceptar or Cancelar already ran, so that the dismissal that
    /// follows a button does not restore the backup over an accepted state.
    settled: Cell<bool>,
    /// Whether the session ended with Cancelar (or a dismissal): an import
    /// still in flight then must not leave its state in place.
    cancelled: Cell<bool>,
}

/// One opening of the Load dialog; see the module documentation. Clones
/// share the session.
#[derive(Clone)]
pub struct PresetLoad {
    inner: Rc<LoadInner>,
}

impl std::fmt::Debug for PresetLoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresetLoad")
            .field("selected", &self.inner.selected.get())
            .field("settled", &self.inner.settled.get())
            .finish_non_exhaustive()
    }
}

impl PresetLoad {
    /// The list's rows: Default, the custom presets, then the built-ins that
    /// were not deleted.
    pub fn entries(&self) -> ListModel<PresetEntry> {
        self.inner.vm.inner.services.eq_presets.list()
    }

    /// Bumps each time the rows were replaced (the dialog rebuilds its list).
    pub fn version(&self) -> Signal<u64> {
        self.inner.vm.inner.services.eq_presets.version()
    }

    /// The highlighted row, by index (Iced's `selected_preset_idx`).
    pub fn selected(&self) -> Signal<Option<usize>> {
        self.inner.selected.clone()
    }

    /// The number of custom rows: the divider sits after Default and these
    /// (`app.rs:5339`).
    pub fn custom_count(&self) -> usize {
        let list = self.entries();
        (0..list.len())
            .filter(|&i| {
                list.with_item(i, |entry| entry.kind == PresetKind::Custom)
                    .unwrap_or(false)
            })
            .count()
    }

    /// The name of the row at `index`.
    pub fn name_at(&self, index: usize) -> Option<String> {
        self.entries()
            .with_item(index, |entry| entry.preset.name.clone())
    }

    /// The highlighted row's name.
    pub fn selected_name(&self) -> Option<String> {
        self.inner.selected.get().and_then(|i| self.name_at(i))
    }

    /// A click on a row: highlights it and previews the preset on the engine
    /// and in the tab, without saving (`EqPresetLoadSelected`,
    /// `audio_center.rs:1451`).
    pub fn select(&self, index: usize) {
        let Some(preset) = self
            .entries()
            .with_item(index, |entry| entry.preset.clone())
        else {
            return;
        };
        self.inner.selected.set(Some(index));
        let vm = &self.inner.vm;
        {
            let mut state = vm.inner.state.borrow_mut();
            eq::preview_preset(&vm.inner.services.ctx, &mut state, &preset);
        }
        vm.inner.epoch.set(vm.inner.epoch.get() + 1);
        vm.sync();
    }

    /// Whether Aceptar or Cancelar (or a dismissal) already ended the
    /// session.
    pub fn settled(&self) -> bool {
        self.inner.settled.get()
    }

    /// "Aceptar": keeps what the previews left and saves it
    /// (`EqPresetLoadConfirm`, `audio_center.rs:1463`, persisted by
    /// `app.rs:4539`).
    pub fn accept(&self) {
        if self.inner.settled.replace(true) {
            return;
        }
        let vm = &self.inner.vm;
        let reply = eq::save(&vm.inner.services.ctx, &vm.inner.state.borrow());
        vm.follow(reply);
    }

    /// "Cancelar", Escape or a click outside: puts the captured state back
    /// on the engine (both band sets: quirk fix 20) and in the tab, without
    /// saving (`CloseDialog`, `app.rs:2163`).
    pub fn cancel(&self) {
        if self.inner.settled.replace(true) {
            return;
        }
        self.inner.cancelled.set(true);
        self.restore_backup();
    }

    /// Puts the backup back in the engine, the state and the database.
    fn restore_backup(&self) {
        let vm = &self.inner.vm;
        {
            let mut state = vm.inner.state.borrow_mut();
            eq::restore(&vm.inner.services.ctx, &mut state, &self.inner.backup);
            // Quirk fix 60: an "Importar" during the dialog was persisted at
            // once; save the restored state too, so the database matches what
            // the engine and the tab show (Iced restores without saving,
            // `app.rs:2163-2178`). The worker writes it in order; nothing waits.
            drop(eq::save(&vm.inner.services.ctx, &state));
        }
        vm.inner.epoch.set(vm.inner.epoch.get() + 1);
        vm.sync();
    }

    /// "Eliminar": deletes the highlighted preset (a built-in is hidden for
    /// good: quirk fix 14). Nothing is highlighted: nothing happens, as in
    /// Iced (`app.rs:5469`). The highlight stays on its row index.
    pub fn delete_selected(&self, ctx: &mut EventContext) {
        let Some(name) = self.selected_name() else {
            return;
        };
        let vm = &self.inner.vm;
        let reply = eq::delete_preset(&vm.inner.services.ctx, &vm.inner.state.borrow(), &name);
        let presets = vm.inner.services.eq_presets.clone();
        vm.spawn_reported(ctx, EqFailure::Delete, async move {
            presets.replace_from(&reply.recv().await?);
            Ok(())
        });
    }

    /// "Predeterminado": deletes every custom preset and shows the built-ins
    /// again (`EqPresetRestoreDefaults`, `audio_center.rs:1527`).
    pub fn restore_defaults(&self, ctx: &mut EventContext) {
        let vm = &self.inner.vm;
        let reply = eq::restore_default_presets(&vm.inner.services.ctx);
        let presets = vm.inner.services.eq_presets.clone();
        vm.spawn_reported(ctx, EqFailure::RestoreDefaults, async move {
            presets.replace_from(&reply.recv().await?);
            Ok(())
        });
    }

    /// "Importar", once the file is picked: saves the file's preset, applies
    /// it at once and persists (`EqPresetFileImported`,
    /// `audio_center.rs:1542`). Cancelling the dialog afterwards still puts
    /// the captured state back, as in Iced.
    pub fn import(&self, path: PathBuf, ctx: &mut EventContext) {
        let vm = &self.inner.vm;
        let reply = eq::import_preset(&vm.inner.services.ctx, &vm.inner.state.borrow(), path);
        let owner = vm.clone();
        let session = self.clone();
        vm.spawn_reported(ctx, EqFailure::Import, async move {
            let imported = reply.recv().await?;
            // The imported preset joins the list either way, as in Iced.
            owner
                .inner
                .services
                .eq_presets
                .replace_from(&imported.presets);
            if session.inner.cancelled.get() {
                // Cancelar came before the import finished: the facade has
                // applied and saved the import since, so restore the backup
                // again rather than show the imported state (quirk fix 60).
                session.restore_backup();
            } else {
                *owner.inner.state.borrow_mut() = imported.state;
                owner.inner.epoch.set(owner.inner.epoch.get() + 1);
                owner.sync();
            }
            Ok(())
        });
    }

    /// "Exportar", once the file is chosen: writes the current state, named
    /// after the file stem, with both band sets (`EqPresetFileExported`,
    /// `audio_center.rs:1576`).
    pub fn export(&self, path: PathBuf, ctx: &mut EventContext) {
        let vm = &self.inner.vm;
        let reply = eq::export_preset(&vm.inner.services.ctx, &vm.inner.state.borrow(), path);
        vm.spawn_reported(ctx, EqFailure::Export, async move { reply.recv().await });
    }
}

#[cfg(test)]
pub(crate) mod tests;
