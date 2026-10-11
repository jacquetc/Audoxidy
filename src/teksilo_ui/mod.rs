//! The Teksilo UI of Audoxidy (`teksilo-ui` feature, binary `audoxidy-teksilo`).
//!
//! A second desktop UI over the same backend as the Iced UI, which stays the
//! default. It is built the way Skribisto's Teksilo UI is: view-models over
//! the UI-agnostic facade (`crate::frontend`), thin views, and the facade's
//! events delivered to widgets through Teksilo's event source. It shares the
//! database, the covers cache and the settings keys with the Iced UI, so the
//! two binaries can be used on one library.
//!
//! Deliberate behaviour changes from the Iced UI are listed in
//! `docs/teksilo-port-quirk-fixes.md`; the Iced UI's stubs, kept as stubs, in
//! `docs/teksilo-port-stubs.md`.
//!
//! New code in this tree is held to a stricter bar than the code that predates
//! the port: any warning or clippy finding is an error, and `unwrap()` /
//! `expect()` are only allowed in tests (see `clippy.toml`).

#![deny(warnings, clippy::all, clippy::unwrap_used, clippy::expect_used)]

pub mod app;
pub mod audio_center;
pub mod event_source;
pub mod filters;
pub mod icons;
pub mod library;
#[cfg(any(test, feature = "teksilo-mocks"))]
pub mod mock_data;
pub mod models;
pub mod player;
pub mod shared;
pub mod shell;
pub mod singles;
pub mod startup;
pub mod style;
#[cfg(test)]
pub mod test_support;
pub mod widgets;

use teksilo::prelude::*;

use self::event_source::FrontendEventSource;
use self::shell::main_window::{self, AppHandles};

/// Runs the application until its window is closed.
///
/// Exits the process with status 1 when the backend cannot start (the Iced UI
/// panics at the same points).
pub fn run() {
    let config = startup::init_process();
    let ctx = match startup::build_context(&config) {
        Ok(ctx) => ctx,
        Err(error) => {
            tracing::error!("Audoxidy could not start: {error}");
            eprintln!("Audoxidy could not start: {error}");
            std::process::exit(1);
        }
    };
    crate::frontend::mark_ui_thread();

    let handles = AppHandles::new(ctx.clone());
    let on_exit = handles.clone();
    TeksiloAppBuilder::new()
        .app_paths(startup::app_paths())
        .settings(SettingsBundle::new().with_window_state(true))
        .theme(style::audoxidy_dark())
        .register_fonts(startup::fonts())
        .i18n(startup::i18n_config())
        .install_inspector_in_debug()
        .install_automation_bridge_in_debug()
        .install_file_dialog()
        .install_toast_default()
        .install_async()
        .event_source(FrontendEventSource::new(ctx.hub.clone()))
        .app_state(handles.clone())
        .initial_window(main_window::main_window(handles))
        .run();

    // The window's removal hook has stopped the backend. If the event loop
    // ended another way, stop it here; nothing is saved at this point.
    if !on_exit.ctx.is_shut_down() {
        on_exit.after_close();
    }
}
