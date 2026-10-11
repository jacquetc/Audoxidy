//! The player menu: the entries, their actions, and the popover that hosts
//! them at the menu button.
//!
//! Port of the dropdown in `src/gui/player.rs:455-575`. The entries are text
//! rows ([`PlayerMenu`] draws them in
//! Iced's frame):
//!
//! | Entry | Action |
//! | --- | --- |
//! | Abrir Carpeta | pick a folder, then scan it (`OpenFolderPicker`, `app.rs:3498`) |
//! | Configuración de Audio / Ecualizador / Efectos de Audio | toggle the Audio Center on tab 0 / 1 / 2 (`ToggleAudioCenter`) |
//! | Salir | close the window (`PlayerWindowAction(Close)`) |
//!
//! The other nine entries do nothing in Iced but close the menu
//! (`Message::ToggleMenu`): they are stubs, kept as stubs, shown disabled.
//!
//! The menu opens below the menu button, anchored to it (quirk fix 7): Iced
//! places it 15 px from the left and 35 px from the top of the player
//! (`player.rs:513`), which is just under the button. It closes on Escape
//! (quirk fix 4), on a click outside, and after an entry runs, as Iced's
//! `GlobalClick` and `ToggleMenu` do.
//!
//! Choosing "Abrir Carpeta" opens the platform's folder dialog without
//! blocking the UI (quirk fix 5); a scan that cannot start shows an error
//! toast as well as the log line (quirk fix 8).

use std::path::Path;

use teksilo::core::overlay::OverlayPlacement;
use teksilo::prelude::*;
use teksilo::widgets::{OverlayTrigger, PopoverCustom};

use super::geometry::MENU_ICON_SIZE;
use super::player_vm::PlayerViewModel;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::widgets::menu::MenuEntry;
use crate::teksilo_ui::widgets::{IconAction, PlayerMenu};

/// Starts scanning the folder the user picked. A scan that cannot start is
/// logged and shown as an error toast (quirk fix 8).
pub fn scan(vm: &PlayerViewModel, path: &Path, ctx: &mut EventContext) {
    if let Err(error) = vm.scan_folder(&path.to_string_lossy()) {
        tracing::error!(
            "the folder {} could not be scanned: {error}",
            path.display()
        );
        ctx.show_toast(Toast::error(tr!(player_scan_failed(
            error = error.to_string()
        ))));
    }
}

/// Shows a folder dialog that failed, as a log line and an error toast.
fn dialog_failed(error: &dyn std::fmt::Display, ctx: &mut EventContext) {
    tracing::error!("the folder dialog failed: {error}");
    ctx.show_toast(Toast::error(tr!(player_folder_dialog_failed(
        error = error.to_string()
    ))));
}

/// "Abrir Carpeta": the platform's folder dialog, then [`scan`]. The dialog
/// runs off the UI thread and answers to a callback on it (quirk fix 5).
pub fn pick_folder_and_scan(vm: &PlayerViewModel, ctx: &mut EventContext) {
    let vm = vm.clone();
    let started = ctx.pick_folder(
        FileDialogRequest::pick_folder(),
        move |result, ctx| match result {
            FileDialogResult::Folder(Some(path)) => scan(&vm, &path, ctx),
            FileDialogResult::Error(error) => dialog_failed(&error, ctx),
            _ => {}
        },
    );
    if let Err(error) = started {
        dialog_failed(&error, ctx);
    }
}

/// The entries of the menu, in Iced's order, wired to `vm`.
pub fn entries(vm: &PlayerViewModel) -> Vec<MenuEntry> {
    let open_folder = {
        let vm = vm.clone();
        MenuEntry::item(tr!(player_menu_open_folder()))
            .on_activate(move |ctx| pick_folder_and_scan(&vm, ctx))
    };
    let audio = |label: LocalizedString, open: fn(&PlayerViewModel)| {
        let vm = vm.clone();
        MenuEntry::item(label).on_activate(move |_| open(&vm))
    };
    vec![
        // TODO(iced-stub): Iced's "Abrir Archivo" only closes the menu (`player.rs:546`).
        MenuEntry::item(tr!(player_menu_open_file())).enabled(false),
        open_folder,
        // TODO(iced-stub): Iced's "Biblioteca" only closes the menu (`player.rs:548`).
        MenuEntry::item(tr!(player_menu_library())).enabled(false),
        // TODO(iced-stub): Iced's "Lista de Reproducción" only closes the menu (`player.rs:549`).
        MenuEntry::item(tr!(player_menu_playlist())).enabled(false),
        // TODO(iced-stub): Iced's "Lyrics" (spelt "Lirycs") only closes the menu (`player.rs:550`).
        MenuEntry::item(tr!(player_menu_lyrics())).enabled(false),
        MenuEntry::divider(),
        audio(
            tr!(player_menu_audio_settings()),
            PlayerViewModel::open_audio_config,
        ),
        audio(
            tr!(player_menu_equalizer()),
            PlayerViewModel::open_equalizer,
        ),
        audio(tr!(player_menu_effects()), PlayerViewModel::open_effects),
        MenuEntry::divider(),
        // TODO(iced-stub): Iced's "Apagado Automático" only closes the menu (`player.rs:567`).
        MenuEntry::item(tr!(player_menu_auto_shutdown())).enabled(false),
        // TODO(iced-stub): Iced's "Personalización" only closes the menu (`player.rs:568`).
        MenuEntry::item(tr!(player_menu_customization())).enabled(false),
        // TODO(iced-stub): Iced's "Preferencias" only closes the menu (`player.rs:569`).
        MenuEntry::item(tr!(player_menu_preferences())).enabled(false),
        // TODO(iced-stub): Iced's "Complementos" only closes the menu (`player.rs:570`).
        MenuEntry::item(tr!(player_menu_plugins())).enabled(false),
        // TODO(iced-stub): Iced's "Acerca" only closes the menu (`player.rs:571`).
        MenuEntry::item(tr!(player_menu_about())).enabled(false),
        // The guarded close path saves the session and stops the backend
        // (quirk fix 3).
        MenuEntry::item(tr!(player_menu_quit())).on_activate(|ctx| ctx.close_window()),
    ]
}

/// The 31 px `menu.svg` button (`player.rs:125`) with the popover it opens
/// below it. Where the button sits in the row is the caller's concern.
pub fn button(vm: &PlayerViewModel) -> PopoverCustom {
    let icon = IconAction::new(
        icons::player::menu(),
        MENU_ICON_SIZE,
        tr!(player_menu_button()),
    )
    // An icon over the cover: Iced draws the SVG's own white.
    .color(Color::WHITE);
    let open = vm.clone();
    let close = vm.clone();
    PopoverCustom::new(OverlayTrigger::around(icon))
        .content(PlayerMenu::new(entries(vm), tr!(player_menu_label())))
        .bare()
        .placement(OverlayPlacement::Below)
        .on_open(move || open.set_menu_open(true))
        .on_close(move || close.set_menu_open(false))
}
