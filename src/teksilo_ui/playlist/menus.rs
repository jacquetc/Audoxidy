//! The playlist's menus, as entries for `widgets::menu::context_menu`.
//!
//! - [`item_menu`]: a right-click on a row (`get_item_context_menu_entries`,
//!   `src/gui/playlist.rs:1034`): twelve commands in four blocks. Three are
//!   Iced stubs and stay **disabled**: "Información", "Editor de Etiquetas"
//!   and "Convertidor de audio" (`src/gui/playlist.rs:1066`, `:1091`,
//!   `:1096`, all `Message::NoOp`). "Enviar a otra lista" is a submenu
//!   ([`send_to_menu`]);
//! - [`empty_list_menu`]: a right-click on the list's background
//!   (`get_empty_playlist_context_menu_entries`, `src/gui/playlist.rs:1123`);
//! - [`tab_menu`]: a right-click on a tab (`src/gui/playlist.rs:451-536`).
//!   "Eliminar lista" is disabled on the system playlists (quirk fix 16);
//! - [`bar_menu`]: a right-click on the tab strip's background
//!   (`src/gui/playlist.rs:589-604`);
//! - [`overflow_menu`]: the chevron's list of every playlist
//!   (`PlaylistShowAllTabs`, `src/gui/app.rs:2675`).
//!
//! Context menus open at the pointer, the overflow menu under its chevron
//! (quirk fix 7); Escape closes them (quirk fix 4).

use teksilo::prelude::*;

use super::dialogs::{self, toast_window};
use super::playlist_vm::PlaylistViewModel;
use crate::logic::playlist_export::{ExportFormat, ExportMode};
use crate::teksilo_ui::icons;
use crate::teksilo_ui::models::PlaylistTab;
use crate::teksilo_ui::widgets::MenuEntry;

/// The menu of the row `linear_idx`.
pub fn item_menu(vm: &PlaylistViewModel, linear_idx: usize) -> Vec<MenuEntry> {
    let (play, location, library, send, toggle, fold, remove) = (
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
    );
    let (files, folder) = (vm.clone(), vm.clone());
    vec![
        MenuEntry::item(tr!(playlist_menu_play()))
            .icon(icons::playlist::play_circle_outlined)
            .on_activate(move |_| play.play_item(linear_idx)),
        MenuEntry::divider(),
        MenuEntry::item(tr!(playlist_menu_add_files()))
            .icon(icons::playlist::audio_file_outlined)
            .on_activate(move |ctx| {
                let _ = dialogs::pick_files(ctx, &files);
            }),
        MenuEntry::item(tr!(playlist_menu_add_folder()))
            .icon(icons::playlist::folder_add_outlined)
            .on_activate(move |ctx| {
                let _ = dialogs::pick_folder(ctx, &folder);
            }),
        MenuEntry::divider(),
        // TODO(iced-stub): "Información" sends `Message::NoOp` in the Iced UI (src/gui/playlist.rs:1066).
        MenuEntry::item(tr!(playlist_menu_info()))
            .icon(icons::playlist::info_outlined)
            .enabled(false),
        MenuEntry::item(tr!(playlist_menu_file_location()))
            .icon(icons::shared::folder_open_outlined)
            .on_activate(move |ctx| location.show_file_location(linear_idx, toast_window(ctx))),
        MenuEntry::item(tr!(playlist_menu_show_in_library()))
            .icon(icons::playlist::show_library_outlined)
            .on_activate(move |_| library.show_in_library(linear_idx)),
        MenuEntry::divider(),
        MenuEntry::submenu(tr!(playlist_menu_send_to()), move || {
            send_to_menu(&send, linear_idx)
        })
        .icon(icons::playlist::send),
        // TODO(iced-stub): "Editor de Etiquetas" sends `Message::NoOp` in the Iced UI (src/gui/playlist.rs:1091).
        MenuEntry::item(tr!(playlist_menu_tag_editor()))
            .icon(icons::playlist::edit_tag)
            .enabled(false),
        // TODO(iced-stub): "Convertidor de audio" sends `Message::NoOp` in the Iced UI (src/gui/playlist.rs:1096).
        MenuEntry::item(tr!(playlist_menu_converter()))
            .icon(icons::playlist::convert)
            .enabled(false),
        MenuEntry::divider(),
        MenuEntry::item(tr!(playlist_menu_toggle_enabled()))
            .icon(icons::playlist::indicator_outlined)
            .on_activate(move |_| toggle.toggle_enabled(linear_idx)),
        MenuEntry::item(tr!(playlist_menu_fold()))
            .icon(icons::playlist::expand_collapse)
            .on_activate(move |_| fold.toggle_all_groups()),
        MenuEntry::item(tr!(playlist_menu_remove()))
            .icon(icons::playlist::delete_outlined)
            .on_activate(move |_| {
                let _ = remove.remove(linear_idx);
            }),
    ]
}

