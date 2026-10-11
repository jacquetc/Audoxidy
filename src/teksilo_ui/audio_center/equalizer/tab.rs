//! `EqualizerTab`: the body of the Audio Center's "Ecualizador" tab.
//!
//! Port of `view_equalizer` (`src/gui/audio_center.rs:2783-3032`), top to
//! bottom in a column that fills the tab:
//!
//! 1. 19 px of space;
//! 2. the top row, three equal parts (`FillPortion(1)` each): "Activar
//!    Ecualizador" with its toggle (spacing 10) at the leading edge, "Bandas:"
//!    with the 20 / 31 radios (spacing 10) in the middle, and the preset row
//!    at the trailing edge: the "Preset: X" label, which only exists while a
//!    preset other than "Default" is selected, and the load, save and reset
//!    icon buttons (spacing 7, icons of 14 px);
//! 3. 25 px of space;
//! 4. the sliders: the preamp, 2 px, the guide labels (+9, 0, -9), 2 px, and
//!    the 20 or 31 bands 20.18 or 3.98 px apart. A slider is a column of the
//!    top label (Inter Medium 10.4 px), 10 px, the 24 × 240 [`EqSlider`], 10 px
//!    and the bottom label; the bands put their frequency above the even ones
//!    and below the odd ones, and the preamp "Pre" above and its gain below;
//! 5. the free space, and the hint (Inter Medium 12 px, secondary) at the
//!    bottom.
//!
//! The equalizer-off look of the sliders (a dark handle) is [`EqSlider`]'s.
//! Switching to the other band set rebuilds the slider row.

use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{Expand, FixedSize, HStack, Padding, Spacer, TextWidget, Toggle, VStack};

use super::equalizer_vm::{EqualizerViewModel, MODE_20, MODE_31};
use super::{load_dialog, save_dialog};
use crate::teksilo_ui::icons;
use crate::teksilo_ui::style::inter;
use crate::teksilo_ui::widgets::stock_styles::{self, ToggleLook};
use crate::teksilo_ui::widgets::{EqSlider, IconAction};

/// The space above the top row (`audio_center.rs:3014`).
pub const TOP_SPACE: f32 = 19.0;
/// The space between the top row and the sliders (`audio_center.rs:3016`).
pub const ROW_GAP: f32 = 25.0;
/// The space between the toggle and its label, and between the radios and
/// theirs (`audio_center.rs:2798`, `2810`).
pub const CONTROL_SPACING: f32 = 10.0;
/// The space between the preset row's items (`audio_center.rs:2860`).
pub const PRESET_SPACING: f32 = 7.0;
/// The side padding of the "Preset: X" label (`audio_center.rs:2822`).
pub const PRESET_LABEL_PADDING: f32 = 5.0;
/// The size of the preset icons (`audio_center.rs:2844`).
pub const PRESET_ICON_SIZE: f32 = 14.0;
/// The gap between the band columns in 20-band mode (`audio_center.rs:2951`).
pub const SPACING_20: f32 = 20.18;
/// The gap between the band columns in 31-band mode (`audio_center.rs:2950`).
pub const SPACING_31: f32 = 3.98;
/// The space between the preamp, the guide and the bands
/// (`audio_center.rs:3021-3023`).
pub const SLIDER_ROW_GAP: f32 = 2.0;
/// The size of a slider's labels (`audio_center.rs:2912`).
pub const LABEL_SIZE: f32 = 10.4;
/// The space between a slider and its labels (`audio_center.rs:2915`).
pub const LABEL_GAP: f32 = 10.0;
/// The hint's size (`audio_center.rs:2987`).
pub const HINT_SIZE: f32 = 12.0;
/// The guide's offsets: the space above "+9", and between its labels
/// (`audio_center.rs:2992-3005`).
pub const GUIDE_TOP: f32 = 25.0;
/// The space between the guide's labels.
pub const GUIDE_GAP: f32 = 97.0;

/// The tab's body; see the module documentation.
pub struct EqualizerTab {
    vm: EqualizerViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for EqualizerTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EqualizerTab")
            .field("vm", &self.vm)
            .finish_non_exhaustive()
    }
}

impl EqualizerTab {
    /// The tab over `vm`.
    pub fn new(vm: EqualizerViewModel) -> Self {
        Self { vm, root: None }
    }
}

