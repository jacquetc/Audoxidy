//! The equalizer: its controls, its presets, and the Load dialog's preview.
//!
//! The view-model owns an [`EqState`], the Iced `AudioCenterManager`'s EQ
//! fields: on/off, mode, preamp, the active band gains and the selected
//! preset. Each control updates the state and the engine the way the Iced
//! handler does (`src/gui/audio_center.rs`, `AudioCenterManager::update`), then
//! persists the six EQ keys as `save_eq_settings_to_db` does, through
//! coalesced writes. The engine calls are quick lock operations, made on the
//! caller's thread; the database work runs on the database worker.
//!
//! Three Teksilo-only changes (see `docs/teksilo-port-quirk-fixes.md`):
//!
//! - both `eq_band_gains_20` and `eq_band_gains_31` are written, the inactive
//!   set converted from the active one with `EqPreset`'s log-frequency
//!   interpolation (quirk fix 18, Iced defect D17);
//! - deleting a built-in preset hides it persistently, under
//!   `eq_hidden_builtin_presets` (quirk fix 14);
//! - cancelling a preset preview restores both band sets, not only the active
//!   one (quirk fix 20).

#![warn(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::preset::{EqPreset, preset_from_json, preset_to_json};
use crate::db::Database;
use crate::frontend::app_context::AppContext;
use crate::frontend::db_worker::DbWorker;
use crate::frontend::error::FrontendError;
use crate::frontend::ports::PlayerPort;
use crate::frontend::reply::Reply;
use crate::frontend::settings::{self, EqSettings, keys};
use crate::logic::eq as rules;

/// The equalizer as the Audio Center shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct EqState {
    /// The equalizer is on.
    pub enabled: bool,
    /// 31-band mode (else 20 bands).
    pub bands_31: bool,
    /// The preamp gain, in dB. While the equalizer is off, a change is kept
    /// here and reaches the engine when it is turned on, as in Iced.
    pub preamp_gain: f32,
    /// The gains of the active band set, in dB.
    pub gains: Vec<f32>,
    /// The selected preset, if any.
    pub selected_preset: Option<EqPreset>,
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            enabled: false,
            bands_31: false,
            preamp_gain: 0.0,
            gains: vec![0.0; 20],
            selected_preset: None,
        }
    }
}

impl EqState {
    /// The number of bands of the active set (20 or 31).
    pub fn band_count(&self) -> usize {
        band_count(self.bands_31)
    }

    /// Both band sets as persisted: the active gains, and the other set
    /// converted from them with `EqPreset`'s interpolation. Returns
    /// `(gains_20, gains_31)`.
    pub fn band_sets(&self) -> (Vec<f32>, Vec<f32>) {
        if self.bands_31 {
            (EqPreset::convert_31_to_20(&self.gains), self.gains.clone())
        } else {
            (self.gains.clone(), EqPreset::convert_20_to_31(&self.gains))
        }
    }

    /// The six EQ keys for this state (quirk fix 18: both band sets).
    pub fn to_settings(&self) -> EqSettings {
        let (band_gains_20, band_gains_31) = self.band_sets();
        EqSettings {
            enabled: self.enabled,
            bands_31: self.bands_31,
            preamp_gain: self.preamp_gain,
            band_gains_20,
            band_gains_31,
            selected_preset: self.selected_preset.as_ref().map(|p| p.name.clone()),
        }
    }

    /// The state as a preset called `name`, with both band sets, as Iced
    /// builds it to save or export.
    pub fn to_preset(&self, name: &str) -> EqPreset {
        let (gains_20, gains_31) = self.band_sets();
        EqPreset::new(name, self.preamp_gain, Some(gains_20), Some(gains_31))
    }
}

fn band_count(bands_31: bool) -> usize {
    if bands_31 { 31 } else { 20 }
}

/// The gains of the engine's active band set.
fn active_gains(player: &dyn PlayerPort, count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| player.eq_band(i).map_or(0.0, |(_, gain)| gain))
        .collect()
}

/// The engine's two band sets, `(gains_20, gains_31)`, read without changing
/// the live equalizer (the inactive set is read from a copy switched to it).
fn engine_band_sets(player: &dyn PlayerPort) -> (Vec<f32>, Vec<f32>) {
    let mut sets = (Vec::new(), Vec::new());
    player.read_dsp(&mut |dsp| {
        let gains = |eq: &crate::audio::dsp::Equalizer| -> Vec<f32> {
            eq.bands.iter().map(|band| band.gain).collect()
        };
        let mut other = dsp.equalizer.clone();
        if dsp.equalizer.bands.len() == 31 {
            other.set_mode(20);
            sets = (gains(&other), gains(&dsp.equalizer));
        } else {
            other.set_mode(31);
            sets = (gains(&dsp.equalizer), gains(&other));
        }
    });
    sets
}

