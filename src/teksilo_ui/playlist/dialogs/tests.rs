//! The dialogs and pickers, presented from a real event context.

use std::cell::RefCell;
use std::rc::Rc;

use teksilo::core::WidgetTree;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::teksilo_ui::playlist::testing::{Env, all_of_type, restored, tree};
use crate::teksilo_ui::test_support::{
    access_click, drain_dialog_titles, find_by_name, layout, press, with_shipped_messages,
};

fn presented(
    env: &Env,
    f: impl Fn(&mut EventContext, &PlaylistViewModel) + 'static,
) -> Vec<String> {
    let (mut tree, _, _) = tree(env);
    let vm = env.vm.clone();
    press(&mut tree, move |ctx| f(ctx, &vm));
    drain_dialog_titles(&mut tree)
}

#[test]
fn the_dialogs_are_titled_as_in_iced() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        assert_eq!(
            presented(&env, |ctx, vm| open_create(ctx, vm, Vec::new())),
            vec!["Nueva lista de reproducción"]
        );
        assert_eq!(
            presented(&env, |ctx, vm| open_rename(ctx, vm, 3, "David Bowie")),
            vec!["Renombrar lista"]
        );
        assert_eq!(
            presented(&env, |ctx, vm| open_delete(ctx, vm, 3, "David Bowie")),
            vec!["¿Eliminar lista?"]
        );
        assert_eq!(
            presented(&env, |ctx, vm| {
                open_export(
                    ctx,
                    vm,
                    3,
                    "David Bowie",
                    ExportFormat::M3U8,
                    ExportMode::SingleFile,
                );
            }),
            vec!["Guardar | Exportar lista"]
        );
    });
}

#[test]
fn the_dialog_texts_are_iceds() {
    with_shipped_messages("es-ES", || {
        assert_eq!(
            tr!(dialogs_delete_text(name = "Mixtape".to_string())).resolve_now(),
            "¿Estás seguro de querer eliminar\nla lista \"Mixtape\"?\nEsta acción no se puede deshacer."
        );
        assert_eq!(
            tr!(dialogs_export_intro(name = "Mixtape".to_string())).resolve_now(),
            "Puedes guardar solo la lista o también\nexportar todas las canciones de la lista\n\"Mixtape\"."
        );
        assert_eq!(
            tr!(dialogs_create_prompt()).resolve_now(),
            "Nombre para la nueva lista:"
        );
        assert_eq!(
            tr!(dialogs_create_placeholder()).resolve_now(),
            "Nombre de la lista..."
        );
        assert_eq!(tr!(dialogs_create_confirm()).resolve_now(), "Crear");
        assert_eq!(
            tr!(dialogs_rename_prompt()).resolve_now(),
            "Nuevo nombre de la lista:"
        );
        assert_eq!(
            tr!(dialogs_rename_placeholder()).resolve_now(),
            "Nuevo nombre..."
        );
        assert_eq!(tr!(dialogs_rename_confirm()).resolve_now(), "Renombrar");
        assert_eq!(tr!(dialogs_delete_confirm()).resolve_now(), "Eliminar");
        assert_eq!(tr!(dialogs_export_confirm()).resolve_now(), "Continuar");
        assert_eq!(
            tr!(dialogs_export_folder_title()).resolve_now(),
            "Seleccionar carpeta de exportación"
        );
        assert_eq!(
            tr!(dialogs_export_save_title()).resolve_now(),
            "Guardar lista de reproducción"
        );
        assert_eq!(
            tr!(dialogs_playlist_filter()).resolve_now(),
            "Lista de reproducción"
        );
        assert_eq!(tr!(dialogs_lists_filter()).resolve_now(), "Listas");
    });
}

/// The export choices for "David Bowie", mounted on their own.
fn choices(
    env: &Env,
) -> (
    WidgetTree,
    WidgetId,
    Signal<ExportFormat>,
    Signal<ExportMode>,
) {
    let format = Signal::new(ExportFormat::M3U8);
    let mode = Signal::new(ExportMode::SingleFile);
    let (mut tree, _, _) = tree(env);
    let id = tree.add(ExportChoices::new(
        "David Bowie",
        format.clone(),
        mode.clone(),
    ));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(id)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    (tree, id, format, mode)
}

