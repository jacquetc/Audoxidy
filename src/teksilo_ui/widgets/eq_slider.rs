//! [`EqSlider`]: one equalizer band, or the preamp, of the Audio Center's
//! equalizer tab.
//!
//! The Iced UI builds it in `vertical_slider` (`src/gui/audio_center.rs:2881-2933`)
//! from the global `CustomSlider` (`src/gui/widgets.rs:3139-4531`): a vertical
//! slider 24 × 240 px over -9…+9 dB in steps of 0.1. Here it is the stock
//! Teksilo `Slider` with the Iced chrome painted by a custom style
//! ([`body::CustomSliderStyle`]), and the Iced behaviour around it:
//!
//! - **Look**: an 8 px track with no coloured part (the equalizer leaves
//!   `enable_colored_track` off), a 16 px square handle, red at rest and light
//!   under the pointer or while selected. While the equalizer is off the handle
//!   is dark, red under the pointer and outlined while selected
//!   (`audio_center.rs:2902-2908`); the slider still works, as in Iced.
//! - **Pointer**: a primary press moves the value to the pointer at once and
//!   starts a drag (`widgets.rs:3787-3824`); a secondary press resets the band
//!   to its default (`widgets.rs:3849-3855`), from anywhere on the slider.
//! - **Selection**: Iced's "selected" slider (the one last pressed) is the
//!   focused one here; it shows the selected handle colour and takes the arrow
//!   keys.
//! - **Keyboard**: Up and Down step by 0.1 (`widgets.rs:3897-3961`). Left and
//!   Right do nothing on this vertical slider, as in Iced. The stock slider
//!   adds Page Up / Page Down (±1 dB) and Home / End (-9 / +9 dB).
//! - **Value bubble**: "-3.0 dB" (Iced's `{:.1} dB`) while the slider is
//!   hovered, dragged or focused from the keyboard, through the stock
//!   `Slider::value_tooltip`; it is also the slider's accessible value text.
//! - **Accessibility**: a slider named after the band, with its numeric value,
//!   range and step, and a "Reset to default" custom action that does what the
//!   secondary click does.

pub mod body;
pub mod chrome;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;

use teksilo::canvas::Size;
use teksilo::core::styles::SliderOrientation;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::widgets::{FixedSize, Orientation, Slider};

use self::body::{CustomSliderStyle, SliderLook};
use crate::logic::eq::{EQ_GAIN_MAX, EQ_GAIN_MIN};

/// The slider's width (`audio_center.rs:2898`).
pub const WIDTH: f32 = 24.0;
/// The slider's height (`audio_center.rs:2899`).
pub const HEIGHT: f32 = 240.0;
/// The step of a drag and of the arrow keys (`widgets.rs:3223`).
pub const STEP: f32 = 0.1;

/// What a slider calls for every value the user produces.
pub type ChangeFn = Rc<dyn Fn(f32, &mut EventContext)>;

/// A hook run with the event context of a press.
pub(crate) type PressHook = Rc<dyn Fn(&mut EventContext)>;

/// One vertical equalizer slider; see the module documentation.
pub struct EqSlider {
    value: Signal<f32>,
    label: LocalizedString,
    default_value: f32,
    eq_enabled: Prop<bool>,
    on_change: Option<ChangeFn>,
    selected: Signal<bool>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for EqSlider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EqSlider")
            .field("label", &self.label.resolve_now())
            .field("value", &self.value.get())
            .finish_non_exhaustive()
    }
}

impl EqSlider {
    /// A band slider bound to `value` (in dB), announced as `label` (the
    /// band's frequency, or the preamp).
    pub fn new(value: Signal<f32>, label: impl Into<LocalizedString>) -> Self {
        Self {
            value,
            label: label.into(),
            default_value: 0.0,
            eq_enabled: Prop::Static(true),
            on_change: None,
            selected: Signal::new(false),
            root: None,
        }
    }