/// A preset by name: the custom presets first, then the built-ins.
fn find_preset(name: &str, customs: &[EqPreset]) -> Option<EqPreset> {
    customs
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .or_else(|| {
            EqPreset::default_presets()
                .into_iter()
                .find(|p| p.name == name)
        })
}

/// The equalizer as the engine has it, with the persisted selected preset
/// (looked up among the custom presets, then the built-ins, as the Iced
/// startup does). Read on the database worker.
pub fn load_state(ctx: &AppContext) -> Reply<EqState> {
    let player = Arc::clone(&ctx.player);
    ctx.db.read(move |db| {
        let selected = EqSettings::read(db).selected_preset;
        let customs = db.load_eq_presets()?;
        let count = player.eq_band_count();
        Ok(EqState {
            enabled: player.eq_enabled(),
            bands_31: count == 31,
            preamp_gain: player.preamp_gain(),
            gains: active_gains(&*player, count),
            selected_preset: selected.and_then(|name| find_preset(&name, &customs)),
        })
    })
}

/// Persists the six EQ keys of `state` (`save_eq_settings_to_db`, with both
/// band sets converted: quirk fix 18).
pub fn save(ctx: &AppContext, state: &EqState) -> Reply<()> {
    persist(&ctx.db, state)
}

fn persist(db: &DbWorker, state: &EqState) -> Reply<()> {
    settings::write_entries(db, state.to_settings().entries())
}

/// Turns the equalizer on or off. Turning it on also pushes the state's
/// preamp to the engine, as Iced does. Persists.
pub fn set_enabled(ctx: &AppContext, state: &mut EqState, enabled: bool) -> Reply<()> {
    state.enabled = enabled;
    ctx.player.set_eq_enabled(enabled);
    if enabled {
        ctx.player.set_preamp_gain(state.preamp_gain);
    }
    save(ctx, state)
}

/// Switches between 20 and 31 bands. The engine swaps to its other band set;
/// the state's gains are re-read from it. Persists.
pub fn set_mode(ctx: &AppContext, state: &mut EqState, bands_31: bool) -> Reply<()> {
    state.bands_31 = bands_31;
    ctx.player.set_eq_mode(band_count(bands_31));
    state.gains = active_gains(&*ctx.player, band_count(bands_31));
    save(ctx, state)
}

/// Sets the preamp from its slider: rounded to 0.1 dB and clamped to ±9 dB.
/// It reaches the engine only while the equalizer is on, as in Iced.
/// Persists.
pub fn set_preamp(ctx: &AppContext, state: &mut EqState, value: f32) -> Reply<()> {
    let gain = rules::slider_gain(value);
    state.preamp_gain = gain;
    if state.enabled {
        ctx.player.set_preamp_gain(gain);
    }
    save(ctx, state)
}

/// Sets band `index` of the active set from its slider: rounded to 0.1 dB and
/// clamped to ±9 dB. An index past the active set is ignored, as in Iced.
/// Persists.
pub fn set_band_gain(ctx: &AppContext, state: &mut EqState, index: usize, value: f32) -> Reply<()> {
    let gain = rules::slider_gain(value);
    if let Some(slot) = state.gains.get_mut(index) {
        *slot = gain;
        ctx.player.set_eq_band_gain(index, gain);
    }
    save(ctx, state)
}

/// Applies `preset` without persisting it: the Load dialog's preview of a
/// clicked preset (Iced `EqPresetLoadSelected`). The engine takes the preset's
/// preamp and both band sets; the state takes the preset, its preamp and its
/// active gains rounded to 0.1 dB. Confirm with [`save`], or undo with
/// [`restore`].
pub fn preview_preset(ctx: &AppContext, state: &mut EqState, preset: &EqPreset) {
    apply_preset_to(&*ctx.player, state, preset);
}

/// [`preview_preset`] against a player (also used off the caller's thread).
fn apply_preset_to(player: &dyn PlayerPort, state: &mut EqState, preset: &EqPreset) {
    state.selected_preset = Some(preset.clone());
    state.preamp_gain = preset.preamp_gain;
    player.apply_eq_preset(preset);
    let gains = if state.bands_31 {
        preset.get_gains_31()
    } else {
        preset.get_gains_20()
    };
    state.gains = rules::rounded_gains(&gains);
}

