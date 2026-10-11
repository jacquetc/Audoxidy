//! The Save dialog ("Guardar Preset"): a name field and two buttons.
//!
//! Port of `ActiveDialog::EqPresetSave`'s view (`src/gui/app.rs:5553-5688`). A
//! 190 × 162 panel (15 px of padding, a 2 px accent border, an 8 px radius and
//! the dialog shadow) holding the title (Inter Medium 16 px, 2 px under it
//! and 8 px of space), the prompt (Inter 14 px, centred), 12 px of space, the
//! name field (a 160 px `DialogInputStyle` field), 15 px of space and the
//! buttons: Cancelar and Guardar, 74 × 30 and 8 px apart. Guardar is disabled
//! while the name is empty.
//!
//! Enter confirms (`on_submit(ConfirmDialogAction)`, `app.rs:5560`): the
//! dialog closes whether or not the name is empty, and an empty name saves
//! nothing (`audio_center.rs:1472`). The field has the focus when the dialog
//! opens (the first focusable widget of a modal does).

use teksilo::core::accesskit::Role;
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{FixedSize, HStack, Padding, Spacer, TextInput, TextWidget, VStack, ZStack};

use super::buttons::fixed_button;
use super::equalizer_vm::EqualizerViewModel;
use super::placement::{self, DialogKind, SAVE_SIZE};
use crate::teksilo_ui::style::inter;
use crate::teksilo_ui::widgets::dialog_frame::{Frame, PANEL_BORDER, PANEL_RADIUS, panel_shadow};

/// The dialog's padding (`app.rs:5670`).
pub const PADDING: f32 = 15.0;
/// The title's size (`app.rs:5652`).
pub const TITLE_SIZE: f32 = 16.0;
/// The space under the title: its 2 px padding and the 8 px spacer
/// (`app.rs:5659-5662`).
pub const TITLE_GAP: f32 = 10.0;
/// The prompt's size (`app.rs:5617`).
pub const PROMPT_SIZE: f32 = 14.0;
/// The space under the prompt (`app.rs:5620`).
pub const PROMPT_GAP: f32 = 12.0;
/// The name field's width: the panel's content (`app.rs:5612`).
pub const INPUT_WIDTH: f32 = SAVE_SIZE.0 - 2.0 * PADDING;
/// The space under the field (`app.rs:5622`).
pub const INPUT_GAP: f32 = 15.0;
/// The buttons' size (`app.rs:5580-5581`).
pub const BUTTON_SIZE: (f32, f32) = (74.0, 30.0);
/// The space between the buttons (`app.rs:5629`).
pub const BUTTON_SPACING: f32 = 8.0;

/// Opens the Save dialog over `vm` (the save icon, `app.rs:4515`).
pub fn open(vm: &EqualizerViewModel, ctx: &mut EventContext) {
    placement::present(
        ctx,
        DialogKind::Save,
        vm.panel_position(),
        tr!(ac_equalizer_save_title()),
        Box::new(SaveDialog::new(vm.clone())),
        // Nothing to undo: the name is only used when Guardar runs.
        std::rc::Rc::new(|_| {}),
    );
}

/// The dialog's panel; see the module documentation.
pub struct SaveDialog {
    vm: EqualizerViewModel,
    name: Signal<String>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for SaveDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveDialog")
            .field("name", &self.name.get())
            .finish_non_exhaustive()
    }
}

impl SaveDialog {
    /// The dialog over `vm`, with an empty name.
    pub fn new(vm: EqualizerViewModel) -> Self {
        Self {
            vm,
            name: Signal::new(String::new()),
            root: None,
        }
    }

    /// The name typed so far.
    pub fn name(&self) -> Signal<String> {
        self.name.clone()
    }
}

/// Saves under `name`, then closes the dialog (`ConfirmDialogAction`).
fn confirm(vm: &EqualizerViewModel, name: &Signal<String>, ctx: &mut EventContext) {
    ctx.dismiss_modal();
    vm.save_preset(&name.get(), ctx);
}

impl Widget for SaveDialog {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let submit = {
            let (vm, name) = (self.vm.clone(), self.name.clone());
            move |ctx: &mut EventContext| confirm(&vm, &name, ctx)
        };
        let input = TextInput::new(self.name.clone())
            .placeholder(tr!(ac_equalizer_save_placeholder()))
            .label(tr!(ac_equalizer_save_prompt()))
            .on_submit_fn(submit);
        let cancel = fixed_button(
            tr!(chrome_dialog_cancel()),
            BUTTON_SIZE.0,
            BUTTON_SIZE.1,
            true,
        )
        .on_activate(|ctx| ctx.dismiss_modal());
        let save = {
            let (vm, name) = (self.vm.clone(), self.name.clone());
            fixed_button(
                tr!(ac_equalizer_save_confirm()),
                BUTTON_SIZE.0,
                BUTTON_SIZE.1,
                self.name.map(|name| !name.is_empty()),
            )
            .on_activate(move |ctx| confirm(&vm, &name, ctx))
        };
        let column = VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Center)
            .child(
                TextWidget::new(tr!(ac_equalizer_save_title()))
                    .style(inter(TITLE_SIZE, FontWeight::MEDIUM))
                    .color(TextRole::Primary)
                    .single_line(),
            )
            .child(FixedSize::new().height(TITLE_GAP).child(Spacer::new()))
            .child(
                TextWidget::new(tr!(ac_equalizer_save_prompt()))
                    .style(inter(PROMPT_SIZE, FontWeight::REGULAR))
                    .color(TextRole::Primary)
                    .single_line(),
            )
            .child(FixedSize::new().height(PROMPT_GAP).child(Spacer::new()))
            .child(FixedSize::new().width(INPUT_WIDTH).child(input))
            .child(FixedSize::new().height(INPUT_GAP).child(Spacer::new()))
            .child(
                HStack::new()
                    .spacing(BUTTON_SPACING)
                    .alignment(VAlignment::Center)
                    .child(cancel)
                    .child(save),
            );
        let root = ctx.add(
            FixedSize::new()
                .width(SAVE_SIZE.0)
                .height(SAVE_SIZE.1)
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
                .access_label(tr!(ac_equalizer_save_title())),
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

#[cfg(test)]
mod tests;