    /// The value a reset goes back to (default 0 dB, as every Iced band and
    /// the preamp, `audio_center.rs:2925`, `2974`).
    pub fn default_value(mut self, value: f32) -> Self {
        self.default_value = value;
        self
    }

    /// Whether the equalizer is on (default `true`). Off only changes the look.
    pub fn eq_enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.eq_enabled = enabled.into();
        self
    }

    /// What runs for every value the user produces: a press, each step of a
    /// drag, a key, an assistive-technology action, a reset. Not for writes to
    /// the bound signal.
    pub fn on_change(mut self, f: impl Fn(f32, &mut EventContext) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl Widget for EqSlider {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let eq_enabled = match &self.eq_enabled {
            Prop::Static(on) => ctx.signal(*on),
            Prop::Bound(signal) => signal.clone(),
        };
        let look = eq_enabled.map(|on| {
            if *on {
                SliderLook::EQ_ON
            } else {
                SliderLook::EQ_OFF
            }
        });
        let painted_size = Rc::new(Cell::new(Size::new(0.0, 0.0)));
        let writer = UserWrite::new(self.value.clone(), self.on_change.clone());
        let press = SliderPress {
            orientation: SliderOrientation::Vertical,
            range: (EQ_GAIN_MIN, EQ_GAIN_MAX),
            step: STEP,
            default_value: self.default_value,
            painted_size: painted_size.clone(),
            writer: writer.clone(),
            selected: self.selected.clone(),
            before_press: None,
        };
        let on_change = writer.clone();
        let reset = writer.clone();
        let default_value = self.default_value;
        let slider = Slider::new(self.value.clone(), EQ_GAIN_MIN, EQ_GAIN_MAX)
            .orientation(Orientation::Vertical)
            .step(STEP)
            .style(CustomSliderStyle {
                look,
                selected: self.selected.clone(),
                painted_size,
            })
            .label(self.label.clone())
            .value_tooltip(|value| tr!(controls_eq_value(value = format!("{value:.1}"))))
            .on_change(move |value, ctx| on_change.notify(value, ctx))
            .on_pointer_event(move |event, ctx| press.handle(event, ctx))
            .access_custom_action(tr!(controls_reset_default()), move |ctx| {
                reset.set(default_value, ctx)
            });

        let slider_focused = Signal::new(false);
        track_selection(ctx, &slider_focused, &self.selected, None);
        let selected = self.selected.clone();
        ctx.apply_self_handlers(HandlerSet::new().on_key_preview(move |event, _ctx| {
            arrow_keys(event, SliderOrientation::Vertical, &selected)
        }));
        let root = ctx.add(
            FixedSize::new()
                .width(WIDTH)
                .height(HEIGHT)
                .child(slider)
                .focus_within(slider_focused),
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

/// A write to a slider's value made by the user: it reaches the bound signal
/// and the `on_change` callback, unless it changes nothing.
#[derive(Clone)]
pub(crate) struct UserWrite {
    value: Signal<f32>,
    on_change: Option<ChangeFn>,
}

impl UserWrite {
    /// Writes `value` and `on_change` together.
    pub(crate) fn new(value: Signal<f32>, on_change: Option<ChangeFn>) -> Self {
        Self { value, on_change }
    }

    /// Sets the value and reports it, unless it is already there.
    pub(crate) fn set(&self, value: f32, ctx: &mut EventContext) {
        if self.value.get() != value {
            self.value.set(value);
            self.notify(value, ctx);
        }
    }

    /// Reports a value the stock slider has already written.
    pub(crate) fn notify(&self, value: f32, ctx: &mut EventContext) {
        if let Some(f) = &self.on_change {
            f(value, ctx);
        }
    }
}

/// The Iced press behaviour of a `CustomSlider`, attached to the stock slider
/// with `on_pointer_event` (not `accept_tap_buttons`: that would also hand the
/// stock slider's own click-to-jump to the secondary button).
pub(crate) struct SliderPress {
    /// The slider's orientation.
    pub(crate) orientation: SliderOrientation,
    /// The value range.
    pub(crate) range: (f32, f32),
    /// The step a press snaps to.
    pub(crate) step: f32,
    /// What a secondary press resets to.
    pub(crate) default_value: f32,
    /// The slider's size, as last painted.
    pub(crate) painted_size: Rc<Cell<Size>>,
    /// How the value is written.
    pub(crate) writer: UserWrite,
    /// The selection flag the press sets.
    pub(crate) selected: Signal<bool>,
    /// What runs first on a primary press (the effect slider commits its
    /// text field there, as Iced does at `widgets.rs:3788-3797`).
    pub(crate) before_press: Option<PressHook>,
}

impl SliderPress {
    /// Handles one pointer event of the slider.
    ///
    /// - A secondary press resets the value and is consumed.
    /// - A primary press selects the slider and moves the value to the
    ///   pointer at once (the stock slider moves it on release or on the
    ///   drag); it is not consumed, so the stock drag follows.
    pub(crate) fn handle(&self, event: &WidgetEvent, ctx: &mut EventContext) -> EventResponse {
        let WidgetEvent::PointerDown {
            button, position, ..
        } = event
        else {
            return EventResponse::Ignored;
        };
        match button {
            PointerButton::Secondary => {
                self.writer.set(self.default_value, ctx);
                EventResponse::Handled
            }
            PointerButton::Primary => {
                if let Some(before) = &self.before_press {
                    before(ctx);
                }
                self.selected.set(true);
                let size = self.painted_size.get();
                if size.width > 0.0 && size.height > 0.0 {
                    let value = body::value_at(
                        *position,
                        size,
                        self.orientation,
                        ctx.is_rtl(),
                        self.range,
                        self.step,
                    );
                    self.writer.set(value, ctx);
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }
}

/// Keeps `selected` in step with the slider's focus: on when it gains focus,
/// off when it loses it. When `quiet` is set as focus arrives, the slider is
/// focused without being selected (focus handed back by the effect slider's
/// text field, which Iced leaves unselected) and the flag is cleared.
pub(crate) fn track_selection(
    ctx: &mut BuildContext,
    focused: &Signal<bool>,
    selected: &Signal<bool>,
    quiet: Option<Rc<Cell<bool>>>,
) {
    let selected = selected.clone();
    ctx.effect(focused, move |focused| {
        if *focused {
            let quiet = quiet.as_ref().is_some_and(|quiet| quiet.replace(false));
            if !quiet {
                selected.set(true);
            }
        } else {
            selected.set(false);
        }
    });
}

/// The arrow keys of a focused `CustomSlider`: the pair along the slider is
/// left to the stock slider (and selects it, as a click would); the other
/// pair does nothing, as in Iced (`widgets.rs:3940-3951`), and is consumed so
/// the stock slider does not step on it.
pub(crate) fn arrow_keys(
    event: &WidgetEvent,
    orientation: SliderOrientation,
    selected: &Signal<bool>,
) -> EventResponse {
    let WidgetEvent::KeyDown { key, modifiers, .. } = event else {
        return EventResponse::Ignored;
    };
    if modifiers.ctrl() || modifiers.alt() || modifiers.super_key() {
        // An accelerator chord is the application's (docs/range-keyboard.md).
        return EventResponse::Ignored;
    }
    let vertical = matches!(orientation, SliderOrientation::Vertical);
    match key {
        Key::ArrowUp | Key::ArrowDown if vertical => {
            selected.set(true);
            EventResponse::Ignored
        }
        Key::ArrowLeft | Key::ArrowRight if !vertical => {
            selected.set(true);
            EventResponse::Ignored
        }
        Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => EventResponse::Handled,
        _ => EventResponse::Ignored,
    }
}