/// Applies `preset` ([`preview_preset`]) and persists the result.
pub fn apply_preset(ctx: &AppContext, state: &mut EqState, preset: &EqPreset) -> Reply<()> {
    preview_preset(ctx, state, preset);
    save(ctx, state)
}

/// Resets the equalizer: flat bands (both sets), preamp 0 dB, no preset
/// (Iced `EqPresetIconReset`). Persists.
pub fn reset(ctx: &AppContext, state: &mut EqState) -> Reply<()> {
    ctx.player.reset_dsp_defaults();
    state.selected_preset = None;
    state.preamp_gain = 0.0;
    state.gains = vec![0.0; state.band_count()];
    save(ctx, state)
}

/// What the Load dialog restores on Cancel.
#[derive(Debug, Clone, PartialEq)]
pub struct EqSnapshot {
    /// The state's preamp gain.
    pub preamp_gain: f32,
    /// The engine's 20-band set.
    pub gains_20: Vec<f32>,
    /// The engine's 31-band set.
    pub gains_31: Vec<f32>,
    /// The selected preset.
    pub selected_preset: Option<EqPreset>,
}

/// Captures what a preview can change, before the Load dialog opens. Both
/// band sets are taken from the engine (Iced keeps only the active one, so a
/// cancelled preview left the other set changed).
pub fn capture(ctx: &AppContext, state: &EqState) -> EqSnapshot {
    let (gains_20, gains_31) = engine_band_sets(&*ctx.player);
    EqSnapshot {
        preamp_gain: state.preamp_gain,
        gains_20,
        gains_31,
        selected_preset: state.selected_preset.clone(),
    }
}

/// Puts a [`capture`]d snapshot back in the engine and the state, without
/// persisting (the Load dialog's Cancel). The preamp reaches the engine even
/// while the equalizer is off, as in Iced, since a preview set it there too.
pub fn restore(ctx: &AppContext, state: &mut EqState, snapshot: &EqSnapshot) {
    ctx.player.set_preamp_gain(snapshot.preamp_gain);
    ctx.player
        .apply_eq_preset_gains(&snapshot.gains_20, &snapshot.gains_31);
    state.preamp_gain = snapshot.preamp_gain;
    state.selected_preset = snapshot.selected_preset.clone();
    state.gains = if state.bands_31 {
        snapshot.gains_31.clone()
    } else {
        snapshot.gains_20.clone()
    };
}

/// Where a preset comes from, for the Load dialog's dividers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    /// The built-in flat preset, always first.
    Default,
    /// A preset the user saved or imported.
    Custom,
    /// A built-in preset.
    Builtin,
}

/// One row of the Load dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetEntry {
    /// The preset.
    pub preset: EqPreset,
    /// Where it comes from.
    pub kind: PresetKind,
}

/// The presets: built-in, custom (from the database), and the hidden
/// built-ins.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetList {
    /// The built-in presets (`EqPreset::default_presets`).
    pub builtins: Vec<EqPreset>,
    /// The custom presets, as stored.
    pub customs: Vec<EqPreset>,
    /// The built-ins the user deleted (quirk fix 14).
    pub hidden_builtins: Vec<String>,
}

impl PresetList {
    /// Reads the custom and hidden presets.
    pub fn read(db: &Database) -> Result<Self, FrontendError> {
        Ok(Self {
            builtins: EqPreset::default_presets(),
            customs: db.load_eq_presets()?,
            hidden_builtins: settings::read_hidden_builtin_presets(db),
        })
    }

    /// The Load dialog's list, in `logic::eq::unified_presets` order: Default,
    /// the custom presets A–Z, the visible built-ins A–Z.
    pub fn unified(&self) -> Vec<PresetEntry> {
        rules::unified_presets(&self.builtins, &self.customs, &self.hidden_builtins)
            .into_iter()
            .map(|preset| {
                let kind = if self.customs.iter().any(|c| std::ptr::eq(c, preset)) {
                    PresetKind::Custom
                } else if preset.name == rules::DEFAULT_PRESET_NAME {
                    PresetKind::Default
                } else {
                    PresetKind::Builtin
                };
                PresetEntry {
                    preset: preset.clone(),
                    kind,
                }
            })
            .collect()
    }
}

