//! The menus over the mock library ("Audoxidy", "Oxidy Drift", "David
//! Bowie"), mounted as the context menus mount them.

use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::platform::file_dialog::FileDialogHandle;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::frontend::commands::playlist;
use crate::teksilo_ui::playlist::testing::{Env, menu_entries, menus, restored, tree};
use crate::teksilo_ui::test_support::{drain_dialog_titles, layout, with_shipped_messages};
use crate::teksilo_ui::widgets::menu::context_menu;

/// The labels of `entries`, a divider as "—".
fn labels(entries: &[MenuEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| {
            if entry.is_divider() {
                "—".to_string()
            } else {
                entry.label().resolve_now()
            }
        })
        .collect()
}

/// `entries` mounted as a context menu, in a tree with the app's services.
fn mounted(env: &Env, entries: Vec<MenuEntry>) -> (WidgetTree, FileDialogHandle) {
    let (mut tree, _, dialogs) = tree(env);
    let menu = tree.add(context_menu(entries));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(menu)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    (tree, dialogs)
}

fn activate(tree: &mut WidgetTree, label: &str) {
    let id = tree
        .find_by_label(label)
        .unwrap_or_else(|| panic!("no entry {label}"));
    tree.click(id);
    layout(tree);
}

fn tab(env: &Env, name: &str) -> PlaylistTab {
    env.services
        .playlist_tabs
        .tab(env.playlist_id(name))
        .unwrap()
}

#[test]
fn the_row_menu_has_iceds_twelve_commands_in_four_blocks() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        assert_eq!(
            labels(&item_menu(&env.vm, 3)),
            vec![
                "Reproducir",
                "—",
                "Agregar archivo",
                "Agregar carpeta",
                "—",
                "Información",
                "Ubicación del archivo",
                "Mostrar en la biblioteca",
                "—",
                "Enviar a otra lista",
                "Editor de Etiquetas",
                "Convertidor de audio",
                "—",
                "Activar | Desactivar",
                "Plegar | Desplegar",
                "Eliminar de la lista",
            ]
        );
    });
}

#[test]
fn the_send_to_submenu_lists_new_then_the_system_lists_then_the_rest_sorted() {
    let env = restored();
    for name in ["mixtape", "Baladas"] {
        playlist::create_playlist(&env.services.ctx, name, Vec::new())
            .recv_blocking()
            .unwrap();
    }
    env.reload_tabs();
    with_shipped_messages("es-ES", || {
        assert_eq!(
            labels(&send_to_menu(&env.vm, 3)),
            vec![
                "Nueva lista",
                "—",
                "Audoxidy",
                "Oxidy Drift",
                "Baladas",
                "David Bowie",
                "mixtape"
            ]
        );
    });
}

#[test]
fn a_playlist_of_the_send_to_submenu_receives_the_songs() {
    let env = restored();
    env.vm.menu_opened_on(3);
    let song = env.song_id(3);
    let (mut tree, _) = mounted(&env, send_to_menu(&env.vm, 3));
    activate(&mut tree, "Oxidy Drift");
    let drift = env.playlist_id("Oxidy Drift");
    env.wait(|| {
        playlist::load_playlist(&env.services.ctx, drift)
            .recv_blocking()
            .is_ok_and(|loaded| {
                loaded
                    .groups
                    .iter()
                    .any(|g| g.songs.iter().any(|s| s.song_id == song))
            })
    });
}

#[test]
fn new_list_in_the_send_to_submenu_opens_the_create_dialog() {
    let env = restored();
    let (mut tree, _) = mounted(&env, send_to_menu(&env.vm, 3));
    activate(&mut tree, "New playlist");
    assert_eq!(drain_dialog_titles(&mut tree), vec!["New playlist"]);
}

