//! The playlist's dialogs and file pickers.
//!
//! The Iced dialogs (`ActiveDialog`, `src/gui/app.rs:349`, drawn at
//! `src/gui/app.rs:5242-5330`):
//!
//! - **create** ("Nueva lista de reproducción"): a name, "Crear"; a blank
//!   name gets the default "Nueva lista n". From "Enviar a otra lista → Nueva
//!   lista" it carries the songs to put in the new list;
//! - **rename** ("Renombrar lista"): the field starts with the current name;
//!   a system playlist gets a renamed copy (`RenameOutcome::Copied`); a blank
//!   name does nothing;
//! - **delete** ("¿Eliminar lista?"), a confirmation;
//! - **export** ("Guardar | Exportar lista"): the format (M3U, M3U8) and the
//!   mode (list only, portable folder), then "Continuar" opens the save
//!   dialog (list only) or the folder picker (portable), and the export runs
//!   as a long operation that ends in a toast.
//!
//! The create and rename dialogs are the stock `InputDialog` in the Audoxidy
//! look, and the delete confirmation Iced's `standard_modal`
//! (`widgets::dialog_frame::confirm_box`, a `DialogFrame` in which Enter does
//! nothing, as Iced's dialog takes no key); the export dialog is the Iced column itself
//! ([`DialogFrame`]). Escape cancels each of them (quirk fix 4).
//!
//! The file pickers (`PlaylistAddFiles`, `PlaylistAddFolder`,
//! `OpenPlaylistFilePicker`, `src/gui/app.rs:3511`, `:3587`, `:3612`) use the
//! framework's asynchronous file dialogs, so nothing blocks the UI thread
//! (quirk fix 5); what they pick starts a long operation.

use std::path::PathBuf;

use teksilo::core::accesskit::Role;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    FixedSize, HStack, Padding, RectWidget, Spacer, StandardButton, TextWidget, VStack, ZStack,
};

use super::playlist_vm::{PlaylistViewModel, ToastWindow};
use crate::logic::playlist_export::{ExportFormat, ExportMode};
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::dialog_frame::{
    BUTTON_PADDING, BUTTON_RADIUS, TEXT_SIZE, confirm_box, input_dialog,
};
use crate::teksilo_ui::widgets::{DialogFrame, ModalText, Pressable};

/// The audio extensions of the "Agregar archivo" picker
/// (`src/gui/app.rs:3593`).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "m4a", "aac", "ape", "aiff", "mpc", "opus", "spx", "wv",
];

/// The playlist file extensions (`src/gui/app.rs:3597`).
pub const PLAYLIST_EXTENSIONS: &[&str] = &["m3u", "m3u8"];

/// The space between the export dialog's lines (`src/gui/app.rs:5323`).
pub const EXPORT_SPACING: f32 = 10.0;

/// The window a toast about the result of an action started in `ctx` goes to.
pub fn toast_window(ctx: &EventContext) -> ToastWindow {
    ctx.window().map(|window| window.id())
}

// ---------------------------------------------------------------------------
// Create, rename, delete
// ---------------------------------------------------------------------------

/// The create dialog; `songs` go into the new playlist.
pub fn open_create(ctx: &mut EventContext, vm: &PlaylistViewModel, songs: Vec<i64>) {
    let vm = vm.clone();
    input_dialog(
        tr!(dialogs_create_title()),
        tr!(dialogs_create_prompt()),
        tr!(dialogs_create_confirm()),
    )
    .placeholder(tr!(dialogs_create_placeholder()))
    .on_result(move |name, ctx| {
        if let Some(name) = name {
            vm.create_playlist(&name, songs.clone(), toast_window(ctx));
        }
    })
    .present(ctx);
}

/// The rename dialog for playlist `id`, now called `name`.
pub fn open_rename(ctx: &mut EventContext, vm: &PlaylistViewModel, id: i64, name: &str) {
    let vm = vm.clone();
    input_dialog(
        tr!(dialogs_rename_title()),
        tr!(dialogs_rename_prompt()),
        tr!(dialogs_rename_confirm()),
    )
    .placeholder(tr!(dialogs_rename_placeholder()))
    .default_text(name.to_string())
    .on_result(move |new_name, ctx| {
        if let Some(new_name) = new_name {
            vm.rename_playlist(id, &new_name, toast_window(ctx));
        }
    })
    .present(ctx);
}