/// The presets, read on the database worker.
pub fn presets(ctx: &AppContext) -> Reply<PresetList> {
    ctx.db.read(PresetList::read)
}

/// Queues `f` after `earlier` (a persistence reply queued before it), and
/// fails with `earlier`'s error if it failed.
fn after_persist<T, F>(ctx: &AppContext, earlier: Reply<()>, f: F) -> Reply<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Database) -> Result<T, FrontendError> + Send + 'static,
{
    ctx.db.write(move |db| {
        // `earlier` was queued before this job, so it has resolved.
        if let Some(Err(e)) = earlier.try_recv() {
            return Err(e);
        }
        f(db)
    })
}

/// Saves the state as a custom preset called `name` (Iced
/// `EqPresetSaveConfirm`): the active gains and the other set converted from
/// them. Like Iced, it also persists the EQ keys and does not select the new
/// preset. An empty name is refused. Replies with the updated presets.
pub fn save_preset(ctx: &AppContext, state: &EqState, name: &str) -> Reply<PresetList> {
    if name.is_empty() {
        return Reply::ready(Err(FrontendError::InvalidInput(
            "a preset needs a name".to_string(),
        )));
    }
    let preset = state.to_preset(name);
    let persisted = save(ctx, state);
    after_persist(ctx, persisted, move |db| {
        db.save_eq_preset(&preset)?;
        PresetList::read(db)
    })
}

/// Deletes the preset called `name` (Iced `EqPresetDelete`): a built-in with
/// no custom preset of that name is hidden, and the hidden list is persisted
/// (quirk fix 14); anything else is deleted from the custom presets. Like Iced,
/// it also persists the EQ keys and keeps the selection. Replies with the
/// updated presets.
pub fn delete_preset(ctx: &AppContext, state: &EqState, name: &str) -> Reply<PresetList> {
    let name = name.to_string();
    let persisted = save(ctx, state);
    after_persist(ctx, persisted, move |db| {
        let mut list = PresetList::read(db)?;
        match rules::preset_delete_action(&name, &list.builtins, &list.customs) {
            rules::PresetDeleteAction::HideBuiltin => {
                rules::hide_builtin(&mut list.hidden_builtins, &name);
                db.set_setting(
                    keys::EQ_HIDDEN_BUILTIN_PRESETS,
                    &settings::encode_hidden_builtin_presets(&list.hidden_builtins),
                )?;
            }
            rules::PresetDeleteAction::DeleteCustom => {
                db.delete_eq_preset(&name)?;
            }
        }
        PresetList::read(db)
    })
}

/// Deletes every custom preset and shows every built-in again ("Predeterminado",
/// Iced `EqPresetRestoreDefaults`). Replies with the updated presets.
pub fn restore_default_presets(ctx: &AppContext) -> Reply<PresetList> {
    ctx.db.write(|db| {
        db.clear_eq_presets()?;
        db.set_setting(
            keys::EQ_HIDDEN_BUILTIN_PRESETS,
            &settings::encode_hidden_builtin_presets(&[]),
        )?;
        PresetList::read(db)
    })
}

/// The result of [`import_preset`].
#[derive(Debug, Clone, PartialEq)]
pub struct EqImport {
    /// The imported preset, as saved.
    pub preset: EqPreset,
    /// The state with the preset applied; the view-model adopts it.
    pub state: EqState,
    /// The updated presets.
    pub presets: PresetList,
}

/// Imports a preset file (Iced `EqPresetFileImported`): reads and parses it
/// with `preset_from_json` (one band set is enough, the other is converted),
/// saves it as a custom preset, applies it to the engine and persists the EQ
/// keys. The file is read on the database worker, not on the caller's thread.
///
/// A file that cannot be read, is not UTF-8 or is not a preset is an error the
/// UI can show (quirk fix 8); Iced only logs it.
pub fn import_preset(ctx: &AppContext, state: &EqState, path: PathBuf) -> Reply<EqImport> {
    let mut state = state.clone();
    let player = Arc::clone(&ctx.player);
    let worker = ctx.db.clone();
    ctx.db.write(move |db| {
        let bytes = std::fs::read(&path)?;
        let text = String::from_utf8(bytes).map_err(|_| {
            FrontendError::InvalidInput(format!("{} is not a text file", path.display()))
        })?;
        let preset = preset_from_json(&text).map_err(|e| {
            tracing::error!(
                "error parsing the EQ preset JSON from {}: {e}",
                path.display()
            );
            FrontendError::InvalidInput(format!("{} is not an EQ preset: {e}", path.display()))
        })?;
        db.save_eq_preset(&preset)?;
        apply_preset_to(&*player, &mut state, &preset);
        // Queued behind this job; nothing waits for it here (waiting on the
        // database worker from one of its own jobs would never return).
        let _persisted = persist(&worker, &state);
        Ok(EqImport {
            presets: PresetList::read(db)?,
            preset,
            state,
        })
    })
}