#[test]
fn the_tab_menu_disables_eliminar_lista_on_a_system_playlist() {
    // Quirk fix 16.
    let env = restored();
    with_shipped_messages("es-ES", || {
        let system = tab(&env, "Audoxidy");
        assert_eq!(
            labels(&tab_menu(&env.vm, &system)),
            vec![
                "Reproducir",
                "Nueva lista",
                "Renombrar lista",
                "Eliminar lista",
                "—",
                "Guardar lista",
                "Importar lista",
                "Exportar lista",
            ]
        );
        let (tree, _) = mounted(&env, tab_menu(&env.vm, &system));
        let menu = menus(&tree)[0];
        let entries = menu_entries(&tree, menu);
        assert!(entries.contains(&("Eliminar lista".to_string(), false)));
        assert_eq!(entries.iter().filter(|(_, enabled)| !enabled).count(), 1);
        let user = tab(&env, "David Bowie");
        let (tree, _) = mounted(&env, tab_menu(&env.vm, &user));
        let menu = menus(&tree)[0];
        assert!(
            menu_entries(&tree, menu)
                .iter()
                .all(|(_, enabled)| *enabled)
        );
        assert_eq!(tree.accessibility_node(menu).role(), Role::Menu);
    });
}

#[test]
fn the_tab_menu_opens_its_dialogs() {
    let env = restored();
    let bowie = tab(&env, "David Bowie");
    for (label, title) in [
        ("New playlist", "New playlist"),
        ("Rename playlist", "Rename playlist"),
        ("Delete playlist", "Delete playlist?"),
        ("Save playlist", "Save | Export playlist"),
        ("Export playlist", "Save | Export playlist"),
    ] {
        let (mut tree, _) = mounted(&env, tab_menu(&env.vm, &bowie));
        activate(&mut tree, label);
        assert_eq!(drain_dialog_titles(&mut tree), vec![title], "{label}");
    }
}

#[test]
fn reproducir_in_the_tab_menu_switches_and_plays() {
    let env = restored();
    let audoxidy = tab(&env, "Audoxidy");
    let (mut tree, _) = mounted(&env, tab_menu(&env.vm, &audoxidy));
    activate(&mut tree, "Play");
    env.wait(|| env.vm.active_playlist().get() == Some(audoxidy.id));
    assert_eq!(env.vm.playing_index().get(), Some(1));
}

#[test]
fn importar_lista_asks_for_a_file() {
    let env = restored();
    let (mut tree, dialogs) = mounted(&env, bar_menu(&env.vm));
    activate(&mut tree, "Import playlist");
    assert_eq!(dialogs.pending_count(), 1);
}

#[test]
fn the_add_entries_ask_for_files_and_a_folder() {
    let env = restored();
    let (mut tree, dialogs) = mounted(&env, empty_list_menu(&env.vm));
    activate(&mut tree, "Add file");
    let (mut tree2, dialogs2) = mounted(&env, empty_list_menu(&env.vm));
    activate(&mut tree2, "Add folder");
    assert_eq!(dialogs.pending_count() + dialogs2.pending_count(), 2);
}

#[test]
fn the_bar_and_empty_list_menus_are_iceds() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        assert_eq!(
            labels(&bar_menu(&env.vm)),
            vec!["Nueva lista", "Importar lista"]
        );
        assert_eq!(
            labels(&empty_list_menu(&env.vm)),
            vec!["Agregar archivo", "Agregar carpeta"]
        );
    });
}

#[test]
fn the_overflow_menu_lists_new_then_the_playlists_and_switches() {
    let env = restored();
    playlist::create_playlist(&env.services.ctx, "Aaa", Vec::new())
        .recv_blocking()
        .unwrap();
    env.reload_tabs();
    with_shipped_messages("es-ES", || {
        assert_eq!(
            labels(&overflow_menu(&env.vm)),
            vec![
                "Nueva lista",
                "—",
                "Audoxidy",
                "Oxidy Drift",
                "David Bowie",
                "Aaa"
            ],
            "the user lists in tab order"
        );
    });
    let (mut tree, _) = mounted(&env, overflow_menu(&env.vm));
    activate(&mut tree, "Audoxidy");
    let audoxidy = env.playlist_id("Audoxidy");
    env.wait(|| env.vm.active_playlist().get() == Some(audoxidy));
    assert_eq!(env.vm.playing_index().get(), None, "no autoplay");
}
