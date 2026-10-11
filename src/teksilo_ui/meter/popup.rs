//! [`MeterPopup`]: the meter's settings.
//!
//! Port of `meter_popup_view` (`src/gui/widgets.rs:5193-5370`): a 280 px
//! panel in `COLOR_CONTRAST` with a 1 px secondary border and a 6 px radius,
//! 10 px of padding above and below and 12 px on the sides, holding, top to
//! bottom (Inter Medium, the spaces in px):
//!
//! | Row | Control |
//! | --- | --- |
//! | title, 14 | "Configuración del Meter" |
//! | 8 | |
//! | show, 13 | checkbox, 5, "Mostrar medidor" |
//! | 8 | |
//! | hold time, 13 | "Tiempo de hold del pico:", fill, stepper 500-5000 ms |
//! | 6 | |
//! | RMS window, 13 | "Ventana RMS:", fill, stepper 50-1000 ms |
//! | 8 | |
//! | M/S, 13 | checkbox, 5, "Modo M/S" |
//! | 6 | |
//! | infinite hold, 13 | checkbox, 5, "Hold infinito" |
//!
//! A toggle's label is primary while its option is on and secondary while it
//! is off, and a press on it flips the option like the checkbox does. The
//! hold time and the RMS window are Iced stubs, kept: they are stored and
//! persisted, and the meter does not read them (see
//! [`MeterViewModel`](super::MeterViewModel)).
//!
//! Not in Iced: a "Restablecer pico" button under the toggles (the Iced popup
//! has none; its `MeterResetPeak` message is never sent), so the held peaks
//! can be cleared.
//!
//! **The popup stays open while its values change** (quirk fix 10: Iced
//! closes it after each change, `app.rs:4909-4976`). It closes on Escape and
//! on a click outside (quirk fix 4), which the popover hosting it provides.

use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, VAlignment};
use teksilo::widgets::{Checkbox, FixedSize, HStack, Spacer, TextWidget, VStack};

use super::meter_vm::MeterViewModel;
use crate::logic::stepper::StepperUnit;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Stepper;
use crate::teksilo_ui::widgets::dialog_frame::{Frame, text_button};
use crate::teksilo_ui::widgets::stock_styles::checkbox;

/// The popup's width (`widgets.rs:5336`).
pub const POPUP_WIDTH: f32 = 280.0;
/// The padding above and below the content (`widgets.rs:5329`).
pub const PADDING_VERTICAL: f32 = 10.0;
/// The padding on the sides (`widgets.rs:5331`).
pub const PADDING_HORIZONTAL: f32 = 12.0;
/// The border (`widgets.rs:5343`).
pub const BORDER_WIDTH: f32 = 1.0;
/// The corner radius (`widgets.rs:5342`).
pub const RADIUS: f32 = 6.0;
/// The title's size (`widgets.rs:5217`).
pub const TITLE_SIZE: f32 = 14.0;
/// The labels' size (`widgets.rs:5228`).
pub const LABEL_SIZE: f32 = 13.0;
/// The space between a checkbox and its label (`widgets.rs:5260`).
pub const CHECKBOX_GAP: f32 = 5.0;
/// The space under the title and above the toggles (`widgets.rs:5324`).
pub const GAP_LARGE: f32 = 8.0;
/// The space between the two steppers and between the last two toggles
/// (`widgets.rs:5326`).
pub const GAP_SMALL: f32 = 6.0;

/// A vertical gap.
fn vgap(height: f32) -> FixedSize {
    FixedSize::new()
        .width(0.0)
        .height(height)
        .child(Spacer::new())
}

/// A horizontal gap.
fn hgap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

/// A row's text in Inter Medium.
fn label(text: LocalizedString, size: f32, color: impl Into<ColorProp>) -> TextWidget {
    TextWidget::new(text)
        .style(style::inter(size, FontWeight::MEDIUM))
        .color(color)
        .single_line()
}

