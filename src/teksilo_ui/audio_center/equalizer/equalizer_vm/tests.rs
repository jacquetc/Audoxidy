//! Tests of the equalizer view-model on the in-memory facade: the controls
//! and what they persist, both band sets across the 20 / 31 switch, the Load
//! dialog's preview, accept and cancel, the preset operations, and the files.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use teksilo::core::widget_tree::WidgetTree;
use teksilo::prelude::AsyncRuntimeHandle;
use teksilo::widgets::{ToastInstallOptions, ToastRegistry};

use super::*;
use crate::frontend::AppContext;
use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::keys;
use crate::teksilo_ui::app::services::AppServices;
use crate::teksilo_ui::mock_data::screenshot_library;
use crate::teksilo_ui::models::EqPresetsModel;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{
    press, run_until, tree_with_app_state, with_shipped_messages,
};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Everything a view-model test needs.
pub(crate) struct Fixture {
    pub services: AppServices,
    pub fakes: InMemoryFakes,
    pub toasts: ToastRegistry,
    pub tree: WidgetTree,
    pub vm: EqualizerViewModel,
}

impl Fixture {
    /// A view-model over the mock library, in 20-band mode, loaded.
    pub fn new() -> Self {
        Self::in_locale("es-ES")
    }

    /// [`Fixture::new`] with the shipped messages of `locale` installed for
    /// the rest of the test (every test runs on its own thread).
    pub fn in_locale(locale: &str) -> Self {
        install_locale(locale);
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        screenshot_library()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        // The fake engine starts in 31-band mode; Iced's manager starts in 20.
        fakes.player.set_eq_mode(20);
        let runtime = AsyncRuntimeHandle::new();
        let toasts = ToastRegistry::new(ToastInstallOptions {
            archive: None,
            ..ToastInstallOptions::default()
        });
        let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
        state.insert(
            TypeId::of::<AsyncRuntimeHandle>(),
            Box::new(runtime.clone()),
        );
        state.insert(TypeId::of::<ToastRegistry>(), Box::new(toasts.clone()));
        let tree = tree_with_app_state(&ctx, state);
        let services = AppServices::new(&AppHandles::new(ctx), runtime);
        let vm = EqualizerViewModel::new(&services);
        let fixture = Self {
            services,
            fakes,
            toasts,
            tree,
            vm,
        };
        let presets = fixture.services.eq_presets.version();
        fixture.wait(|| presets.get() >= 1);
        fixture.vm.reload();
        fixture.wait(|| fixture.vm.state().gains.len() == 20);
        fixture.fakes.player.take_calls();
        fixture
    }

    /// Ticks the executor until `done` holds.
    pub fn wait(&self, done: impl Fn() -> bool) {
        assert!(
            run_until(&self.services.runtime, TIMEOUT, done),
            "timed out"
        );
    }

    /// The raw value of a setting, once it satisfies `ready`.
    pub fn setting_when(
        &self,
        key: &'static str,
        ready: impl Fn(Option<&str>) -> bool,
    ) -> Option<String> {
        let last = std::cell::RefCell::new(None);
        self.wait(|| {
            *last.borrow_mut() = self.raw(key);
            ready(last.borrow().as_deref())
        });
        last.into_inner()
    }

