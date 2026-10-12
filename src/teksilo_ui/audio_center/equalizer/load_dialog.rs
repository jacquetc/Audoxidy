//! The Load dialog ("Cargar Preset"): the preset list and its six buttons.
//!
//! Port of `ActiveDialog::EqPresetLoad`'s view (`src/gui/app.rs:5332-5551`). A
//! 330 × 352 panel (17 px of padding, a 2 px accent border, an 8 px radius and
//! the dialog shadow) holding the title (Inter Medium 16 px, centred), 8 px
//! below it a 140 px list and, 10 px to its right, a 150 px column of buttons.
//!
//! - **List** (`app.rs:5344-5414`): a 140 × 289 box with 1 px of padding, a
//!   `COLOR_CONTRAST` fill, a 1 px secondary border and a 2 px radius, around
//!   a scroll area 287 px high. Rows are 26 px high with 8 px of side padding,
//!   the name in Inter Medium 14 px, filled with the accent while highlighted
//!   or hovered. A 1 px secondary divider separates the custom presets from
//!   the built-ins, before row `1 + customs` while "Default" leads.
//! - **Buttons** (150 × 30, 8 px apart): Aceptar, Cancelar, Eliminar,
//!   Importar, Exportar, then, 16 px lower, Predeterminado and the warning
//!   (Inter Medium 11 px, secondary).
//!
//! Clicking a row previews the preset. Aceptar keeps it; Cancelar, Escape and
//! a click outside put the captured state back ([`PresetLoad::cancel`]).
//! Eliminar and Exportar do nothing while no row is highlighted, as in Iced
//! (`app.rs:5469-5477`); they look the same either way.
//!
//! Importar and Exportar use the platform's file dialogs, which run without
//! blocking the UI (quirk fix 5), with Iced's "Audoxidy EQ Preset"
//! `json` / `aeqp` filter (`app.rs:4436`); a dialog that fails is an error
//! toast (quirk fix 8).

use std::rc::Rc;

use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    FixedSize, HStack, Padding, RectWidget, Spacer, TextWidget, VStack, ZStack,
};

use super::buttons::fixed_button;
use super::equalizer_vm::{EqFailure, EqualizerViewModel, PresetLoad, report};
use super::placement::{self, DialogKind, LOAD_SIZE};
use crate::frontend::commands::eq::PresetKind;
use crate::logic::eq::DEFAULT_PRESET_NAME;
use crate::teksilo_ui::style::inter;
use crate::teksilo_ui::widgets::dialog_frame::{Frame, PANEL_BORDER, PANEL_RADIUS, panel_shadow};
use crate::teksilo_ui::widgets::{Pressable, stock_styles};

/// The dialog's padding (`app.rs:5547`).
pub const PADDING: f32 = 17.0;
/// The title's size (`app.rs:5530`).
pub const TITLE_SIZE: f32 = 16.0;
/// The space under the title (`app.rs:5538`).
pub const TITLE_GAP: f32 = 8.0;
/// The list box's width and height (`app.rs:5379-5380`).
pub const LIST_SIZE: (f32, f32) = (140.0, 289.0);
/// The list box's padding (`app.rs:5381`).
pub const LIST_PADDING: f32 = 1.0;
/// A row's height (`app.rs:5362`).
pub const ROW_HEIGHT: f32 = 26.0;
/// A row's side padding (`app.rs:5363`).
pub const ROW_PADDING: f32 = 8.0;
/// The space between the list and the buttons (`app.rs:5517`).
pub const COLUMN_GAP: f32 = 10.0;
/// The buttons' size (`app.rs:4540`).
pub const BUTTON_SIZE: (f32, f32) = (150.0, 30.0);
/// The space between the buttons (`app.rs:5509`).
pub const BUTTON_SPACING: f32 = 8.0;
/// The warning's size (`app.rs:5504`).
pub const WARNING_SIZE: f32 = 11.0;
/// The name of the file filter (`app.rs:4436`).
pub const FILE_FILTER: &str = "Audoxidy EQ Preset";
/// The extensions the file filter accepts (`app.rs:4436`).
pub const FILE_EXTENSIONS: [&str; 2] = ["json", "aeqp"];

/// Opens the Load dialog over `vm` (the load icon, `app.rs:4488`). Nothing
/// opens before the view-model is loaded: its backup would not be the
/// restored equalizer ([`EqualizerViewModel::is_loaded`]).
pub fn open(vm: &EqualizerViewModel, ctx: &mut EventContext) {
    if !vm.is_loaded() {
        return;
    }
    let session = vm.begin_load();
    let dismissed = session.clone();
    placement::present(
        ctx,
        DialogKind::Load,
        vm.panel_position(),
        tr!(ac_equalizer_load_title()),
        Box::new(LoadDialog::new(session)),
        Rc::new(move |_| dismissed.cancel()),
    );
}

