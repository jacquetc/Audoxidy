//! [`VolumeMixTab`]: the Audio Center's "Volumen y Mezcla" tab.
//!
//! Port of `view_volumen_mezcla` (`src/gui/audio_center.rs:3708-4664`). The tab
//! is 20 px under the tab strip's line and has two columns in 5 : 4 parts of
//! what the divider leaves, with a 2 × 220 `COLOR_CONTRAST` line between
//! them, lifted 24 px above the middle (`:4617`). The left column has the
//! groups volume fades, silence removal, the pre-amp and ReplayGain, 18 px
//! apart; the right column has crossfade. In a group, rows are 5 px apart:
//!
//! - the **title** row (`:3780`): the master check box, the title (Inter
//!   Medium 14 px, primary, clickable like the box), 8 px, and a 1 px line in
//!   the secondary colour filling the rest of the row;
//! - a **switch** row: a check box, 5 px, the label (Inter Medium 13 px, lit
//!   in primary while its control is active and dim in secondary otherwise,
//!   clickable), and at the trailing edge the [`Stepper`] of the value it
//!   switches, if it has one;
//! - a **value** row, for a stepper that depends on the master only (or that
//!   sits under a sub-heading such as "Cambio manual:"): 8 px, the label, the
//!   stepper.
//!
//! While a master is off, what is in its group is locked: the check boxes are
//! drawn unchecked and take no click, the labels take none, the steppers are
//! disabled. See [`VolumeMixViewModel`].
//!
//! The Iced check box is a 13 px square; the stock one reserves a 24 px target
//! around its drawing (`widgets::stock_styles`). Here it is held to the 13 px
//! of Iced, so that the fourteen rows fit the tab as they do there; the label
//! is the larger target.

use teksilo::core::accesskit::Role;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    Center, Checkbox, Expand, FixedSize, HStack, Padding, RectWidget, Spacer, TextWidget, VStack,
};

use super::volume_mix_vm::{VolumeMixViewModel, value_name};
use crate::frontend::commands::mix::{MixToggle, MixValue};
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Stepper;
use crate::teksilo_ui::widgets::stock_styles::{CHECKBOX_SIZE, checkbox};

/// The space above the columns (`:4645`).
pub const TOP: f32 = 20.0;
/// The space between groups (`:4590`).
pub const GROUP_SPACING: f32 = 18.0;
/// The space between the rows of a group (`:3880`).
pub const ROW_SPACING: f32 = 5.0;
/// The space between a check box and its label (`:3787`), and after a title
/// (`:3795`).
pub const CHECK_SPACING: f32 = 5.0;
/// The space before a title's line, and before a row that has no check box
/// (`:3796`).
pub const TITLE_GAP: f32 = 8.0;
/// The divider's width and height (`:4594`).
pub const LINE_WIDTH: f32 = 2.0;
/// The divider's height.
pub const LINE_HEIGHT: f32 = 220.0;
/// The divider cell's width: Iced's 30, less the 1 px by which the right
/// column's `-1` padding enters it (`:4627`, `:4639`).
pub const DIVIDER_CELL: f32 = 29.0;
/// How much of the divider's cell the line is moved toward the leading edge,
/// as a trailing padding: half of Iced's `-5` leading padding (`:4631`).
pub const DIVIDER_SHIFT: f32 = 5.0;
/// The lift of the divider: a bottom padding, twice the 24 px that Iced's
/// `-48` top padding raises its centre (`:4630`).
pub const DIVIDER_LIFT: f32 = 48.0;
/// The left column's padding at its trailing edge (`:4621`).
pub const LEFT_TRAILING: f32 = 5.0;

/// Inter Medium at `size`, in `role`.
fn text(label: impl Into<LocalizedString>, size: f32, role: impl Into<ColorProp>) -> TextWidget {
    TextWidget::new(label)
        .style(style::inter(size, FontWeight::MEDIUM))
        .color(role)
        .single_line()
        .no_shrink()
}

/// Primary while `lit`, secondary otherwise.
fn lit_role(lit: Signal<bool>) -> Signal<TextRole> {
    lit.map(|lit| {
        if *lit {
            TextRole::Primary
        } else {
            TextRole::Secondary
        }
    })
}

/// A row as Iced lays one out, centred vertically.
fn row() -> HStack {
    HStack::new().spacing(0.0).alignment(VAlignment::Center)
}

/// A column with the given spacing, children at the leading edge.
fn column(spacing: f32) -> VStack {
    VStack::new()
        .spacing(spacing)
        .alignment(HAlignment::Leading)
}

/// A fixed gap in a row.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(1.0)
        .child(Spacer::new())
}

