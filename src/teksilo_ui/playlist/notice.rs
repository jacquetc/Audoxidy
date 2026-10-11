//! What the user is told when something they started in the playlist ends:
//! the toasts of [`Notice`].
//!
//! The Iced UI tells the user nothing: an export, an import or an added folder
//! that fails is only logged (quirk fix 8). The Teksilo UI shows an error
//! toast as well, and a success toast when an export has written its list.
//! The app installs the default toast host.

use teksilo::prelude::*;

use super::playlist_vm::Notice;

/// The toast for `notice`.
pub fn toast(notice: &Notice) -> Toast {
    match notice {
        Notice::Exported {
            path,
            copied: _,
            failed: 0,
        } => Toast::success(tr!(playlist_toast_exported())).body(lit!(path.display().to_string())),
        Notice::Exported { failed, .. } => Toast::warning(tr!(playlist_toast_exported_partly()))
            .body(tr!(playlist_toast_copies_failed(count = *failed))),
        Notice::ExportFailed(error) => error_toast(tr!(playlist_toast_export_failed()), error),
        Notice::ImportFailed(error) => error_toast(tr!(playlist_toast_import_failed()), error),
        Notice::AddFilesFailed(error) => error_toast(tr!(playlist_toast_add_failed()), error),
        Notice::CreateFailed(error) => error_toast(tr!(playlist_toast_create_failed()), error),
        Notice::RenameFailed(error) => error_toast(tr!(playlist_toast_rename_failed()), error),
        Notice::DeleteFailed(error) => error_toast(tr!(playlist_toast_delete_failed()), error),
        Notice::FileLocationFailed(error) => {
            error_toast(tr!(playlist_toast_location_failed()), error)
        }
    }
}

/// An error toast titled `title` whose body is the error, as the facade
/// words it.
fn error_toast(title: LocalizedString, error: &str) -> Toast {
    Toast::error(title).body(lit!(error.to_string()))
}

/// Shows `notice` as a toast in the window of `ctx`.
pub fn present(notice: &Notice, ctx: &mut EventContext) {
    ctx.show_toast(toast(notice));
}

#[cfg(test)]
mod tests {
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::path::PathBuf;

    use teksilo::widgets::{ToastInstallOptions, ToastRegistry};

    use super::*;
    use crate::frontend::AppContext;
    use crate::teksilo_ui::test_support::{press, tree_with_app_state, with_shipped_messages};

    fn notices() -> Vec<Notice> {
        vec![
            Notice::Exported {
                path: PathBuf::from("/tmp/list.m3u8"),
                copied: 0,
                failed: 0,
            },
            Notice::Exported {
                path: PathBuf::from("/tmp/list.m3u8"),
                copied: 3,
                failed: 1,
            },
            Notice::ExportFailed("disk full".into()),
            Notice::ImportFailed("no file".into()),
            Notice::AddFilesFailed("no folder".into()),
            Notice::CreateFailed("taken".into()),
            Notice::RenameFailed("taken".into()),
            Notice::DeleteFailed("system".into()),
            Notice::FileLocationFailed("no xdg-open".into()),
        ]
    }

    #[test]
    fn every_notice_is_shown_as_a_toast_in_the_window() {
        let ctx = AppContext::in_memory().unwrap();
        let registry = ToastRegistry::new(ToastInstallOptions {
            archive: None,
            ..ToastInstallOptions::default()
        });
        let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
        state.insert(TypeId::of::<ToastRegistry>(), Box::new(registry.clone()));
        let mut tree = tree_with_app_state(&ctx, state);
        let before = registry.version_signal().get();
        press(&mut tree, |ctx| {
            present(&Notice::ExportFailed("disk full".into()), ctx);
        });
        assert!(
            registry.version_signal().get() > before,
            "the toast reached the registry"
        );
        assert_eq!(registry.live_count(), 1);
        assert!(ctx.shutdown());
    }

    #[test]
    fn the_notices_read_in_spanish() {
        with_shipped_messages("es-ES", || {
            assert_eq!(
                tr!(playlist_toast_export_failed()).resolve_now(),
                "No se pudo exportar la lista"
            );
            assert_eq!(
                tr!(playlist_toast_copies_failed(count = 1)).resolve_now(),
                "No se pudo copiar 1 canción"
            );
            assert_eq!(
                tr!(playlist_toast_copies_failed(count = 3)).resolve_now(),
                "No se pudieron copiar 3 canciones"
            );
            for notice in notices() {
                let _ = toast(&notice);
            }
        });
    }
}