/// The "Enviar a otra lista" submenu of the row `linear_idx`
/// (`PlaylistRequestSubMenu`, `src/gui/app.rs:3999`): "Nueva lista", a
/// divider, the system playlists, then the user playlists sorted
/// case-insensitively, the active playlist left out (quirk fix 58). Iced
/// swaps the row menu for this list with an "Atrás" entry; here it opens
/// beside the menu.
pub fn send_to_menu(vm: &PlaylistViewModel, linear_idx: usize) -> Vec<MenuEntry> {
    let new_list = vm.clone();
    let mut entries = vec![
        MenuEntry::item(tr!(playlist_menu_new_list()))
            .icon(icons::shared::playlist_add)
            .on_activate(move |ctx| {
                let songs = new_list.send_targets(linear_idx);
                dialogs::open_create(ctx, &new_list, songs);
            }),
        MenuEntry::divider(),
    ];
    entries.extend(vm.send_to_lists().into_iter().map(|tab| {
        let vm = vm.clone();
        MenuEntry::item(lit!(tab.name.clone()))
            .icon(icons::app::playlist_music)
            .on_activate(move |_| {
                let _ = vm.send_to(linear_idx, tab.id);
            })
    }));
    entries
}

/// The menu of the list's background.
pub fn empty_list_menu(vm: &PlaylistViewModel) -> Vec<MenuEntry> {
    let (files, folder) = (vm.clone(), vm.clone());
    vec![
        MenuEntry::item(tr!(playlist_menu_add_files()))
            .icon(icons::playlist::audio_file_outlined)
            .on_activate(move |ctx| {
                let _ = dialogs::pick_files(ctx, &files);
            }),
        MenuEntry::item(tr!(playlist_menu_add_folder()))
            .icon(icons::playlist::folder_add_outlined)
            .on_activate(move |ctx| {
                let _ = dialogs::pick_folder(ctx, &folder);
            }),
    ]
}

/// The menu of the tab of `tab`.
pub fn tab_menu(vm: &PlaylistViewModel, tab: &PlaylistTab) -> Vec<MenuEntry> {
    let id = tab.id;
    let (play, create, rename, delete, save, import, export) = (
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
        vm.clone(),
    );
    let (rename_name, delete_name, save_name, export_name) = (
        tab.name.clone(),
        tab.name.clone(),
        tab.name.clone(),
        tab.name.clone(),
    );
    vec![
        MenuEntry::item(tr!(playlist_menu_play()))
            .icon(icons::playlist::playlist_play)
            .on_activate(move |_| play.switch_playlist(id, true)),
        MenuEntry::item(tr!(playlist_menu_new_list()))
            .icon(icons::shared::playlist_add)
            .on_activate(move |ctx| dialogs::open_create(ctx, &create, Vec::new())),
        MenuEntry::item(tr!(playlist_menu_rename()))
            .icon(icons::playlist::playlist_edit)
            .icon_shift(1.0)
            .on_activate(move |ctx| dialogs::open_rename(ctx, &rename, id, &rename_name)),
        MenuEntry::item(tr!(playlist_menu_delete_list()))
            .icon(icons::playlist::playlist_remove)
            .enabled(tab.can_delete())
            .on_activate(move |ctx| dialogs::open_delete(ctx, &delete, id, &delete_name)),
        MenuEntry::divider(),
        MenuEntry::item(tr!(playlist_menu_save()))
            .icon(icons::playlist::playlist_add_check)
            .on_activate(move |ctx| {
                dialogs::open_export(
                    ctx,
                    &save,
                    id,
                    &save_name,
                    ExportFormat::M3U8,
                    ExportMode::SingleFile,
                );
            }),
        MenuEntry::item(tr!(playlist_menu_import()))
            .icon(icons::playlist::import)
            .on_activate(move |ctx| {
                let _ = dialogs::pick_import(ctx, &import);
            }),
        MenuEntry::item(tr!(playlist_menu_export()))
            .icon(icons::playlist::export)
            .on_activate(move |ctx| {
                dialogs::open_export(
                    ctx,
                    &export,
                    id,
                    &export_name,
                    ExportFormat::M3U8,
                    ExportMode::PortableFolder,
                );
            }),
    ]
}

/// The menu of the tab strip's background.
pub fn bar_menu(vm: &PlaylistViewModel) -> Vec<MenuEntry> {
    let (create, import) = (vm.clone(), vm.clone());
    vec![
        MenuEntry::item(tr!(playlist_menu_new_list()))
            .icon(icons::shared::playlist_add)
            .on_activate(move |ctx| dialogs::open_create(ctx, &create, Vec::new())),
        MenuEntry::item(tr!(playlist_menu_import()))
            .icon(icons::playlist::import)
            .on_activate(move |ctx| {
                let _ = dialogs::pick_import(ctx, &import);
            }),
    ]
}

/// The overflow chevron's menu: "Nueva lista", a divider, the system
/// playlists ("Audoxidy", "Oxidy Drift"), then the user playlists in tab
/// order.
pub fn overflow_menu(vm: &PlaylistViewModel) -> Vec<MenuEntry> {
    let create = vm.clone();
    let mut entries = vec![
        MenuEntry::item(tr!(playlist_menu_new_list()))
            .icon(icons::shared::playlist_add)
            .on_activate(move |ctx| dialogs::open_create(ctx, &create, Vec::new())),
        MenuEntry::divider(),
    ];
    entries.extend(vm.overflow_lists().into_iter().map(|tab| {
        let vm = vm.clone();
        MenuEntry::item(lit!(tab.name.clone()))
            .icon(icons::app::playlist_music)
            .on_activate(move |_| vm.switch_playlist(tab.id, false))
    }));
    entries
}

#[cfg(test)]
mod tests;