/// `toggle`'s check box, 13 px as in Iced, named `name`.
fn check(vm: &VolumeMixViewModel, toggle: MixToggle, name: LocalizedString) -> impl Widget {
    let handler = vm.clone();
    FixedSize::new()
        .width(CHECKBOX_SIZE)
        .height(CHECKBOX_SIZE)
        .child(checkbox(
            Checkbox::new(vm.checked(toggle))
                .enabled(vm.unlocked(toggle))
                .on_change(move |on, _| {
                    handler.set_toggle(toggle, on);
                }),
            name,
        ))
}

/// A label that flips `toggle` when clicked (UAT round 10), `size` px, lit
/// while `lit`. The check box carries the accessible name.
fn clickable(
    vm: &VolumeMixViewModel,
    toggle: MixToggle,
    label: LocalizedString,
    size: f32,
    color: impl Into<ColorProp>,
) -> impl Widget {
    let vm = vm.clone();
    text(label, size, color)
        .a11y_hidden()
        .cursor(CursorIcon::Pointer)
        .on_tap(move |_, _| {
            vm.flip(toggle);
        })
}

/// `value`'s stepper, reset by a secondary click to Iced's value.
fn stepper(vm: &VolumeMixViewModel, value: MixValue) -> Stepper {
    let changed = vm.clone();
    Stepper::new(
        vm.value(value),
        value.range(),
        value.unit(),
        value_name(value),
    )
    .reset_to(value.reset_value())
    .enabled(vm.value_enabled(value))
    .on_change(move |to| changed.value_changed(value, to))
}

/// The horizontal line after a group title (`:3745`): 1 px, secondary.
fn line() -> impl Widget {
    Expand::horizontal().child(
        FixedSize::new()
            .height(1.0)
            .child(RectWidget::new().background(TextRole::Secondary)),
    )
}

/// A group's title row.
fn title_row(vm: &VolumeMixViewModel, toggle: MixToggle, title: LocalizedString) -> HStack {
    row()
        .child(check(vm, toggle, title.clone()))
        .child(gap(CHECK_SPACING))
        .child(clickable(vm, toggle, title, 14.0, TextRole::Primary))
        .child(gap(TITLE_GAP))
        .child(Expand::horizontal().child(line()))
}

/// A switch row: check box and label, and the stepper of `value` if any.
fn toggle_row(
    vm: &VolumeMixViewModel,
    toggle: MixToggle,
    label: LocalizedString,
    value: Option<MixValue>,
) -> HStack {
    let lit = lit_role(vm.lit(toggle));
    let mut cells = row()
        .child(check(vm, toggle, label.clone()))
        .child(gap(CHECK_SPACING))
        .child(clickable(vm, toggle, label, 13.0, lit))
        .child(Spacer::new());
    if let Some(value) = value {
        cells = cells.child(stepper(vm, value));
    }
    cells
}

/// A value row: the label, lit while the stepper is enabled, and the stepper.
fn value_row(vm: &VolumeMixViewModel, label: LocalizedString, value: MixValue) -> HStack {
    row()
        .child(gap(TITLE_GAP))
        .child(text(label, 13.0, lit_role(vm.value_lit(value))))
        .child(Spacer::new())
        .child(stepper(vm, value))
}

/// Volume fades (`:3757-3950`).
fn fades(vm: &VolumeMixViewModel) -> impl Widget + use<> {
    column(ROW_SPACING)
        .child(title_row(
            vm,
            MixToggle::Fades,
            tr!(ac_volume_mix_fades_title()),
        ))
        .child(toggle_row(
            vm,
            MixToggle::SmoothVolume,
            tr!(ac_volume_mix_smooth()),
            None,
        ))
        .child(toggle_row(
            vm,
            MixToggle::FadeIn,
            tr!(ac_volume_mix_fade_in()),
            Some(MixValue::FadeInMs),
        ))
        .child(toggle_row(
            vm,
            MixToggle::FadeOut,
            tr!(ac_volume_mix_fade_out()),
            Some(MixValue::FadeOutMs),
        ))
}

/// Silence removal (`:3952-4060`).
fn silence(vm: &VolumeMixViewModel) -> impl Widget + use<> {
    column(ROW_SPACING)
        .child(title_row(
            vm,
            MixToggle::Silence,
            tr!(ac_volume_mix_silence_title()),
        ))
        .child(value_row(
            vm,
            tr!(ac_volume_mix_silence_duration()),
            MixValue::SilenceDurationMs,
        ))
        .child(value_row(
            vm,
            tr!(ac_volume_mix_silence_threshold()),
            MixValue::SilenceThresholdDb,
        ))
        .child(toggle_row(
            vm,
            MixToggle::SilenceEdgeTrim,
            tr!(ac_volume_mix_silence_edges()),
            None,
        ))
}

