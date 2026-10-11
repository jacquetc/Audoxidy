//! What runs before the first window: the process set-up the Iced binary does
//! in `main.rs`, the backend context, and the builder inputs (locales, fonts,
//! settings paths).
//!
//! The order follows `src/main.rs` and `AudoxidyApp::new`:
//!
//! 1. the configuration (`~/.config/audoxidy/config.ron`, created on first run);
//! 2. logging, configured from it;
//! 3. the memory manager's clocks;
//! 4. the deadlock watchdog, with the `deadlock-detection` feature;
//! 5. low-resource detection (RAM < 8 GB or fewer than 4 cores), stored in
//!    `utils::LOW_RESOURCE_MODE`;
//! 6. the backend: [`AppContext::production`] (database, scanner, audio
//!    engine, media controls), or, with `teksilo-mocks`, an in-memory database
//!    seeded with the mock library and fake devices.
//!
//! Step 5 runs before the audio engine starts, so the engine sizes its buffers
//! from the detected value. The Iced UI detects it only after the engine is
//! created (quirk fix 31).

use std::path::PathBuf;
use std::sync::Arc;

use teksilo::i18n::{I18nConfig, LanguageIdentifier, compile_in_locales};
use teksilo::settings::AppPaths;
use teksilo::text::{FontFaceSpec, VecFontRegistrar};
use teksilo::widgets::framework_locales;

use crate::frontend::{AppContext, FrontendError};
use crate::utils::config::AppConfig;

/// The locales the UI ships, in the order a locale picker would list them.
/// `en-US` is the source catalogue `tr!` validates against; `es-ES` carries the
/// Iced UI's own strings.
pub const SUPPORTED_LOCALES: &[&str] = &["en-US", "es-ES"];

/// The locale `tr!` validates keys against.
pub const SOURCE_LOCALE: &str = "en-US";

/// The locale used when the OS asks for one the UI does not ship: the Iced UI
/// is in Spanish.
pub const FALLBACK_LOCALE: &str = "es-ES";

/// The `.ftl` files of each locale, one per feature.
pub const LOCALE_FILES: &[&str] = &[
    "app.ftl",
    "player.ftl",
    "playlist.ftl",
    "library.ftl",
    "filters.ftl",
    "ac-shell.ftl",
    "ac-config.ftl",
    "ac-equalizer.ftl",
    "ac-effects.ftl",
    "ac-volume-mix.ftl",
    "meter.ftl",
    "dialogs.ftl",
    "controls.ftl",
    "chrome.ftl",
];

/// RAM below which the low-resource mode is enabled, in GiB (`app.rs:517`).
const LOW_RESOURCE_RAM_GB: u64 = 8;
/// Core count below which the low-resource mode is enabled (`app.rs:517`).
const LOW_RESOURCE_CORES: usize = 4;

/// Every locale's catalogue, compiled into the binary.
///
/// `base` is relative to this file (`include_str!` resolves against the
/// invoking file), so `../../locales/` is the crate's `locales/` directory.
/// The two lists are literals because the macro needs them at expansion time;
/// the drift test keeps them equal to [`SUPPORTED_LOCALES`], [`LOCALE_FILES`]
/// and the files on disk.
pub fn app_locales() -> &'static [(&'static str, &'static [&'static str])] {
    compile_in_locales!(
        base = "../../locales/",
        locales = ["en-US", "es-ES"],
        files = [
            "app.ftl",
            "player.ftl",
            "playlist.ftl",
            "library.ftl",
            "filters.ftl",
            "ac-shell.ftl",
            "ac-config.ftl",
            "ac-equalizer.ftl",
            "ac-effects.ftl",
            "ac-volume-mix.ftl",
            "meter.ftl",
            "dialogs.ftl",
            "controls.ftl",
            "chrome.ftl",
        ],
    )
}

/// Parses a locale tag from the lists above. They are fixed literals, checked
/// by a test, so the fallback to the undetermined locale is never taken.
fn locale(tag: &str) -> LanguageIdentifier {
    tag.parse().unwrap_or_else(|_| {
        tracing::error!(tag, "not a locale tag");
        LanguageIdentifier::default()
    })
}

/// The i18n configuration: the OS locale when the UI ships it, else Spanish.
pub fn i18n_config() -> I18nConfig {
    I18nConfig::new()
        .source_locale(locale(SOURCE_LOCALE))
        .supported_locales(SUPPORTED_LOCALES.iter().map(|tag| locale(tag)))
        .compile_in(app_locales())
        .auto_detect_os_locale(true)
        .fallback_locale(locale(FALLBACK_LOCALE))
        .framework_locales(framework_locales())
}

/// The faces the UI registers on top of Teksilo's bundled Inter: the logo
/// face. The bytes are compiled in, so the binary does not depend on the
/// working directory (the Iced UI loads its icons from `assets/` at run time).
pub fn fonts() -> VecFontRegistrar {
    static STAGE_WANDER: &[u8] = include_bytes!("../../assets/fonts/Stage-Wanders.ttf");
    VecFontRegistrar::new(vec![FontFaceSpec {
        data: Arc::new(STAGE_WANDER),
        is_default: false,
        default_size_px: 19.0,
    }])
}