/// The picker request for `title`, with Iced's filter.
fn file_request(request: FileDialogRequest, title: LocalizedString) -> FileDialogRequest {
    request
        .title(title.resolve_now())
        .add_filter(FILE_FILTER, &FILE_EXTENSIONS)
}

/// "Importar": the file picker, then the import.
fn pick_import(session: &PresetLoad, ctx: &mut EventContext) {
    let session = session.clone();
    let started = ctx.pick_file(
        file_request(
            FileDialogRequest::pick_file(),
            tr!(ac_equalizer_import_title()),
        ),
        move |result, ctx| match result {
            FileDialogResult::File(Some(path)) => session.import(path, ctx),
            FileDialogResult::Error(error) => report(ctx, EqFailure::FileDialog, &error),
            _ => {}
        },
    );
    if let Err(error) = started {
        report(ctx, EqFailure::FileDialog, &error);
    }
}

/// "Exportar": nothing while no row is highlighted; otherwise the save
/// picker, then the export.
fn pick_export(session: &PresetLoad, ctx: &mut EventContext) {
    if session.selected_name().is_none() {
        return;
    }
    let session = session.clone();
    let started = ctx.save_file(
        file_request(
            FileDialogRequest::save_file(),
            tr!(ac_equalizer_export_title()),
        ),
        move |result, ctx| match result {
            FileDialogResult::Saved(Some(path)) => session.export(path, ctx),
            FileDialogResult::Error(error) => report(ctx, EqFailure::FileDialog, &error),
            _ => {}
        },
    );
    if let Err(error) = started {
        report(ctx, EqFailure::FileDialog, &error);
    }
}

/// The dialog's panel; see the module documentation.
pub struct LoadDialog {
    session: PresetLoad,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for LoadDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadDialog")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl LoadDialog {
    /// The dialog over `session`.
    pub fn new(session: PresetLoad) -> Self {
        Self {
            session,
            root: None,
        }
    }
}

/// The text of a dialog button.
fn button(label: LocalizedString, on_activate: impl Fn(&mut EventContext) + 'static) -> Pressable {
    fixed_button(label, BUTTON_SIZE.0, BUTTON_SIZE.1, true).on_activate(on_activate)
}

/// A zero-height spacer: Iced's `Space::new().height(0)` still takes the
/// column's spacing on both sides (`app.rs:5497`).
fn zero_space() -> FixedSize {
    FixedSize::new().height(0.0).child(Spacer::new())
}

impl LoadDialog {
    fn buttons(&self) -> impl Widget + 'static {
        let accept = {
            let session = self.session.clone();
            button(tr!(ac_equalizer_load_accept()), move |ctx| {
                session.accept();
                ctx.dismiss_modal();
            })
        };
        let cancel = {
            let session = self.session.clone();
            button(tr!(chrome_dialog_cancel()), move |ctx| {
                session.cancel();
                ctx.dismiss_modal();
            })
        };
        let delete = {
            let session = self.session.clone();
            button(tr!(ac_equalizer_load_delete()), move |ctx| {
                session.delete_selected(ctx)
            })
        };
        let import = {
            let session = self.session.clone();
            button(tr!(ac_equalizer_load_import()), move |ctx| {
                pick_import(&session, ctx)
            })
        };
        let export = {
            let session = self.session.clone();
            button(tr!(ac_equalizer_load_export()), move |ctx| {
                pick_export(&session, ctx)
            })
        };
        let defaults = {
            let session = self.session.clone();
            button(tr!(ac_equalizer_load_defaults()), move |ctx| {
                session.restore_defaults(ctx)
            })
        };
        FixedSize::new().width(BUTTON_SIZE.0).child(
            VStack::new()
                .spacing(BUTTON_SPACING)
                .alignment(HAlignment::Center)
                .child(accept)
                .child(cancel)
                .child(delete)
                .child(import)
                .child(export)
                .child(zero_space())
                .child(defaults)
                .child(zero_space())
                .child(
                    TextWidget::new(tr!(ac_equalizer_load_warning()))
                        .style(inter(WARNING_SIZE, FontWeight::MEDIUM))
                        .color(TextRole::Secondary),
                ),
        )
    }

    fn list(&self) -> impl Widget + 'static {
        let rows = FixedSize::new()
            .width(LIST_SIZE.0 - 2.0 * LIST_PADDING)
            .height(LIST_SIZE.1 - 2.0 * LIST_PADDING)
            .child(stock_styles::scroll_area().child(PresetRows {
                session: self.session.clone(),
                root: None,
            }));
        FixedSize::new()
            .width(LIST_SIZE.0)
            .height(LIST_SIZE.1)
            .child(
                Frame::new(rows)
                    .fill(SurfaceRole::Raised)
                    .border(TextRole::Secondary, 1.0)
                    .radius(2.0)
                    .padding(LIST_PADDING, LIST_PADDING, LIST_PADDING, LIST_PADDING),
            )
            // A single-select list: its rows are options that say which one
            // is chosen (Iced shows it by colour only).
            .access_role(Role::ListBox)
            .access_label(tr!(ac_equalizer_load_list()))
    }
}