/// The pre-amp (`:4062-4130`).
fn preamp(vm: &VolumeMixViewModel) -> impl Widget + use<> {
    column(ROW_SPACING)
        .child(title_row(
            vm,
            MixToggle::RgFixed,
            tr!(ac_volume_mix_preamp_title()),
        ))
        .child(value_row(
            vm,
            tr!(ac_volume_mix_preamp_level()),
            MixValue::RgFixedDb,
        ))
}

/// ReplayGain (`:4132-4380`).
fn replay_gain(vm: &VolumeMixViewModel) -> impl Widget + use<> {
    column(ROW_SPACING)
        .child(title_row(
            vm,
            MixToggle::RgMaster,
            tr!(ac_volume_mix_rg_title()),
        ))
        .child(toggle_row(
            vm,
            MixToggle::RgAlbum,
            tr!(ac_volume_mix_rg_album()),
            Some(MixValue::RgOffsetAlbumDb),
        ))
        .child(toggle_row(
            vm,
            MixToggle::RgTrack,
            tr!(ac_volume_mix_rg_track()),
            Some(MixValue::RgOffsetTrackDb),
        ))
        .child(toggle_row(
            vm,
            MixToggle::RgAnalyzeRt,
            tr!(ac_volume_mix_rg_untagged()),
            Some(MixValue::RgOffsetRtDb),
        ))
}

/// Crossfade (`:4380-4590`).
fn crossfade(vm: &VolumeMixViewModel) -> impl Widget + use<> {
    column(ROW_SPACING)
        .child(title_row(
            vm,
            MixToggle::Crossfade,
            tr!(ac_volume_mix_crossfade_title()),
        ))
        .child(toggle_row(
            vm,
            MixToggle::CrossfadeManual,
            tr!(ac_volume_mix_crossfade_manual()),
            None,
        ))
        .child(value_row(
            vm,
            tr!(ac_volume_mix_crossfade_blend()),
            MixValue::CrossfadeManualMs,
        ))
        .child(toggle_row(
            vm,
            MixToggle::CrossfadeAuto,
            tr!(ac_volume_mix_crossfade_auto()),
            None,
        ))
        .child(value_row(
            vm,
            tr!(ac_volume_mix_crossfade_blend()),
            MixValue::CrossfadeAutoMs,
        ))
}

/// The tab; see the module documentation.
pub struct VolumeMixTab {
    vm: VolumeMixViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for VolumeMixTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolumeMixTab").finish_non_exhaustive()
    }
}

impl VolumeMixTab {
    /// The tab over `vm`.
    pub fn new(vm: VolumeMixViewModel) -> Self {
        Self { vm, root: None }
    }
}

/// Reads the engine's values again each time the tab appears. It occupies no
/// space and is rebuilt when the tab is shown or hidden.
struct OnShown {
    vm: VolumeMixViewModel,
    shown: Signal<bool>,
}

impl std::fmt::Debug for OnShown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnShown").finish_non_exhaustive()
    }
}

impl Widget for OnShown {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.shown.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            teksilo::core::binding::BindingLevel::Rebuild,
        );
        if self.shown.get() {
            self.vm.on_shown();
        }
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(0.0, 0.0).into()
    }
}

impl Widget for VolumeMixTab {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = &self.vm;
        let left = column(GROUP_SPACING)
            .child(fades(vm))
            .child(silence(vm))
            .child(preamp(vm))
            .child(replay_gain(vm));
        let divider = FixedSize::new().width(DIVIDER_CELL).child(
            Padding::new(0.0, DIVIDER_SHIFT, DIVIDER_LIFT, 0.0).child(
                Center::new().child(
                    FixedSize::new()
                        .width(LINE_WIDTH)
                        .height(LINE_HEIGHT)
                        .child(RectWidget::new().background(SurfaceRole::Raised)),
                ),
            ),
        );
        let columns = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(
                Expand::new()
                    .flex(5.0)
                    .child(Padding::new(0.0, LEFT_TRAILING, 0.0, 0.0).child(left)),
            )
            .child(Expand::vertical().child(divider))
            .child(Expand::new().flex(4.0).child(crossfade(vm)));
        let root = ctx.add(
            column(0.0)
                .child(Padding::new(TOP, 0.0, 0.0, 0.0).child(Expand::new().child(columns)))
                .child(OnShown {
                    vm: vm.clone(),
                    shown: vm.shown(),
                })
                .access_role(Role::Group)
                .access_label(tr!(ac_shell_tab_volume_mix())),
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