/// Where Teksilo keeps its own files (the window geometry): next to the Iced
/// UI's configuration, in a directory of its own. The mocks build uses another
/// one, so its window geometry never replaces the real one.
pub fn app_paths() -> AppPaths {
    let base = crate::utils::config::default_config_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = if cfg!(feature = "teksilo-mocks") {
        base.join("teksilo-mocks")
    } else {
        base.join("teksilo")
    };
    AppPaths::from_dirs(dir.clone(), dir)
}

/// Steps 1 to 5 of the module documentation. Returns the configuration.
pub fn init_process() -> AppConfig {
    let config = crate::utils::config::load_config();
    crate::utils::observability::init_logging(&config.logging);
    tracing::info!(
        "Audoxidy {} (Teksilo) starting with profile {:?}",
        env!("CARGO_PKG_VERSION"),
        config.profile
    );
    crate::utils::memory_manager::MemoryManager::init();
    #[cfg(feature = "deadlock-detection")]
    crate::utils::observability::spawn_deadlock_watchdog();

    let (total_ram_gb, cpu_cores) = system_resources();
    let low_resource = is_low_resource(total_ram_gb, cpu_cores);
    crate::utils::LOW_RESOURCE_MODE.store(low_resource, std::sync::atomic::Ordering::Relaxed);
    if low_resource {
        tracing::info!(
            "Audoxidy Performance: Low resource mode ENABLED (RAM: {}GB, Cores: {})",
            total_ram_gb,
            cpu_cores
        );
    }
    config
}

/// The low-resource rule of the Iced UI (`app.rs:517`).
pub fn is_low_resource(total_ram_gb: u64, cpu_cores: usize) -> bool {
    total_ram_gb < LOW_RESOURCE_RAM_GB || cpu_cores < LOW_RESOURCE_CORES
}

/// Total RAM in whole GiB and the logical core count. Reads only the memory
/// figures and the CPU list (Iced refreshes every process as well).
fn system_resources() -> (u64, usize) {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    (
        system.total_memory() / 1024 / 1024 / 1024,
        system.cpus().len(),
    )
}

/// Step 6: the backend context.
///
/// The real build opens the database and the audio device. The mocks build
/// seeds an in-memory database with the mock library and waits for the seed,
/// so the session restore (queued after it on the same worker) finds it.
/// Called on the main thread before the event loop starts and before it is
/// marked as the UI thread.
pub fn build_context(config: &AppConfig) -> Result<AppContext, FrontendError> {
    #[cfg(not(feature = "teksilo-mocks"))]
    {
        AppContext::production(config)
    }
    #[cfg(feature = "teksilo-mocks")]
    {
        let (ctx, _fakes) = AppContext::in_memory_with_config(config.clone())?;
        crate::teksilo_ui::mock_data::screenshot_library()
            .seed_on(&ctx.db)
            .recv_blocking()?;
        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_low_resource_rule_matches_the_iced_ui() {
        assert!(is_low_resource(7, 8));
        assert!(is_low_resource(16, 3));
        assert!(is_low_resource(4, 2));
        assert!(!is_low_resource(8, 4));
        assert!(!is_low_resource(32, 16));
    }

    #[test]
    fn every_locale_tag_parses() {
        for tag in SUPPORTED_LOCALES
            .iter()
            .chain([&SOURCE_LOCALE, &FALLBACK_LOCALE])
        {
            let parsed: Result<LanguageIdentifier, _> = tag.parse();
            assert!(parsed.is_ok(), "{tag}");
        }
        assert!(SUPPORTED_LOCALES.contains(&SOURCE_LOCALE));
        assert!(SUPPORTED_LOCALES.contains(&FALLBACK_LOCALE));
    }

    /// The macro's literal lists, the constants and the files on disk must
    /// agree: a file on disk missing from the lists would never ship.
    #[test]
    fn the_compiled_catalogue_matches_the_locale_files_on_disk() {
        let compiled = app_locales();
        let tags: Vec<&str> = compiled.iter().map(|(tag, _)| *tag).collect();
        assert_eq!(tags, SUPPORTED_LOCALES);
        for (tag, files) in compiled {
            assert_eq!(files.len(), LOCALE_FILES.len(), "{tag}");
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("locales")
                .join(tag);
            let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".ftl"))
                .collect();
            on_disk.sort();
            let mut listed: Vec<String> = LOCALE_FILES.iter().map(|f| f.to_string()).collect();
            listed.sort();
            assert_eq!(on_disk, listed, "{tag}");
        }
    }

    #[test]
    fn the_settings_paths_live_beside_the_iced_configuration() {
        let paths = app_paths();
        let config_dir = crate::utils::config::default_config_path();
        let config_dir = config_dir.parent().unwrap();
        assert_eq!(paths.config_dir().parent(), Some(config_dir));
        assert_eq!(paths.config_dir(), paths.data_dir());
    }
}