/// A checkbox, 5 px, and its label (primary while on, secondary while off);
/// a press on the label flips the option.
fn toggle_row(
    value: Signal<bool>,
    text: LocalizedString,
    on_change: impl Fn(bool, &mut EventContext) + 'static,
) -> impl Widget {
    let on_change = std::rc::Rc::new(on_change);
    let flip = on_change.clone();
    let flipped = value.clone();
    HStack::new()
        .spacing(0.0)
        .alignment(VAlignment::Center)
        .child(checkbox(
            Checkbox::new(value.clone()).on_change({
                let on_change = on_change.clone();
                move |on, ctx| on_change(on, ctx)
            }),
            text.clone(),
        ))
        .child(hgap(CHECKBOX_GAP))
        .child(
            label(
                text,
                LABEL_SIZE,
                value.map(|on| {
                    if *on {
                        TextRole::Primary
                    } else {
                        TextRole::Secondary
                    }
                }),
            )
            .a11y_hidden()
            .on_tap(move |_, ctx| flip(!flipped.get(), ctx)),
        )
        .child(Spacer::new())
}

/// A label, the space that fills, and a stepper.
fn stepper_row(text: LocalizedString, stepper: Stepper) -> impl Widget {
    HStack::new()
        .spacing(0.0)
        .alignment(VAlignment::Center)
        .child(label(text, LABEL_SIZE, TextRole::Primary))
        .child(Spacer::new())
        .child(stepper)
}

/// The meter's settings; see the module documentation.
pub struct MeterPopup {
    vm: MeterViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for MeterPopup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeterPopup").finish_non_exhaustive()
    }
}

impl MeterPopup {
    /// The popup of `vm`.
    pub fn new(vm: MeterViewModel) -> Self {
        Self { vm, root: None }
    }
}

impl Widget for MeterPopup {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        let (show_vm, ms_vm, inf_vm, hold_vm, rms_vm, reset_vm) = (
            vm.clone(),
            vm.clone(),
            vm.clone(),
            vm.clone(),
            vm.clone(),
            vm.clone(),
        );
        let hold = Stepper::new(
            vm.hold_time_ms(),
            500.0..=5000.0,
            StepperUnit::Milliseconds,
            tr!(meter_hold_time_name()),
        )
        .on_change(move |ms| hold_vm.set_hold_time_ms(ms));
        let rms = Stepper::new(
            vm.rms_window_ms(),
            50.0..=1000.0,
            StepperUnit::Milliseconds,
            tr!(meter_rms_window_name()),
        )
        .on_change(move |ms| rms_vm.set_rms_window_ms(ms));

        let content = VStack::new()
            .spacing(0.0)
            .alignment(teksilo::tokens::HAlignment::Leading)
            .child(label(
                tr!(meter_popup_title()),
                TITLE_SIZE,
                TextRole::Primary,
            ))
            .child(vgap(GAP_LARGE))
            .child(toggle_row(
                vm.enabled(),
                tr!(meter_show()),
                move |on, ctx| show_vm.set_enabled(on, ctx),
            ))
            .child(vgap(GAP_LARGE))
            .child(stepper_row(tr!(meter_hold_time()), hold))
            .child(vgap(GAP_SMALL))
            .child(stepper_row(tr!(meter_rms_window()), rms))
            .child(vgap(GAP_LARGE))
            .child(toggle_row(
                vm.ms_mode(),
                tr!(meter_ms_mode()),
                move |on, ctx| ms_vm.set_ms_mode(on, ctx),
            ))
            .child(vgap(GAP_SMALL))
            .child(toggle_row(
                vm.infinite_hold(),
                tr!(meter_infinite_hold()),
                move |on, ctx| inf_vm.set_infinite_hold(on, ctx),
            ))
            .child(vgap(GAP_LARGE))
            .child(
                text_button(tr!(meter_reset_peak()), LABEL_SIZE, (3.0, 8.0))
                    .on_activate(move |_| reset_vm.reset_peaks()),
            );

        let inner_width = POPUP_WIDTH - 2.0 * PADDING_HORIZONTAL;
        let panel = Frame::new(FixedSize::new().width(inner_width).child(content))
            .fill(SurfaceRole::Raised)
            .border(TextRole::Secondary, BORDER_WIDTH)
            .radius(RADIUS)
            .padding(
                PADDING_VERTICAL,
                PADDING_HORIZONTAL,
                PADDING_VERTICAL,
                PADDING_HORIZONTAL,
            )
            .min_width(POPUP_WIDTH);
        // The overlay offers at least the anchor's width: the panel keeps its own.
        let root = ctx.add(FixedSize::new().width(POPUP_WIDTH).child(panel));
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(POPUP_WIDTH, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