/// The delete confirmation for playlist `id`, called `name`.
pub fn open_delete(ctx: &mut EventContext, vm: &PlaylistViewModel, id: i64, name: &str) {
    let vm = vm.clone();
    confirm_box(
        tr!(dialogs_delete_title()),
        tr!(dialogs_delete_text(name = name.to_string())),
        tr!(dialogs_delete_confirm()),
    )
    .on_result(move |result, ctx| {
        if result.button == StandardButton::Ok {
            vm.delete_playlist(id, toast_window(ctx));
        }
    })
    .present(ctx);
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// The export dialog for playlist `id`, called `name`, starting on `format`
/// and `mode` ("Guardar lista": M3U8 and the list only; "Exportar lista":
/// M3U8 and the portable folder, `src/gui/playlist.rs:509-534`).
pub fn open_export(
    ctx: &mut EventContext,
    vm: &PlaylistViewModel,
    id: i64,
    name: &str,
    format: ExportFormat,
    mode: ExportMode,
) {
    let format = Signal::new(format);
    let mode = Signal::new(mode);
    let content = ExportChoices::new(name, format.clone(), mode.clone());
    let vm = vm.clone();
    DialogFrame::new(tr!(dialogs_export_title()), content)
        .on_cancel(|_| {})
        .on_confirm(tr!(dialogs_export_confirm()), move |ctx| {
            let _ = pick_export_target(ctx, &vm, id, format.get(), mode.get());
        })
        .present(ctx);
}

/// Asks where to export (`ConfirmDialogAction`, `src/gui/app.rs:2320`): a
/// file to save for the list only, a folder for the portable export. Then
/// the export runs. `Err` when the dialog could not be opened (it is
/// logged).
pub fn pick_export_target(
    ctx: &mut EventContext,
    vm: &PlaylistViewModel,
    id: i64,
    format: ExportFormat,
    mode: ExportMode,
) -> Result<(), String> {
    let vm = vm.clone();
    let submitted = match mode {
        ExportMode::PortableFolder => ctx.pick_folder(
            FileDialogRequest::pick_folder()
                .title(tr!(dialogs_export_folder_title()).resolve_now()),
            move |result, _ctx| {
                if let Some(dir) = picked(result) {
                    vm.export(id, dir, format, mode);
                }
            },
        ),
        ExportMode::SingleFile => ctx.save_file(
            FileDialogRequest::save_file()
                .title(tr!(dialogs_export_save_title()).resolve_now())
                .add_filter(
                    tr!(dialogs_playlist_filter()).resolve_now(),
                    &[format.extension()],
                ),
            move |result, _ctx| {
                if let Some(path) = picked(result) {
                    vm.export(id, path, format, mode);
                }
            },
        ),
    };
    log_refused(submitted)
}

/// The format and mode choices of the export dialog, under the Iced text
/// (`src/gui/app.rs:5305-5329`).
pub struct ExportChoices {
    name: String,
    format: Signal<ExportFormat>,
    mode: Signal<ExportMode>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ExportChoices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportChoices")
            .field("name", &self.name)
            .field("format", &self.format.get())
            .field("mode", &self.mode.get())
            .finish_non_exhaustive()
    }
}

impl ExportChoices {
    /// The choices for the playlist `name`, bound to `format` and `mode`.
    pub fn new(name: &str, format: Signal<ExportFormat>, mode: Signal<ExportMode>) -> Self {
        Self {
            name: name.to_string(),
            format,
            mode,
            root: None,
        }
    }
}

/// One choice of a row: the Iced `export_choice_button`
/// (`src/gui/app.rs:5753`): Inter Medium 14 px, 5 × 10 px of padding, a
/// 6 px radius; accent with a primary label when chosen or hovered,
/// `COLOR_CONTRAST` with a secondary label otherwise.
fn choice<T: Copy + PartialEq + 'static>(
    label: LocalizedString,
    value: T,
    chosen: &Signal<T>,
) -> Pressable {
    let hovered = Signal::new(false);
    let lit = chosen
        .zip(&hovered)
        .map(move |(current, hovered)| *current == value || *hovered);
    let selected = chosen.map(move |current| *current == value);
    let body = ZStack::new()
        .child(
            RectWidget::new()
                .background(lit.map(|lit| choice_fill(*lit)))
                .corner_radius(CornerRadius::uniform(BUTTON_RADIUS)),
        )
        .child(
            Padding::symmetric(BUTTON_PADDING.0, BUTTON_PADDING.1).child(
                TextWidget::new(label.clone())
                    .style(style::inter(TEXT_SIZE, FontWeight::MEDIUM))
                    .color(lit.map(|lit| choice_text(*lit)))
                    .single_line()
                    .a11y_hidden(),
            ),
        );
    let chosen = chosen.clone();
    Pressable::new(body, label)
        .hovered(hovered)
        .tab(selected)
        .corner_radius(BUTTON_RADIUS)
        .on_activate(move |_| chosen.set(value))
}

/// A choice's fill: the accent when chosen or hovered, `COLOR_CONTRAST`
/// otherwise.
pub fn choice_fill(lit: bool) -> SurfaceRole {
    if lit {
        SurfaceRole::Accent
    } else {
        SurfaceRole::Raised
    }
}

/// A choice's label colour: primary when chosen or hovered, secondary
/// otherwise.
pub fn choice_text(lit: bool) -> TextRole {
    if lit {
        TextRole::Primary
    } else {
        TextRole::Secondary
    }
}