/// An Inter Medium label in `role`.
fn label(text: LocalizedString, size: f32, role: TextRole) -> TextWidget {
    TextWidget::new(text)
        .style(inter(size, FontWeight::MEDIUM))
        .color(role)
        .single_line()
}

/// A literal (untranslated) label: a frequency, a gain, a guide mark.
fn plain(text: impl Into<String>, size: f32, role: TextRole) -> TextWidget {
    label(lit!(text.into()), size, role).a11y_hidden()
}

fn space(width: f32, height: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(height)
        .child(Spacer::new())
}

/// "Activar Ecualizador" and its toggle (`audio_center.rs:2786-2807`).
fn toggle_part(vm: &EqualizerViewModel) -> impl Widget + 'static {
    let on = vm.enabled();
    let owner = vm.clone();
    HStack::new()
        .spacing(CONTROL_SPACING)
        .alignment(VAlignment::Center)
        .child(label(tr!(ac_equalizer_toggle()), 14.0, TextRole::Primary).a11y_hidden())
        .child(stock_styles::toggle(
            Toggle::new(on).on_change(move |on, _| owner.set_enabled(on)),
            ToggleLook::equalizer(),
            tr!(ac_equalizer_toggle()),
        ))
}

/// "Bandas:" and the 20 / 31 radios (`audio_center.rs:2809-2820`).
fn bands_part(vm: &EqualizerViewModel) -> impl Widget + 'static {
    HStack::new()
        .spacing(CONTROL_SPACING)
        .alignment(VAlignment::Center)
        .child(label(tr!(ac_equalizer_bands()), 14.0, TextRole::Primary))
        .child(stock_styles::radio(MODE_20, vm.mode(), lit!("20")))
        .child(stock_styles::radio(MODE_31, vm.mode(), lit!("31")))
}

/// The preset row (`audio_center.rs:2822-2862`): the label, when there is
/// one, and the three icon buttons.
fn preset_part(vm: &EqualizerViewModel) -> impl Widget + 'static {
    let load = {
        let vm = vm.clone();
        IconAction::button(
            icons::shared::equalizer(),
            PRESET_ICON_SIZE,
            tr!(ac_equalizer_load_button()),
        )
        .on_activate(move |ctx| load_dialog::open(&vm, ctx))
    };
    let save = {
        let vm = vm.clone();
        IconAction::button(
            icons::audio_center::save_outlined(),
            PRESET_ICON_SIZE,
            tr!(ac_equalizer_save_button()),
        )
        .on_activate(move |ctx| save_dialog::open(&vm, ctx))
    };
    let reset = {
        let vm = vm.clone();
        IconAction::button(
            icons::shared::restore(),
            PRESET_ICON_SIZE,
            tr!(ac_equalizer_reset_button()),
        )
        .on_activate(move |_| vm.reset())
    };
    HStack::new()
        .spacing(PRESET_SPACING)
        .alignment(VAlignment::Center)
        .child(PresetLabel {
            vm: vm.clone(),
            root: None,
        })
        .child(load)
        .child(save)
        .child(reset)
}

/// The top row: three equal parts (`audio_center.rs:2864-2876`).
fn top_row(vm: &EqualizerViewModel) -> impl Widget + 'static {
    HStack::new()
        .spacing(0.0)
        .alignment(VAlignment::Center)
        .child(
            Expand::horizontal()
                .align_child(Alignment::CENTER_LEADING)
                .child(toggle_part(vm)),
        )
        .child(
            Expand::horizontal()
                .align_child(Alignment::CENTER)
                .child(bands_part(vm)),
        )
        .child(
            Expand::horizontal()
                .align_child(Alignment::CENTER_TRAILING)
                .child(preset_part(vm)),
        )
}

/// One slider with its two labels (`audio_center.rs:2881-2933`).
fn slider_column(top: String, bottom: String, slider: EqSlider) -> impl Widget + 'static {
    VStack::new()
        .spacing(0.0)
        .alignment(HAlignment::Center)
        .child(plain(top, LABEL_SIZE, TextRole::Primary))
        .child(space(0.0, LABEL_GAP))
        .child(slider)
        .child(space(0.0, LABEL_GAP))
        .child(plain(bottom, LABEL_SIZE, TextRole::Primary))
}