    pub fn raw(&self, key: &'static str) -> Option<String> {
        self.services
            .ctx
            .db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    /// The index of the row called `name` in the preset list.
    pub fn row(&self, name: &str) -> usize {
        let list = self.services.eq_presets.list();
        (0..list.len())
            .find(|&i| {
                list.with_item(i, |e| e.preset.name == name)
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| panic!("no preset row {name:?}"))
    }

    pub fn names(&self) -> Vec<String> {
        let list = self.services.eq_presets.list();
        (0..list.len())
            .filter_map(|i| list.with_item(i, |e| e.preset.name.clone()))
            .collect()
    }

    /// Runs `f` with an event context, as a click would.
    pub fn press(&mut self, f: impl Fn(&mut EventContext) + 'static) {
        press(&mut self.tree, f);
    }

    /// Takes the modal the last click presented and builds its content into
    /// the tree: the layer's id, and the callback the framework runs when the
    /// modal goes away by any route.
    pub fn open_modal(
        &mut self,
    ) -> (
        WidgetId,
        Option<teksilo::core::overlay::OverlayDismissCallback>,
    ) {
        use teksilo::core::modal::ModalContent;
        let queued = self
            .tree
            .drain_pending_modal_requests()
            .pop()
            .expect("no modal was presented");
        let on_dismiss = queued.request.on_dismiss.clone();
        let id = match queued.request.content {
            ModalContent::Deferred(build) => build(&mut self.tree),
            ModalContent::ExistingWidget(id) => id,
        };
        crate::teksilo_ui::test_support::layout(&mut self.tree);
        let _ = self.tree.render();
        (id, on_dismiss)
    }

    pub fn finish(self) {
        assert!(self.services.ctx.shutdown());
    }
}

fn install_locale(locale: &str) {
    use teksilo::i18n::config::I18nConfig;
    use teksilo::i18n::manager::I18nManager;
    use teksilo::i18n::thread_local::{clear, install};
    let Some(&(tag, files)) = crate::teksilo_ui::startup::app_locales()
        .iter()
        .find(|(tag, _)| *tag == locale)
    else {
        panic!("{locale} is not a locale the app ships");
    };
    let id: teksilo::i18n::LanguageIdentifier = tag.parse().unwrap();
    clear();
    let config = I18nConfig::new()
        .source_locale(id.clone())
        .supported_locales([id.clone()])
        .compile_in(&[(tag, files)])
        .auto_detect_os_locale(false)
        .fallback_locale(id);
    install(I18nManager::from_config(&config));
}

fn temp_path(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "audoxidy-eq-vm-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

// --- Loading -------------------------------------------------------------------------------

#[test]
fn the_view_model_shows_the_engines_equalizer_once_loaded() {
    let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
    fakes.player.set_eq_enabled(true);
    fakes.player.set_eq_band_gain(2, 4.5);
    let runtime = AsyncRuntimeHandle::new();
    let services = AppServices::new(&AppHandles::new(ctx), runtime.clone());
    let vm = EqualizerViewModel::new(&services);
    assert!(run_until(&runtime, TIMEOUT, || vm.enabled().get()));
    assert!(vm.bands_31().get(), "the engine starts in 31-band mode");
    assert_eq!(vm.mode().get(), MODE_31);
    assert_eq!(vm.band_count(), 31);
    assert_eq!(vm.band_gain(2).get(), 4.5);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_late_reload_does_not_overwrite_a_change_made_meanwhile() {
    let (ctx, _fakes) = AppContext::in_memory_with_fakes().unwrap();
    let runtime = AsyncRuntimeHandle::new();
    let services = AppServices::new(&AppHandles::new(ctx), runtime.clone());
    let vm = EqualizerViewModel::new(&services);
    // The constructor's reload is in flight; the user acts before it answers.
    vm.set_preamp(4.0);
    let _ = run_until(&runtime, Duration::from_millis(300), || false);
    assert_eq!(vm.preamp().get(), 4.0, "the older answer was dropped");
    assert!(services.ctx.shutdown());
}

#[test]
fn band_labels_are_the_engines_frequencies_as_iced_writes_them() {
    let f = Fixture::new();
    let first = f.vm.band_label(0);
    let expected =
        crate::logic::eq::band_frequency_label(f.fakes.player.eq_band(0).map(|b| b.0).unwrap());
    assert_eq!(first, expected);
    assert_eq!(
        f.vm.band_label(1),
        crate::logic::eq::band_frequency_label(f.fakes.player.eq_band(1).map(|b| b.0).unwrap())
    );
    f.finish();
}

// --- The controls --------------------------------------------------------------------------

#[test]
fn the_toggle_turns_the_equalizer_on_and_persists_it() {
    let f = Fixture::new();
    f.vm.set_preamp(3.0);
    f.fakes.player.take_calls();
    f.vm.set_enabled(true);
    assert!(f.vm.enabled().get());
    assert_eq!(
        f.fakes.player.calls(),
        vec![
            PlayerCall::SetEqEnabled(true),
            PlayerCall::SetPreampGain(3.0)
        ],
        "turning it on also pushes the preamp, as Iced does"
    );
    assert_eq!(
        f.setting_when(keys::EQ_ENABLED, |v| v == Some("1"))
            .as_deref(),
        Some("1")
    );
    f.vm.set_enabled(false);
    assert!(!f.vm.enabled().get());
    f.setting_when(keys::EQ_ENABLED, |v| v == Some("0"));
    f.finish();
}

#[test]
fn the_preamp_and_a_band_are_rounded_clamped_and_persisted() {
    let f = Fixture::new();
    f.vm.set_preamp(3.04);
    assert_eq!(f.vm.preamp().get(), 3.0);
    f.vm.set_preamp(12.0);
    assert_eq!(f.vm.preamp().get(), 9.0, "clamped to 9 dB");
    f.setting_when(keys::PREAMP_GAIN, |v| v == Some("9.0"));

    f.vm.set_band_gain(3, -3.06);
    assert_eq!(f.vm.band_gain(3).get(), -3.1);
    assert_eq!(f.fakes.player.eq_band(3).map(|b| b.1), Some(-3.1));
    let stored = f
        .setting_when(keys::EQ_BAND_GAINS_20, |v| {
            v.is_some_and(|v| v.starts_with("0.0,0.0,0.0,-3.1"))
        })
        .unwrap();
    assert!(stored.starts_with("0.0,0.0,0.0,-3.1"), "{stored}");

    f.vm.set_band_gain(40, 1.0);
    assert_eq!(
        f.vm.state().gains.len(),
        20,
        "a band past the set is ignored"
    );
    f.finish();
}

#[test]
fn switching_between_20_and_31_bands_keeps_both_sets() {
    let f = Fixture::new();
    f.vm.set_band_gain(5, 2.0);
    assert_eq!(f.vm.mode().get(), MODE_20);

    f.vm.set_bands_31(true);
    assert!(f.vm.bands_31().get());
    assert_eq!(f.vm.mode().get(), MODE_31, "the radios follow the state");
    assert_eq!(f.vm.band_count(), 31);
    f.vm.set_band_gain(7, -4.0);
    f.setting_when(keys::EQ_BANDS_31, |v| v == Some("1"));

    f.vm.set_bands_31(false);
    assert_eq!(f.vm.band_count(), 20);
    assert_eq!(f.vm.band_gain(5).get(), 2.0, "the 20-band set was kept");
    f.vm.set_bands_31(true);
    assert_eq!(f.vm.band_gain(7).get(), -4.0, "the 31-band set was kept");

    // Quirk fix 18: both sets reach the database.
    f.setting_when(keys::EQ_BAND_GAINS_31, |v| {
        v.is_some_and(|v| v.split(',').nth(7) == Some("-4.0"))
    });
    f.setting_when(keys::EQ_BAND_GAINS_20, |v| {
        v.is_some_and(|v| v.split(',').count() == 20)
    });
    f.finish();
}

#[test]
fn selecting_the_active_mode_again_does_nothing() {
    let f = Fixture::new();
    f.fakes.player.take_calls();
    f.vm.set_bands_31(false);
    assert!(f.fakes.player.calls().is_empty());
    f.finish();
}

#[test]
fn the_reset_icon_flattens_the_bands_the_preamp_and_the_preset() {
    let f = Fixture::new();
    f.vm.set_band_gain(2, 5.0);
    f.vm.set_preamp(2.0);
    let load = f.vm.begin_load();
    load.select(f.row("Rock"));
    load.accept();
    assert_eq!(f.vm.preset_name().get().as_deref(), Some("Rock"));
    f.vm.reset();
    assert_eq!(f.vm.preamp().get(), 0.0);
    assert!(f.vm.state().gains.iter().all(|g| *g == 0.0));
    assert_eq!(f.vm.preset_name().get(), None);
    assert!(!f.vm.preset_shown().get());
    f.finish();
}

// --- The Load dialog -----------------------------------------------------------------------

#[test]
fn the_list_starts_with_default_then_the_customs_then_the_built_ins() {
    let f = Fixture::new();
    let names = f.names();
    assert_eq!(names[0], "Default");
    assert!(names.contains(&"Rock".to_string()));
    let load = f.vm.begin_load();
    assert_eq!(load.custom_count(), 0);
    f.finish();
}

#[test]
fn a_click_previews_the_preset_without_saving_it() {
    let f = Fixture::new();
    let before = f.vm.state();
    let load = f.vm.begin_load();
    f.fakes.player.take_calls();
    let rock = f.row("Rock");
    load.select(rock);
    assert_eq!(load.selected().get(), Some(rock));
    assert_eq!(load.selected_name().as_deref(), Some("Rock"));
    assert_eq!(f.vm.preset_name().get().as_deref(), Some("Rock"));
    assert_ne!(f.vm.state().gains, before.gains);
    assert!(
        f.fakes
            .player
            .calls()
            .contains(&PlayerCall::ApplyEqPreset("Rock".into()))
    );
    // Nothing was persisted: the previous selection is still the stored one.
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(f.raw(keys::EQ_SELECTED_PRESET), None);
    f.finish();
}

#[test]
fn cancelling_restores_the_state_and_both_band_sets() {
    let f = Fixture::new();
    f.vm.set_band_gain(1, 1.5);
    f.vm.set_preamp(1.0);
    f.vm.set_bands_31(true);
    f.vm.set_band_gain(9, -2.5);
    f.vm.set_bands_31(false);
    let before = f.vm.state();
    let load = f.vm.begin_load();
    load.select(f.row("Rock"));
    load.select(f.row("Default"));
    load.select(f.row("Rock"));
    assert_ne!(f.vm.state().gains, before.gains);
    load.cancel();
    let after = f.vm.state();
    assert_eq!(after.gains, before.gains);
    assert_eq!(after.preamp_gain, 1.0);
    assert_eq!(f.vm.preamp().get(), 1.0);
    assert_eq!(f.vm.band_gain(1).get(), 1.5);
    assert_eq!(f.vm.preset_name().get(), None);
    // Quirk fix 20: the other set came back too.
    f.vm.set_bands_31(true);
    assert_eq!(f.vm.band_gain(9).get(), -2.5);
    assert!(load.settled());
    f.finish();
}

#[test]
fn accepting_keeps_the_preview_and_a_later_dismissal_restores_nothing() {
    let f = Fixture::new();
    let load = f.vm.begin_load();
    load.select(f.row("Rock"));
    let previewed = f.vm.state();
    load.accept();
    f.setting_when(keys::EQ_SELECTED_PRESET, |v| v == Some("Rock"));
    load.cancel();
    assert_eq!(
        f.vm.state(),
        previewed,
        "the dismissal after Aceptar is inert"
    );
    f.finish();
}

#[test]
fn cancelling_twice_restores_once() {
    let f = Fixture::new();
    let load = f.vm.begin_load();
    load.select(f.row("Rock"));
    load.cancel();
    f.vm.set_preamp(5.0);
    load.cancel();
    assert_eq!(f.vm.preamp().get(), 5.0, "the second cancel did nothing");
    f.finish();
}

#[test]
fn deleting_a_built_in_hides_it_across_a_reload() {
    let mut f = Fixture::new();
    let load = f.vm.begin_load();
    let rock = f.row("Rock");
    load.select(rock);
    assert!(f.names().contains(&"Rock".to_string()));
    let session = load.clone();
    f.press(move |ctx| session.delete_selected(ctx));
    let model = f.services.eq_presets.clone();
    f.wait(|| !model.entry("Rock").is_some());
    // A fresh model (a restart, as far as the list goes) still hides it.
    let fresh = EqPresetsModel::new(f.services.ctx.clone(), f.services.runtime.clone());
    fresh.reload();
    f.wait(|| fresh.version().get() >= 1);
    assert!(
        fresh.entry("Rock").is_none(),
        "quirk fix 14: hidden for good"
    );
    assert!(fresh.entry("Default").is_some());
    f.finish();
}

#[test]
fn deleting_with_nothing_highlighted_does_nothing() {
    let mut f = Fixture::new();
    let load = f.vm.begin_load();
    let before = f.names();
    let session = load.clone();
    f.press(move |ctx| session.delete_selected(ctx));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(f.names(), before);
    f.finish();
}

#[test]
fn saving_adds_a_custom_row_and_an_empty_name_does_nothing() {
    let mut f = Fixture::new();
    f.vm.set_band_gain(4, 3.0);
    let vm = f.vm.clone();
    f.press(move |ctx| vm.save_preset("My curve", ctx));
    let model = f.services.eq_presets.clone();
    f.wait(|| model.entry("My curve").is_some());
    assert_eq!(f.names()[1], "My curve", "customs come right after Default");
    let saved = model.entry("My curve").unwrap().preset;
    assert_eq!(saved.get_gains_20()[4], 3.0);
    assert_eq!(saved.get_gains_20().len(), 20);
    assert_eq!(saved.get_gains_31().len(), 31, "both band sets are saved");
    assert_eq!(f.vm.preset_name().get(), None, "saving does not select it");

    let count = f.names().len();
    let vm = f.vm.clone();
    f.press(move |ctx| vm.save_preset("", ctx));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(f.names().len(), count);
    f.finish();
}

#[test]
fn predeterminado_deletes_every_custom_preset_and_unhides_the_built_ins() {
    let mut f = Fixture::new();
    let vm = f.vm.clone();
    f.press(move |ctx| vm.save_preset("Mine", ctx));
    let model = f.services.eq_presets.clone();
    f.wait(|| model.entry("Mine").is_some());
    let load = f.vm.begin_load();
    let rock = f.row("Rock");
    load.select(rock);
    let session = load.clone();
    f.press(move |ctx| session.delete_selected(ctx));
    f.wait(|| model.entry("Rock").is_none());

    let session = load.clone();
    f.press(move |ctx| session.restore_defaults(ctx));
    f.wait(|| model.entry("Rock").is_some() && model.entry("Mine").is_none());
    f.finish();
}

// --- Import and export ---------------------------------------------------------------------

#[test]
fn exporting_writes_the_state_named_after_the_file_stem() {
    let mut f = Fixture::new();
    f.vm.set_band_gain(2, 6.0);
    f.vm.set_preamp(-2.0);
    let load = f.vm.begin_load();
    let path = temp_path("warm.json");
    let (session, target) = (load.clone(), path.clone());
    f.press(move |ctx| session.export(target.clone(), ctx));
    f.wait(|| path.exists() && std::fs::metadata(&path).is_ok_and(|m| m.len() > 0));
    let preset =
        crate::audio::preset::preset_from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(preset.name, "warm");
    assert_eq!(preset.preamp_gain, -2.0);
    assert_eq!(preset.get_gains_20()[2], 6.0);
    assert_eq!(preset.get_gains_31().len(), 31);
    f.finish();
}

#[test]
fn importing_applies_the_file_at_once_and_adds_it_to_the_list() {
    let mut f = Fixture::new();
    let preset = crate::audio::preset::EqPreset::new(
        "from file",
        1.5,
        Some(vec![2.0; 20]),
        Some(vec![2.0; 31]),
    );
    let path = temp_path("from-file.aeqp");
    std::fs::write(
        &path,
        crate::audio::preset::preset_to_json(&preset).unwrap(),
    )
    .unwrap();
    let load = f.vm.begin_load();
    let (session, source) = (load.clone(), path.clone());
    f.press(move |ctx| session.import(source.clone(), ctx));
    let model = f.services.eq_presets.clone();
    f.wait(|| model.entry("from file").is_some());
    assert_eq!(f.vm.preset_name().get().as_deref(), Some("from file"));
    assert_eq!(f.vm.preamp().get(), 1.5);
    assert!(f.vm.state().gains.iter().all(|g| *g == 2.0));
    assert_eq!(f.fakes.player.eq_band(0).map(|b| b.1), Some(2.0));
    f.setting_when(keys::EQ_SELECTED_PRESET, |v| v == Some("from file"));
    f.finish();
}

#[test]
fn importing_a_file_that_is_not_a_preset_changes_nothing() {
    let mut f = Fixture::new();
    let path = temp_path("broken.json");
    std::fs::write(&path, "{ not a preset").unwrap();
    let before = (f.vm.state(), f.names());
    let load = f.vm.begin_load();
    let (session, source) = (load.clone(), path.clone());
    f.press(move |ctx| session.import(source.clone(), ctx));
    std::thread::sleep(Duration::from_millis(100));
    f.wait(|| true);
    assert_eq!((f.vm.state(), f.names()), before);
    f.finish();
}

// --- Failures (quirk fix 8) ----------------------------------------------------------------

#[test]
fn a_failed_preset_operation_shows_an_error_toast() {
    let mut f = Fixture::new();
    assert_eq!(f.toasts.live_count(), 0);
    f.press(|ctx| report(ctx, EqFailure::Import, &"the file is gone"));
    assert_eq!(
        f.toasts.live_count(),
        1,
        "quirk fix 8: shown, not only logged"
    );
    f.finish();
}

#[test]
fn every_failure_has_a_toast_text_in_both_languages() {
    for failure in [
        EqFailure::Save,
        EqFailure::Delete,
        EqFailure::RestoreDefaults,
        EqFailure::Import,
        EqFailure::Export,
        EqFailure::FileDialog,
    ] {
        for locale in ["en-US", "es-ES"] {
            with_shipped_messages(locale, || {
                let _ = failure.toast("boom".to_string());
            });
        }
    }
}

#[test]
fn the_toast_texts_carry_the_error() {
    with_shipped_messages("es-ES", || {
        let text = tr!(ac_equalizer_import_failed(error = "boom".to_string())).resolve_now();
        assert_eq!(text, "No se pudo importar el preset: boom");
    });
    with_shipped_messages("en-US", || {
        let text = tr!(ac_equalizer_export_failed(error = "boom".to_string())).resolve_now();
        assert_eq!(text, "The preset could not be exported: boom");
    });
}