impl Widget for LoadDialog {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let content = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(self.list())
            .child(FixedSize::new().width(COLUMN_GAP).child(Spacer::new()))
            .child(self.buttons());
        let column = VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Center)
            .child(
                TextWidget::new(tr!(ac_equalizer_load_title()))
                    .style(inter(TITLE_SIZE, FontWeight::MEDIUM))
                    .color(TextRole::Primary)
                    .single_line(),
            )
            .child(FixedSize::new().height(TITLE_GAP).child(Spacer::new()))
            .child(content);
        let root = ctx.add(
            FixedSize::new()
                .width(LOAD_SIZE.0)
                .height(LOAD_SIZE.1)
                .child(
                    ZStack::new()
                        .alignment(Alignment::TOP_LEADING)
                        .child(
                            Frame::new(Spacer::new())
                                .fill(SurfaceRole::Main)
                                .border(BorderRole::Accent, PANEL_BORDER)
                                .radius(PANEL_RADIUS)
                                .shadow(panel_shadow()),
                        )
                        .child(Padding::uniform(PADDING).child(column)),
                )
                .access_role(Role::Dialog)
                .access_label(tr!(ac_equalizer_load_title())),
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

/// The list's rows, rebuilt when the presets change.
struct PresetRows {
    session: PresetLoad,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for PresetRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresetRows").finish_non_exhaustive()
    }
}

impl Widget for PresetRows {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.session.version().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Rebuild,
        );
        let entries = self.session.entries();
        let width = LIST_SIZE.0 - 2.0 * LIST_PADDING;
        let mut column = VStack::new().spacing(0.0).alignment(HAlignment::Leading);
        let has_default = entries
            .with_item(0, |entry| entry.kind == PresetKind::Default)
            .unwrap_or(false);
        debug_assert!(
            !has_default
                || entries
                    .with_item(0, |entry| entry.preset.name == DEFAULT_PRESET_NAME)
                    .unwrap_or(false)
        );
        let divider_at = 1 + self.session.custom_count();
        for index in 0..entries.len() {
            if has_default && index == divider_at {
                column = column.child(
                    FixedSize::new()
                        .width(width)
                        .height(1.0)
                        .child(RectWidget::new().background(TextRole::Secondary)),
                );
            }
            let name = entries
                .with_item(index, |entry| entry.preset.name.clone())
                .unwrap_or_default();
            column = column.child(preset_row(&self.session, index, name, width));
        }
        let root = ctx.add(column);
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

/// One row (`app.rs:5354-5403`): highlighted or hovered, it is filled with the
/// accent. To assistive technologies it is an option of the list box,
/// selected while highlighted: Iced shows the chosen preset by its colour
/// only, which a screen reader cannot see.
fn preset_row(
    session: &PresetLoad,
    index: usize,
    name: String,
    width: f32,
) -> impl Widget + 'static {
    let hovered = Signal::new(false);
    let chosen = session
        .selected()
        .map(move |selected| *selected == Some(index));
    let lit = chosen
        .zip(&hovered)
        .map(|(chosen, hovered)| *hovered || *chosen);
    let fill = lit.map(|lit| {
        if *lit {
            SurfaceRole::Accent
        } else {
            SurfaceRole::Transparent
        }
    });
    let label = lit!(name.clone());
    let body = FixedSize::new().width(width).height(ROW_HEIGHT).child(
        ZStack::new()
            .alignment(Alignment::CENTER_LEADING)
            .child(RectWidget::new().background(fill))
            .child(
                Padding::symmetric(0.0, ROW_PADDING).child(
                    TextWidget::new(label.clone())
                        .style(inter(14.0, FontWeight::MEDIUM))
                        .color(TextRole::Primary)
                        .single_line()
                        .a11y_hidden(),
                ),
            ),
    );
    let session = session.clone();
    // `tab` is how a `Pressable` carries a selected state (and keeps the AT
    // node in step with the signal); the role is then the list box's option.
    Pressable::new(body, label)
        .tab(chosen)
        .hovered(hovered)
        .on_activate(move |_| session.select(index))
        .access_role(Role::ListBoxOption)
}

#[cfg(test)]
mod tests;