/// Exports the state to `path` as a preset named after the file stem (Iced
/// `EqPresetFileExported`), with both band sets. The file is written on the
/// database worker; a write failure is an error the UI can show (quirk fix 8).
pub fn export_preset(ctx: &AppContext, state: &EqState, path: PathBuf) -> Reply<()> {
    let preset = state.to_preset(&rules::export_preset_name(&path));
    ctx.db.write(move |_| {
        let json = preset_to_json(&preset).map_err(|e| {
            FrontendError::InvalidInput(format!("the preset cannot be encoded: {e}"))
        })?;
        std::fs::write(&path, json).map_err(|e| {
            tracing::error!(
                "the EQ preset could not be written to {}: {e}",
                path.display()
            );
            FrontendError::from(e)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
    use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn context() -> (AppContext, InMemoryFakes) {
        AppContext::in_memory_with_fakes().unwrap()
    }

    fn raw(ctx: &AppContext, key: &'static str) -> Option<String> {
        ctx.db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    fn twenty_band_state(ctx: &AppContext) -> EqState {
        let mut state = load_state(ctx).recv_blocking().unwrap();
        set_mode(ctx, &mut state, false).recv_blocking().unwrap();
        state
    }

    fn ramp(len: usize) -> Vec<f32> {
        (0..len).map(|i| i as f32).collect()
    }

    fn temp_path(name: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "audoxidy-eq-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn the_state_loads_from_the_engine_and_the_saved_preset() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            ctx.db
                .write(|db| {
                    db.set_setting(keys::EQ_SELECTED_PRESET, "Rock")?;
                    Ok(())
                })
                .recv_blocking()
                .unwrap();
            fakes.player.set_eq_enabled(true);
            fakes.player.set_eq_band_gain(2, 4.5);
            let state = load_state(&ctx).recv_blocking().unwrap();
            assert!(state.enabled);
            assert!(state.bands_31, "the engine starts in 31-band mode");
            assert_eq!(state.gains.len(), 31);
            assert_eq!(state.gains[2], 4.5);
            assert_eq!(
                state.selected_preset.map(|p| p.name).as_deref(),
                Some("Rock")
            );
        });
    }

    #[test]
    fn enabling_pushes_the_preamp_and_persists() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let mut state = twenty_band_state(&ctx);
            set_preamp(&ctx, &mut state, 3.04).recv_blocking().unwrap();
            assert_eq!(state.preamp_gain, 3.0);
            assert!(
                !fakes
                    .player
                    .calls()
                    .contains(&PlayerCall::SetPreampGain(3.0)),
                "a preamp change waits while the EQ is off"
            );
            assert_eq!(raw(&ctx, keys::PREAMP_GAIN).as_deref(), Some("3.0"));

            fakes.player.take_calls();
            set_enabled(&ctx, &mut state, true).recv_blocking().unwrap();
            assert_eq!(
                fakes.player.calls(),
                vec![
                    PlayerCall::SetEqEnabled(true),
                    PlayerCall::SetPreampGain(3.0)
                ]
            );
            assert_eq!(raw(&ctx, keys::EQ_ENABLED).as_deref(), Some("1"));

            set_preamp(&ctx, &mut state, 12.0).recv_blocking().unwrap();
            assert_eq!(state.preamp_gain, 9.0, "clamped to ±9 dB");
            assert!((fakes.player.preamp_gain() - 9.0).abs() < 1e-4);

            fakes.player.take_calls();
            set_enabled(&ctx, &mut state, false)
                .recv_blocking()
                .unwrap();
            assert_eq!(fakes.player.calls(), vec![PlayerCall::SetEqEnabled(false)]);
            assert_eq!(raw(&ctx, keys::EQ_ENABLED).as_deref(), Some("0"));
        });
    }

    #[test]
    fn switching_modes_swaps_the_band_sets_and_rereads_the_gains() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let mut state = load_state(&ctx).recv_blocking().unwrap();
            set_band_gain(&ctx, &mut state, 0, 2.0)
                .recv_blocking()
                .unwrap();
            set_mode(&ctx, &mut state, false).recv_blocking().unwrap();
            assert!(!state.bands_31);
            assert_eq!(state.gains, vec![0.0; 20], "the 20-band set was untouched");
            assert_eq!(fakes.player.eq_band_count(), 20);
            assert_eq!(raw(&ctx, keys::EQ_BANDS_31).as_deref(), Some("0"));
            set_mode(&ctx, &mut state, true).recv_blocking().unwrap();
            assert_eq!(state.gains[0], 2.0);
        });
    }

    #[test]
    fn band_gains_are_rounded_clamped_and_persisted() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let mut state = twenty_band_state(&ctx);
            set_band_gain(&ctx, &mut state, 3, -3.06)
                .recv_blocking()
                .unwrap();
            set_band_gain(&ctx, &mut state, 4, -12.0)
                .recv_blocking()
                .unwrap();
            set_band_gain(&ctx, &mut state, 40, 1.0)
                .recv_blocking()
                .unwrap();
            assert_eq!(state.gains[3], -3.1);
            assert_eq!(state.gains[4], -9.0);
            assert_eq!(fakes.player.eq_band(3).map(|b| b.1), Some(-3.1));
            let stored = raw(&ctx, keys::EQ_BAND_GAINS_20).unwrap();
            assert!(stored.starts_with("0.0,0.0,0.0,-3.1,-9.0,0.0"), "{stored}");
        });
    }

    #[test]
    fn both_band_sets_are_persisted_with_a_converted_inactive_set() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let mut state = twenty_band_state(&ctx);
            for (i, gain) in ramp(20).into_iter().enumerate() {
                state.gains[i] = gain;
            }
            save(&ctx, &state).recv_blocking().unwrap();
            let gains_20 = raw(&ctx, keys::EQ_BAND_GAINS_20).unwrap();
            let gains_31 = raw(&ctx, keys::EQ_BAND_GAINS_31).unwrap();
            assert_eq!(gains_20, rules::encode_band_gains(&ramp(20)));
            let converted: Vec<&str> = gains_31.split(',').collect();
            assert_eq!(converted.len(), 31);
            // 20 Hz is below the first 20-band centre (22.4 Hz): its gain.
            assert_eq!(converted[0], "0.0");
            // 25 Hz lies a third of an octave-step between 22.4 Hz (0) and
            // 31.5 Hz (1): 0.32, rounded to 0.3.
            assert_eq!(converted[1], "0.3");
            // 31.5 Hz is a 20-band centre: its gain exactly.
            assert_eq!(converted[2], "1.0");
            // 20 kHz is above the last centre (16 kHz): its gain.
            assert_eq!(converted[30], "19.0");

            set_mode(&ctx, &mut state, true).recv_blocking().unwrap();
            state.gains = ramp(31);
            save(&ctx, &state).recv_blocking().unwrap();
            let gains_20 = raw(&ctx, keys::EQ_BAND_GAINS_20).unwrap();
            let converted: Vec<&str> = gains_20.split(',').collect();
            assert_eq!(converted.len(), 20, "not the first 20 of the 31 gains");
            // 22.4 Hz lies halfway (in log frequency) between 20 Hz (0) and
            // 25 Hz (1): 0.51, rounded to 0.5.
            assert_eq!(converted[0], "0.5");
            // 31.5 Hz and 16 kHz are 31-band centres 2 and 29.
            assert_eq!(converted[1], "2.0");
            assert_eq!(converted[19], "29.0");
            assert_eq!(
                raw(&ctx, keys::EQ_BAND_GAINS_31).unwrap(),
                rules::encode_band_gains(&ramp(31))
            );
        });
    }

    #[test]
    fn previewing_a_preset_changes_the_engine_without_persisting() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let mut state = twenty_band_state(&ctx);
            let rock = find_preset("Rock", &[]).unwrap();
            preview_preset(&ctx, &mut state, &rock);
            assert_eq!(
                state.selected_preset.as_ref().map(|p| p.name.as_str()),
                Some("Rock")
            );
            assert_eq!(state.gains, rock.get_gains_20());
            assert!(
                fakes
                    .player
                    .calls()
                    .contains(&PlayerCall::ApplyEqPreset("Rock".into()))
            );
            assert_eq!(raw(&ctx, keys::EQ_SELECTED_PRESET).as_deref(), Some(""));

            apply_preset(&ctx, &mut state, &rock)
                .recv_blocking()
                .unwrap();
            assert_eq!(raw(&ctx, keys::EQ_SELECTED_PRESET).as_deref(), Some("Rock"));
        });
    }

    #[test]
    fn a_cancelled_preview_restores_both_band_sets_the_preamp_and_the_preset() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let mut state = twenty_band_state(&ctx);
            set_band_gain(&ctx, &mut state, 1, 1.5)
                .recv_blocking()
                .unwrap();
            set_mode(&ctx, &mut state, true).recv_blocking().unwrap();
            set_band_gain(&ctx, &mut state, 5, -2.0)
                .recv_blocking()
                .unwrap();
            set_mode(&ctx, &mut state, false).recv_blocking().unwrap();
            set_preamp(&ctx, &mut state, -1.0).recv_blocking().unwrap();
            let before = state.clone();

            let snapshot = capture(&ctx, &state);
            assert_eq!(snapshot.gains_20[1], 1.5);
            assert_eq!(snapshot.gains_31[5], -2.0);

            preview_preset(&ctx, &mut state, &find_preset("Party", &[]).unwrap());
            restore(&ctx, &mut state, &snapshot);

            assert_eq!(state, before);
            assert_eq!(
                engine_band_sets(&*fakes.player),
                (snapshot.gains_20, snapshot.gains_31)
            );
            assert!((fakes.player.preamp_gain() + 1.0).abs() < 1e-4);
        });
    }

    #[test]
    fn reset_flattens_both_sets_and_clears_the_preset() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let mut state = twenty_band_state(&ctx);
            apply_preset(&ctx, &mut state, &find_preset("Rock", &[]).unwrap())
                .recv_blocking()
                .unwrap();
            reset(&ctx, &mut state).recv_blocking().unwrap();
            assert_eq!(state.gains, vec![0.0; 20]);
            assert_eq!(state.preamp_gain, 0.0);
            assert_eq!(state.selected_preset, None);
            assert_eq!(
                engine_band_sets(&*fakes.player),
                (vec![0.0; 20], vec![0.0; 31])
            );
            assert_eq!(raw(&ctx, keys::EQ_SELECTED_PRESET).as_deref(), Some(""));
            assert_eq!(
                raw(&ctx, keys::EQ_BAND_GAINS_31).unwrap(),
                vec!["0.0"; 31].join(",")
            );
        });
    }

    #[test]
    fn the_unified_list_puts_default_then_customs_then_builtins() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let state = EqState::default();
            save_preset(&ctx, &state, "zeta").recv_blocking().unwrap();
            let list = save_preset(&ctx, &state, "Alpha").recv_blocking().unwrap();
            let unified = list.unified();
            let names: Vec<(&str, PresetKind)> = unified
                .iter()
                .take(4)
                .map(|e| (e.preset.name.as_str(), e.kind))
                .collect();
            assert_eq!(
                names,
                [
                    ("Default", PresetKind::Default),
                    ("Alpha", PresetKind::Custom),
                    ("zeta", PresetKind::Custom),
                    ("Ballad", PresetKind::Builtin),
                ]
            );
            assert_eq!(unified.len(), EqPreset::default_presets().len() + 2);
        });
    }

    #[test]
    fn saving_a_preset_derives_the_other_band_set_and_keeps_the_selection() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let mut state = twenty_band_state(&ctx);
            state.gains = ramp(20);
            state.preamp_gain = -2.0;
            let list = save_preset(&ctx, &state, "Mine").recv_blocking().unwrap();
            let mine = list.customs.iter().find(|p| p.name == "Mine").unwrap();
            assert_eq!(mine.bands_20.as_deref(), Some(&ramp(20)[..]));
            assert_eq!(mine.bands_31, Some(EqPreset::convert_20_to_31(&ramp(20))));
            assert_eq!(mine.preamp_gain, -2.0);
            assert_eq!(raw(&ctx, keys::PREAMP_GAIN).as_deref(), Some("-2.0"));
            assert!(matches!(
                save_preset(&ctx, &state, "").recv_blocking(),
                Err(FrontendError::InvalidInput(_))
            ));
        });
    }

    #[test]
    fn deleting_a_builtin_hides_it_across_restarts() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let state = EqState::default();
            let list = delete_preset(&ctx, &state, "Rock").recv_blocking().unwrap();
            assert_eq!(list.hidden_builtins, vec!["Rock".to_string()]);
            assert!(!list.unified().iter().any(|e| e.preset.name == "Rock"));
            assert_eq!(
                raw(&ctx, keys::EQ_HIDDEN_BUILTIN_PRESETS).as_deref(),
                Some(r#"["Rock"]"#)
            );
            // What a restart reads back.
            let reread = presets(&ctx).recv_blocking().unwrap();
            assert_eq!(reread.hidden_builtins, vec!["Rock".to_string()]);
            assert!(!reread.unified().iter().any(|e| e.preset.name == "Rock"));
        });
    }

    #[test]
    fn deleting_a_custom_preset_removes_it_from_the_database() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let state = EqState::default();
            save_preset(&ctx, &state, "Rock").recv_blocking().unwrap();
            let list = delete_preset(&ctx, &state, "Rock").recv_blocking().unwrap();
            assert!(list.customs.is_empty());
            assert!(
                list.hidden_builtins.is_empty(),
                "the custom shadowed the built-in"
            );
            assert!(list.unified().iter().any(|e| e.preset.name == "Rock"));
        });
    }

    #[test]
    fn restoring_the_default_presets_clears_customs_and_the_hidden_list() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let state = EqState::default();
            save_preset(&ctx, &state, "Mine").recv_blocking().unwrap();
            delete_preset(&ctx, &state, "Jazz").recv_blocking().unwrap();
            let list = restore_default_presets(&ctx).recv_blocking().unwrap();
            assert!(list.customs.is_empty());
            assert!(list.hidden_builtins.is_empty());
            assert_eq!(
                raw(&ctx, keys::EQ_HIDDEN_BUILTIN_PRESETS).as_deref(),
                Some("[]")
            );
        });
    }

    #[test]
    fn importing_a_preset_saves_applies_and_persists_it() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, fakes) = context();
            let state = twenty_band_state(&ctx);
            let path = temp_path("bright.json");
            let json = serde_json::json!({
                "version": 1,
                "name": "Bright",
                "preamp_gain": -1.5,
                "bands_20": null,
                "bands_31": vec![2.0; 31],
            });
            std::fs::write(&path, json.to_string()).unwrap();

            let imported = import_preset(&ctx, &state, path.clone())
                .recv_blocking()
                .unwrap();
            assert_eq!(imported.preset.name, "Bright");
            assert_eq!(
                imported.state.gains,
                vec![2.0; 20],
                "the 20-band set is converted"
            );
            assert_eq!(imported.state.preamp_gain, -1.5);
            assert!(imported.presets.customs.iter().any(|p| p.name == "Bright"));
            assert!(
                fakes
                    .player
                    .calls()
                    .contains(&PlayerCall::ApplyEqPreset("Bright".into()))
            );
            // The persistence was queued behind the import.
            ctx.db.read(|_| Ok(())).recv_blocking().unwrap();
            assert_eq!(
                raw(&ctx, keys::EQ_SELECTED_PRESET).as_deref(),
                Some("Bright")
            );
            assert_eq!(raw(&ctx, keys::PREAMP_GAIN).as_deref(), Some("-1.5"));
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        });
    }

    #[test]
    fn importing_a_file_that_is_not_a_preset_is_an_error() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let state = EqState::default();
            let path = temp_path("broken.json");
            std::fs::write(&path, "{ not json").unwrap();
            assert!(matches!(
                import_preset(&ctx, &state, path.clone()).recv_blocking(),
                Err(FrontendError::InvalidInput(_))
            ));
            std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
            assert!(matches!(
                import_preset(&ctx, &state, path.clone()).recv_blocking(),
                Err(FrontendError::InvalidInput(_))
            ));
            assert!(matches!(
                import_preset(&ctx, &state, path.with_file_name("missing.json")).recv_blocking(),
                Err(FrontendError::Io(_))
            ));
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        });
    }

    #[test]
    fn exporting_names_the_preset_after_the_file_and_writes_both_sets() {
        with_timeout(TEST_TIMEOUT, || {
            let (ctx, _fakes) = context();
            let mut state = twenty_band_state(&ctx);
            state.gains = ramp(20);
            state.preamp_gain = 1.0;
            let path = temp_path("My EQ.json");
            export_preset(&ctx, &state, path.clone())
                .recv_blocking()
                .unwrap();
            let written = preset_from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(written, state.to_preset("My EQ"));
            assert_eq!(written.bands_31.map(|b| b.len()), Some(31));

            let unwritable = path.parent().unwrap().join("no-such-dir").join("x.json");
            assert!(matches!(
                export_preset(&ctx, &state, unwritable).recv_blocking(),
                Err(FrontendError::Io(_))
            ));
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        });
    }
}