/// A row of choices 10 px apart (`src/gui/app.rs:5310`), announced as a
/// radio group named `label`.
fn choice_row(label: LocalizedString, choices: Vec<Pressable>) -> impl Widget {
    choices
        .into_iter()
        .fold(
            HStack::new()
                .spacing(EXPORT_SPACING)
                .alignment(VAlignment::Center),
            |row, choice| row.child(choice.access_role(Role::RadioButton)),
        )
        .access_role(Role::RadioGroup)
        .access_label(label)
}

/// An empty line of the column (`Space::new().height(0)`).
fn nothing() -> FixedSize {
    FixedSize::new().width(0.0).height(0.0).child(Spacer::new())
}

impl Widget for ExportChoices {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let format_choices = choice_row(
            tr!(dialogs_export_format()),
            vec![
                choice(lit!("M3U"), ExportFormat::M3U, &self.format),
                choice(lit!("M3U8"), ExportFormat::M3U8, &self.format),
            ],
        );
        let mode_choices = choice_row(
            tr!(dialogs_export_mode()),
            vec![
                choice(
                    tr!(dialogs_export_single()),
                    ExportMode::SingleFile,
                    &self.mode,
                ),
                choice(
                    tr!(dialogs_export_portable()),
                    ExportMode::PortableFolder,
                    &self.mode,
                ),
            ],
        );
        let root = ctx.add(
            VStack::new()
                .spacing(EXPORT_SPACING)
                .alignment(HAlignment::Center)
                .child(ModalText::new(tr!(dialogs_export_intro(
                    name = self.name.clone()
                ))))
                .child(nothing())
                .child(ModalText::new(tr!(dialogs_export_format())))
                .child(format_choices)
                .child(nothing())
                .child(ModalText::new(tr!(dialogs_export_mode())))
                .child(mode_choices),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// File pickers
// ---------------------------------------------------------------------------

/// The path a file dialog picked, if any. An error is logged.
fn picked(result: FileDialogResult) -> Option<PathBuf> {
    match result {
        FileDialogResult::File(path)
        | FileDialogResult::Folder(path)
        | FileDialogResult::Saved(path) => path,
        FileDialogResult::Files(paths) => paths.into_iter().next(),
        FileDialogResult::Error(error) => {
            tracing::warn!("the file dialog failed: {error}");
            None
        }
    }
}

/// Logs a file dialog the framework refused to open, and says whether it
/// opened.
fn log_refused<T>(submitted: Result<T, String>) -> Result<(), String> {
    match submitted {
        Ok(_) => Ok(()),
        Err(error) => {
            tracing::warn!("the file dialog could not be opened: {error}");
            Err(error)
        }
    }
}

/// "Agregar archivo": audio files and playlists, several at once, added to
/// the active playlist (`PlaylistAddFiles`, `src/gui/app.rs:3587`). `Err`
/// when the dialog could not be opened (it is logged).
pub fn pick_files(ctx: &mut EventContext, vm: &PlaylistViewModel) -> Result<(), String> {
    let vm = vm.clone();
    let submitted = ctx.pick_files(
        FileDialogRequest::pick_files()
            .add_filter(tr!(dialogs_audio_filter()).resolve_now(), AUDIO_EXTENSIONS)
            .add_filter(
                tr!(dialogs_lists_filter()).resolve_now(),
                PLAYLIST_EXTENSIONS,
            ),
        move |result, _ctx| match result {
            FileDialogResult::Files(paths) if !paths.is_empty() => {
                let _ = vm.add_external(paths);
            }
            other => {
                let _ = picked(other);
            }
        },
    );
    log_refused(submitted)
}

/// "Agregar carpeta": a folder, added to the active playlist
/// (`PlaylistAddFolder`, `src/gui/app.rs:3612`). `Err` when the dialog could
/// not be opened (it is logged).
pub fn pick_folder(ctx: &mut EventContext, vm: &PlaylistViewModel) -> Result<(), String> {
    let vm = vm.clone();
    let submitted = ctx.pick_folder(FileDialogRequest::pick_folder(), move |result, _ctx| {
        if let Some(folder) = picked(result) {
            let _ = vm.add_external(vec![folder]);
        }
    });
    log_refused(submitted)
}

/// "Importar lista": an M3U/M3U8 file, imported as a new playlist that then
/// plays (`OpenPlaylistFilePicker`, `src/gui/app.rs:3511`). `Err` when the
/// dialog could not be opened (it is logged).
pub fn pick_import(ctx: &mut EventContext, vm: &PlaylistViewModel) -> Result<(), String> {
    let vm = vm.clone();
    let submitted = ctx.pick_file(
        FileDialogRequest::pick_file().add_filter(
            tr!(dialogs_playlist_filter()).resolve_now(),
            PLAYLIST_EXTENSIONS,
        ),
        move |result, _ctx| {
            if let Some(path) = picked(result) {
                let _ = vm.import(path);
            }
        },
    );
    log_refused(submitted)
}

#[cfg(test)]
mod tests;