#[test]
fn the_export_choices_pick_the_format_and_the_mode() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        let (mut tree, id, format, mode) = choices(&env);
        let m3u = find_by_name(&tree, id, "M3U")[0];
        let portable = find_by_name(&tree, id, "Carpeta Portable")[0];
        assert_eq!(tree.accessibility_node(m3u).role(), Role::RadioButton);
        assert!(!tree.accessibility_node(m3u).is_selected());
        access_click(&mut tree, m3u);
        access_click(&mut tree, portable);
        assert_eq!(format.get(), ExportFormat::M3U);
        assert_eq!(mode.get(), ExportMode::PortableFolder);
        assert!(tree.accessibility_node(m3u).is_selected());
    });
}

#[test]
fn the_export_choices_are_centred_10_pixels_apart() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        let (tree, id, _, _) = choices(&env);
        let column = tree.bounds(id);
        let m3u = tree.bounds(find_by_name(&tree, id, "M3U")[0]);
        let m3u8 = tree.bounds(find_by_name(&tree, id, "M3U8")[0]);
        assert_eq!(m3u8.x - (m3u.x + m3u.width), EXPORT_SPACING);
        let middle = (m3u.x + m3u8.x + m3u8.width) / 2.0;
        assert!((middle - column.center().x).abs() < 0.5);
        let texts = all_of_type(&tree, id, "ModalText");
        assert_eq!(texts.len(), 3);
        let intro = tree.bounds(texts[0]);
        let format_label = tree.bounds(texts[1]);
        // The intro, 10 px, an empty line, 10 px, then "Formato de la lista:".
        assert_eq!(
            format_label.y - (intro.y + intro.height),
            2.0 * EXPORT_SPACING
        );
        assert_eq!(
            m3u.y - (format_label.y + format_label.height),
            EXPORT_SPACING
        );
    });
}

#[test]
fn a_chosen_or_hovered_choice_is_lit() {
    assert_eq!(choice_fill(true), SurfaceRole::Accent);
    assert_eq!(choice_fill(false), SurfaceRole::Raised);
    assert_eq!(choice_text(true), TextRole::Primary);
    assert_eq!(choice_text(false), TextRole::Secondary);
}

/// A file picker of this module.
type Pick = fn(&mut EventContext, &PlaylistViewModel) -> Result<(), String>;

/// Runs `pick` from a real event context and returns what it said.
fn picked_with(
    env: &Env,
    pick: impl Fn(&mut EventContext, &PlaylistViewModel) -> Result<(), String> + 'static,
) -> (Result<(), String>, usize) {
    let (mut tree, _, dialogs) = tree(env);
    let vm = env.vm.clone();
    let result = Rc::new(RefCell::new(None));
    let sink = result.clone();
    press(&mut tree, move |ctx| {
        *sink.borrow_mut() = Some(pick(ctx, &vm));
    });
    let result = result.borrow_mut().take().unwrap();
    (result, dialogs.pending_count())
}

#[test]
fn continuar_asks_where_to_export() {
    let env = restored();
    for mode in [ExportMode::SingleFile, ExportMode::PortableFolder] {
        let (result, pending) = picked_with(&env, move |ctx, vm| {
            pick_export_target(ctx, vm, 3, ExportFormat::M3U8, mode)
        });
        assert_eq!(result, Ok(()), "{mode:?}");
        assert_eq!(pending, 1, "{mode:?}");
    }
}

#[test]
fn the_pickers_ask_the_file_dialog_service() {
    let env = restored();
    let picks: [Pick; 3] = [pick_files, pick_folder, pick_import];
    for pick in picks {
        let (result, pending) = picked_with(&env, pick);
        assert_eq!(result, Ok(()));
        assert_eq!(pending, 1);
    }
}

#[test]
fn a_picked_path_is_taken_and_a_cancel_or_an_error_is_nothing() {
    let path = PathBuf::from("/music/a.m3u");
    assert_eq!(
        picked(FileDialogResult::File(Some(path.clone()))),
        Some(path.clone())
    );
    assert_eq!(
        picked(FileDialogResult::Saved(Some(path.clone()))),
        Some(path.clone())
    );
    assert_eq!(picked(FileDialogResult::Folder(None)), None);
    assert_eq!(picked(FileDialogResult::Error("no portal".into())), None);
}
