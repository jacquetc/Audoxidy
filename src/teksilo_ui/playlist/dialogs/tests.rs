//! The dialogs and pickers, presented from a real event context.

use std::cell::RefCell;
use std::rc::Rc;

use teksilo::core::WidgetTree;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::frontend::commands::playlist;
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

/// Presents the dialog `open` opens, builds it as the window manager would,
/// then closes it with Escape: the key to the dialog, then the dismissal the
/// framework runs for a modal that Escape may close. Returns the file dialog
/// requests left pending.
fn escaped(env: &Env, open: impl Fn(&mut EventContext, &PlaylistViewModel) + 'static) -> usize {
    use teksilo::core::modal::{ModalCloseBehavior, ModalContent};
    use teksilo::core::overlay::DismissReason;
    use teksilo::core::{Key, Modifiers};

    let (mut tree, _, dialogs) = tree(env);
    let vm = env.vm.clone();
    press(&mut tree, move |ctx| open(ctx, &vm));
    let queued = tree
        .drain_pending_modal_requests()
        .pop()
        .expect("a dialog was presented");
    assert!(
        matches!(
            queued.request.close_behavior,
            ModalCloseBehavior::EscapeKey | ModalCloseBehavior::EscapeOrClickOutside
        ),
        "Escape closes it: {:?}",
        queued.request.close_behavior
    );
    let on_dismiss = queued.request.on_dismiss.clone();
    let content = match queued.request.content {
        ModalContent::Deferred(build) => build(&mut tree),
        ModalContent::ExistingWidget(id) => id,
    };
    layout(&mut tree);
    if let Some(field) =
        crate::teksilo_ui::test_support::first_of_type(&tree, content, "TextInputField")
    {
        tree.focus(field);
    }
    tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut tree);
    if let Some(on_dismiss) = on_dismiss {
        press(&mut tree, move |ctx| on_dismiss(DismissReason::Escape, ctx));
    }
    env.services.runtime.tick();
    dialogs.pending_count()
}

#[test]
fn quirk_4_escape_closes_the_create_dialog_without_creating() {
    let env = restored();
    let before = playlist::list_playlists(&env.services.ctx)
        .recv_blocking()
        .unwrap()
        .len();
    escaped(&env, |ctx, vm| open_create(ctx, vm, Vec::new()));
    env.services
        .ctx
        .db
        .read(|_| Ok(()))
        .recv_blocking()
        .unwrap();
    let after = playlist::list_playlists(&env.services.ctx)
        .recv_blocking()
        .unwrap()
        .len();
    assert_eq!(after, before);
}

#[test]
fn quirk_4_escape_closes_the_rename_dialog_without_renaming() {
    let env = restored();
    let bowie = env.playlist_id("David Bowie");
    escaped(&env, move |ctx, vm| {
        open_rename(ctx, vm, bowie, "David Bowie")
    });
    env.services
        .ctx
        .db
        .read(|_| Ok(()))
        .recv_blocking()
        .unwrap();
    let names: Vec<String> = playlist::list_playlists(&env.services.ctx)
        .recv_blocking()
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert!(names.contains(&"David Bowie".to_string()), "{names:?}");
}

#[test]
fn quirk_4_escape_closes_the_delete_confirmation_without_deleting() {
    let env = restored();
    let bowie = env.playlist_id("David Bowie");
    escaped(&env, move |ctx, vm| {
        open_delete(ctx, vm, bowie, "David Bowie")
    });
    env.services
        .ctx
        .db
        .read(|_| Ok(()))
        .recv_blocking()
        .unwrap();
    assert!(
        playlist::list_playlists(&env.services.ctx)
            .recv_blocking()
            .unwrap()
            .iter()
            .any(|p| p.id == bowie)
    );
}

#[test]
fn quirk_4_escape_closes_the_export_dialog_without_asking_where() {
    let env = restored();
    let bowie = env.playlist_id("David Bowie");
    let pending = escaped(&env, move |ctx, vm| {
        open_export(
            ctx,
            vm,
            bowie,
            "David Bowie",
            ExportFormat::M3U8,
            ExportMode::SingleFile,
        );
    });
    assert_eq!(pending, 0, "no file dialog");
    assert!(env.vm.pending_ops().is_empty(), "no export");
}