/// The preamp's column: "Pre" above, its gain below.
fn preamp_column(vm: &EqualizerViewModel) -> impl Widget + 'static {
    let owner = vm.clone();
    let gain = vm.preamp();
    let slider = EqSlider::new(vm.preamp(), tr!(ac_equalizer_preamp_name()))
        .eq_enabled(vm.enabled())
        .on_change(move |value, _| owner.set_preamp(value));
    VStack::new()
        .spacing(0.0)
        .alignment(HAlignment::Center)
        .child(plain("Pre", LABEL_SIZE, TextRole::Primary))
        .child(space(0.0, LABEL_GAP))
        .child(slider)
        .child(space(0.0, LABEL_GAP))
        .child(
            label(lit!(""), LABEL_SIZE, TextRole::Primary)
                .text(gain.map(|gain| format!("{gain:.1}")))
                .a11y_hidden(),
        )
}

/// The guide: +9, 0 and -9 dB (`audio_center.rs:2990-3010`).
fn guide() -> impl Widget + 'static {
    VStack::new()
        .spacing(0.0)
        .alignment(HAlignment::Center)
        .child(space(0.0, GUIDE_TOP))
        .child(plain("+9", 11.0, TextRole::Secondary))
        .child(space(0.0, GUIDE_GAP))
        .child(plain("0", 12.0, TextRole::Secondary))
        .child(space(0.0, GUIDE_GAP))
        .child(plain("-9", 11.0, TextRole::Secondary))
}

impl Widget for EqualizerTab {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        // The radios write the mode signal; the view-model switches the band
        // set (a mode that already is the state's does nothing).
        let vm = self.vm.clone();
        ctx.effect(&self.vm.mode(), move |mode| {
            vm.set_bands_31(*mode == MODE_31)
        });
        let sliders = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(preamp_column(&self.vm))
            .child(space(SLIDER_ROW_GAP, 0.0))
            .child(guide())
            .child(space(SLIDER_ROW_GAP, 0.0))
            .child(Bands {
                vm: self.vm.clone(),
                root: None,
            });
        let root = ctx.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(space(0.0, TOP_SPACE))
                .child(top_row(&self.vm))
                .child(space(0.0, ROW_GAP))
                .child(sliders)
                .child(Expand::vertical().child(Spacer::new()))
                .child(label(
                    tr!(ac_equalizer_hint()),
                    HINT_SIZE,
                    TextRole::Secondary,
                )),
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

/// The "Preset: X" label, present only while a preset other than "Default"
/// is selected: the row rebuilds when that changes (Iced puts a zero-width
/// space there otherwise, `audio_center.rs:2831`).
struct PresetLabel {
    vm: EqualizerViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for PresetLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresetLabel").finish_non_exhaustive()
    }
}

impl Widget for PresetLabel {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.preset_shown().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Rebuild,
        );
        let root = if self.vm.preset_shown().get() {
            let text = self.vm.preset_name().map(|name| {
                name.as_deref()
                    .map(|name| {
                        tr!(ac_equalizer_preset_label(name = name.to_string())).resolve_now()
                    })
                    .unwrap_or_default()
            });
            ctx.add(
                Padding::symmetric(0.0, PRESET_LABEL_PADDING)
                    .child(label(lit!(""), 14.0, TextRole::Primary).text(text)),
            )
        } else {
            ctx.add(space(0.0, 0.0))
        };
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

/// The band sliders: 20 or 31 columns, rebuilt when the mode changes.
struct Bands {
    vm: EqualizerViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for Bands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bands").finish_non_exhaustive()
    }
}

impl Widget for Bands {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm
            .bands_31()
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let count = self.vm.band_count();
        let spacing = if count == 31 { SPACING_31 } else { SPACING_20 };
        let mut row = HStack::new().spacing(spacing).alignment(VAlignment::Top);
        for index in 0..count {
            let frequency = self.vm.band_label(index);
            let (top, bottom) = if index % 2 == 0 {
                (frequency.clone(), " ".to_string())
            } else {
                (" ".to_string(), frequency.clone())
            };
            let owner = self.vm.clone();
            let slider = EqSlider::new(
                self.vm.band_gain(index),
                tr!(ac_equalizer_band_name(frequency = frequency)),
            )
            .eq_enabled(self.vm.enabled())
            .on_change(move |value, _| owner.set_band_gain(index, value));
            row = row.child(slider_column(top, bottom, slider));
        }
        let root = ctx.add(row);
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
