//! [`Stepper`]: the number stepper of the Audio Center's volume-and-mix tab
//! and of the meter settings.
//!
//! The Iced UI's `NumberStepper` (`src/gui/widgets.rs:214-854`) is a 96 × 13
//! px control: a "-" zone and a "+" zone, 15 px each, around a 66 px value
//! zone showing the value with its unit ("1000 ms", "-14.25 dB"). The stock
//! Teksilo `SpinBox` cannot take its place: it only offers stacked or hidden
//! buttons, and its repeat speeds up from 120 to 45 ms. This is a custom
//! widget doing what the Iced one does:
//!
//! - **Arrows**: a primary press on "-" or "+" steps once (50 ms or 0.25 dB,
//!   [`StepperUnit::step`]) and, held, repeats after 0.4 s then every 0.1 s
//!   ([`hold`]); the arrow under the pointer lights up in the accent colour.
//!   The repeat is driven by one-shot wake deadlines re-armed from a
//!   frame-tick effect, and the number of steps owed comes from the time
//!   since the press. Nothing is scheduled once the button is released or the
//!   pointer is cancelled.
//! - **Value**: a primary press on it opens a text field over it, seeded with
//!   the bare number (`logic::stepper::edit_seed`) and accepting digits,
//!   ".", "-" and spaces. Enter commits the number clamped to the range and
//!   **not** rounded to the step (`logic::stepper::parse_commit`); Escape
//!   cancels; leaving the field commits it. Pressing an arrow while editing
//!   commits first, and steps from the committed value (`widgets.rs:555-570`).
//! - **Reset**: a secondary press anywhere resets to the call site's default,
//!   when it gives one (the meter settings' steppers have none).
//! - **Disabled**: dimmed text, no highlight, no interaction.
//! - **Keyboard** (not in Iced, which gives the stepper no focus): Up and Down
//!   step while it has focus; Enter opens the text field.
//! - **Accessibility**: a spin button with its name, numeric value, range and
//!   step, the value text with its unit, the increment, decrement and
//!   set-value actions, and a "Reset to default" action when it can reset.
//!
//! `on_change` takes no event context: the repeats of a held arrow come from
//! the frame tick, where there is none.
//!
//! The text field exists only while editing: opening it rebuilds the stepper
//! with the field in it, focused from `build`. A field kept in a hidden
//! `Switcher` page loses the focus it is given in the handler that shows the
//! page: the page is still dormant when the next layout checks the focus.

pub mod face;
pub mod hold;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::time::Instant;

use teksilo::core::accesskit::{Action, ActionData, Role};
use teksilo::core::binding::BindingLevel;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::Alignment;
use teksilo::widgets::{FixedSize, Padding, TextFieldHandle, TextInputField, ZStack};

use self::face::{ARROW_WIDTH, HEIGHT, StepperFace, VALUE_WIDTH, Zone};
use self::hold::HoldRepeat;
use crate::logic::stepper::{
    EDIT_BUFFER_MAX_LEN, StepperUnit, accepts_input_char, edit_seed, format_stepper_value,
    parse_commit, step_value,
};

/// The clock the hold-to-repeat reads (the real one outside tests).
pub type Clock = Rc<dyn Fn() -> Instant>;

/// What a stepper calls for every value the user produces.
pub type StepperChangeFn = Rc<dyn Fn(f64)>;

/// The width of one character for placing the caret where the value was
/// pressed: Iced's `12 × 0.6` (`widgets.rs:576-582`).
const CARET_CHAR_WIDTH: f32 = 12.0 * 0.6;

/// A number stepper; see the module documentation.
pub struct Stepper {
    value: Signal<f64>,
    range: RangeInclusive<f64>,
    unit: StepperUnit,
    label: LocalizedString,
    reset_value: Option<f64>,
    enabled: Prop<bool>,
    on_change: Option<StepperChangeFn>,
    clock: Clock,
    hovered: Signal<Zone>,
    editing: Signal<bool>,
    text: Signal<String>,
    enabled_now: Rc<Cell<bool>>,
    core: Option<Rc<Core>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for Stepper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stepper")
            .field("label", &self.label.resolve_now())
            .field("value", &self.value.get())
            .field("range", &self.range)
            .field("unit", &self.unit)
            .finish_non_exhaustive()
    }
}

impl Stepper {
    /// A stepper bound to `value` within `range`, stepping by `unit`'s step,
    /// announced as `label`.
    pub fn new(
        value: Signal<f64>,
        range: RangeInclusive<f64>,
        unit: StepperUnit,
        label: impl Into<LocalizedString>,
    ) -> Self {
        Self {
            value,
            range,
            unit,
            label: label.into(),
            reset_value: None,
            enabled: Prop::Static(true),
            on_change: None,
            clock: Rc::new(Instant::now),
            hovered: Signal::new(Zone::None),
            editing: Signal::new(false),
            text: Signal::new(String::new()),
            enabled_now: Rc::new(Cell::new(true)),
            core: None,
            root: None,
        }
    }

    /// The value a secondary press resets to (Iced's `on_right_click`).
    /// Without it a secondary press does nothing.
    pub fn reset_to(mut self, value: f64) -> Self {
        self.reset_value = Some(value);
        self
    }

    /// Whether it can be used (default `true`; Iced's `disabled()`).
    pub fn enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    /// What runs for every value the user produces: an arrow, each repeat, a
    /// committed edit, a reset, a key, an assistive-technology action. Not for
    /// writes to the bound signal.
    pub fn on_change(mut self, f: impl Fn(f64) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Replaces the clock the hold-to-repeat reads.
    #[cfg(test)]
    pub(crate) fn clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// The shared state, made on the first build and kept across rebuilds.
    fn core(&mut self, ctx: &BuildContext) -> Rc<Core> {
        let core = self.core.get_or_insert_with(|| {
            Rc::new(Core {
                value: self.value.clone(),
                range: self.range.clone(),
                unit: self.unit,
                on_change: self.on_change.clone(),
                clock: self.clock.clone(),
                enabled: self.enabled_now.clone(),
                hovered: self.hovered.clone(),
                editing: self.editing.clone(),
                text: self.text.clone(),
                field: RefCell::new(None),
                pending_caret: Cell::new(None),
                root: Cell::new(None),
                hold: Cell::new(None),
                wake: ctx.wake_at_handle(),
                pending_wake: Cell::new(None),
            })
        });
        core.clone()
    }
}

/// Keep the earlier of the window's wake deadline and `at`, unless the
/// deadline in place is `mine` (this stepper's previous one, served or
/// stale), which is replaced.
fn wake_by(wake: &Cell<Option<Instant>>, at: Instant, mine: Option<Instant>) {
    let next = match wake.get() {
        Some(current) if Some(current) != mine && current <= at => current,
        _ => at,
    };
    wake.set(Some(next));
}

/// The open text field: a handle on its live text.
struct OpenField {
    handle: TextFieldHandle,
}

/// The state the stepper's handlers share; it outlives rebuilds.
struct Core {
    value: Signal<f64>,
    range: RangeInclusive<f64>,
    unit: StepperUnit,
    on_change: Option<StepperChangeFn>,
    clock: Clock,
    enabled: Rc<Cell<bool>>,
    hovered: Signal<Zone>,
    editing: Signal<bool>,
    text: Signal<String>,
    field: RefCell<Option<OpenField>>,
    pending_caret: Cell<Option<usize>>,
    root: Cell<Option<WidgetId>>,
    hold: Cell<Option<HoldRepeat>>,
    wake: Rc<Cell<Option<Instant>>>,
    pending_wake: Cell<Option<Instant>>,
}

impl Core {
    /// Sets the value and reports it, unless it is already there.
    fn write(&self, value: f64) {
        if self.value.get() != value {
            self.value.set(value);
            if let Some(f) = &self.on_change {
                f(value);
            }
        }
    }

    /// One step in direction `dir` from `base`, clamped.
    fn step_from(&self, base: f64, dir: i8) {
        self.write(step_value(base, dir, &self.unit, &self.range));
    }

    /// The field's text as typed (the bound signal catches up a frame late).
    fn typed(&self) -> String {
        match &*self.field.borrow() {
            Some(open) if open.handle.is_live() => open.handle.text(),
            _ => self.text.get(),
        }
    }

    /// Closes an open field and writes its number, clamped and not rounded.
    /// Returns the committed value; `None` when the field was not open or its
    /// text is not a number.
    fn commit(&self) -> Option<f64> {
        if !self.editing.get() {
            return None;
        }
        self.editing.set(false);
        let committed = parse_commit(&self.typed(), &self.range);
        if let Some(value) = committed {
            self.write(value);
        }
        committed
    }

    /// Opens the field on the bare number, caret at `caret` (or at the end).
    /// The rebuild this causes adds the field and focuses it.
    fn begin_edit(&self, caret: Option<usize>) {
        let seed = edit_seed(self.value.get(), &self.unit);
        let caret = caret.unwrap_or(seed.len()).min(seed.len());
        self.text.set(seed);
        self.pending_caret.set(Some(caret));
        self.editing.set(true);
    }

    /// Hands the focus back to the stepper after Enter or Escape.
    fn refocus(&self, ctx: &mut EventContext) {
        if let Some(root) = self.root.get() {
            ctx.request_focus(root);
        }
    }

    /// A press on an arrow: commit an open field, step from the result, and
    /// start the hold-to-repeat.
    fn press_arrow(&self, dir: i8) {
        let base = self.commit().unwrap_or_else(|| self.value.get());
        self.step_from(base, dir);
        let hold = HoldRepeat::new(dir, (self.clock)());
        self.hold.set(Some(hold));
        self.schedule(hold.next_deadline());
    }

    /// The frame-tick work: the steps a held arrow owes, and the next wake.
    fn tick(&self) {
        let Some(mut hold) = self.hold.get() else {
            return;
        };
        for _ in 0..hold.advance((self.clock)()) {
            self.step_from(self.value.get(), hold.dir);
        }
        self.hold.set(Some(hold));
        self.schedule(hold.next_deadline());
    }

    /// The arrow is released (or the pointer cancelled): stop repeating and
    /// take back the wake deadline when it is still ours.
    fn release(&self) {
        self.hold.set(None);
        if let Some(pending) = self.pending_wake.take()
            && self.wake.get() == Some(pending)
        {
            self.wake.set(None);
        }
    }

    fn schedule(&self, at: Instant) {
        wake_by(&self.wake, at, self.pending_wake.get());
        self.pending_wake.set(Some(at));
    }

    /// The pointer contract of the stepper's own node.
    fn pointer(
        &self,
        reset: Option<f64>,
        event: &WidgetEvent,
        ctx: &mut EventContext,
    ) -> EventResponse {
        if !self.enabled.get() {
            return EventResponse::Ignored;
        }
        match event {
            WidgetEvent::PointerDown {
                button: PointerButton::Secondary,
                ..
            } => {
                if let Some(value) = reset {
                    self.write(value);
                }
                EventResponse::Handled
            }
            WidgetEvent::PointerDown {
                button: PointerButton::Primary,
                position,
                ..
            } => {
                let zone = Zone::at(*position);
                self.hovered.set_if_changed(arrow_only(zone));
                if let Some(dir) = zone.direction() {
                    self.press_arrow(dir);
                    ctx.capture_pointer();
                    EventResponse::Handled
                } else if zone == Zone::Value && !self.editing.get() {
                    let seed = edit_seed(self.value.get(), &self.unit);
                    self.begin_edit(Some(caret_at(position.x - ARROW_WIDTH, &seed)));
                    EventResponse::Handled
                } else {
                    EventResponse::Ignored
                }
            }
            WidgetEvent::PointerMove { position, .. } => {
                self.hovered.set_if_changed(arrow_only(Zone::at(*position)));
                EventResponse::Ignored
            }
            WidgetEvent::PointerUp { .. } => {
                self.release();
                EventResponse::Ignored
            }
            WidgetEvent::PointerLeave { .. } => {
                self.hovered.set_if_changed(Zone::None);
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }

    /// The keys of the focused stepper: Up and Down step, Enter edits.
    fn key(&self, event: &WidgetEvent) -> EventResponse {
        let WidgetEvent::KeyDown { key, modifiers, .. } = event else {
            return EventResponse::Ignored;
        };
        if !self.enabled.get() || modifiers.ctrl() || modifiers.alt() || modifiers.super_key() {
            return EventResponse::Ignored;
        }
        match key {
            Key::ArrowUp => self.step_from(self.value.get(), 1),
            Key::ArrowDown => self.step_from(self.value.get(), -1),
            Key::Enter => self.begin_edit(None),
            _ => return EventResponse::Ignored,
        }
        EventResponse::Handled
    }

    /// The keys seen before the open field: Escape cancels the edit.
    fn field_key(&self, event: &WidgetEvent, ctx: &mut EventContext) -> EventResponse {
        match event {
            WidgetEvent::KeyDown {
                key: Key::Escape, ..
            } if self.editing.get() => {
                self.editing.set(false);
                self.refocus(ctx);
                EventResponse::Handled
            }
            _ => EventResponse::Ignored,
        }
    }

    /// The increment, decrement and set-value actions.
    fn access(&self, action: Action, data: Option<&ActionData>) -> EventResponse {
        if !self.enabled.get() {
            return EventResponse::Ignored;
        }
        match (action, data) {
            (Action::Increment, _) => self.step_from(self.value.get(), 1),
            (Action::Decrement, _) => self.step_from(self.value.get(), -1),
            (Action::SetValue, Some(ActionData::NumericValue(value))) => {
                self.write(value.clamp(*self.range.start(), *self.range.end()));
            }
            (Action::SetValue, Some(ActionData::Value(text))) => {
                match parse_commit(text, &self.range) {
                    Some(value) => self.write(value),
                    None => return EventResponse::Ignored,
                }
            }
            _ => return EventResponse::Ignored,
        }
        EventResponse::Handled
    }
}

/// Only the arrow zones light up.
fn arrow_only(zone: Zone) -> Zone {
    if zone.direction().is_some() {
        zone
    } else {
        Zone::None
    }
}

/// Where Iced puts the caret when the value is pressed `x` px into the value
/// zone: one character per 7.2 px (`widgets.rs:574-582`).
pub fn caret_at(x: f32, seed: &str) -> usize {
    ((x.max(0.0) / CARET_CHAR_WIDTH).round() as usize).min(seed.len())
}

impl Stepper {
    /// The text field over the value zone, focused, while editing.
    fn field(&self, ctx: &mut BuildContext, core: &Rc<Core>) -> WidgetId {
        let field = TextInputField::new(self.text.clone())
            .char_filter(accepts_input_char)
            .max_length(EDIT_BUFFER_MAX_LEN)
            .text_height(HEIGHT);
        let handle = field.handle();
        let set_caret = field.caret_setter();
        let (submit, blur) = (core.clone(), core.clone());
        let field = ctx.add(
            field
                .on_submit_fn(move |ctx| {
                    submit.commit();
                    submit.refocus(ctx);
                })
                .on_blur_fn(move |_| {
                    blur.commit();
                })
                .access_label(self.label.clone()),
        );
        *core.field.borrow_mut() = Some(OpenField { handle });
        ctx.focus(field);
        if let Some(caret) = core.pending_caret.take() {
            set_caret(caret);
        }
        ctx.add(
            Padding::new(0.0, 0.0, 0.0, ARROW_WIDTH).child(
                FixedSize::new()
                    .width(VALUE_WIDTH)
                    .height(HEIGHT)
                    .child(field),
            ),
        )
    }
}

impl Widget for Stepper {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let self_id = ctx.self_id();
        let core = self.core(ctx);
        core.root.set(Some(self_id));
        let enabled = match &self.enabled {
            Prop::Static(on) => ctx.signal(*on),
            Prop::Bound(signal) => signal.clone(),
        };
        self.enabled_now.set(enabled.get());
        ctx.enabled_when(self_id, Prop::Bound(enabled.clone()));
        let registry = ctx.binding_registry();
        self.editing
            .bind_to(self_id, registry, BindingLevel::Rebuild);
        self.value
            .bind_to(self_id, registry, BindingLevel::AccessibilityOnly);
        enabled.bind_to(self_id, registry, BindingLevel::AccessibilityOnly);

        // Disabling ends whatever was going on.
        let (enabled_now, stop) = (self.enabled_now.clone(), core.clone());
        ctx.effect(&enabled, move |on| {
            enabled_now.set(*on);
            if !*on {
                stop.release();
                stop.editing.set(false);
                stop.hovered.set(Zone::None);
            }
        });
        let ticking = core.clone();
        ctx.effect(&ctx.frame_tick(), move |_| ticking.tick());

        let field = if self.editing.get() {
            Some(self.field(ctx, &core))
        } else {
            *core.field.borrow_mut() = None;
            None
        };

        let face = ctx.add(StepperFace {
            value: self.value.clone(),
            unit: self.unit,
            hovered: self.hovered.clone(),
            editing: self.editing.clone(),
        });
        let registry = ctx.binding_registry();
        self.value
            .bind_to(face, registry, BindingLevel::RepaintOnly);
        self.hovered
            .bind_to(face, registry, BindingLevel::RepaintOnly);

        let reset = self.reset_value;
        let (pointer, cancel, key, preview, access) = (
            core.clone(),
            core.clone(),
            core.clone(),
            core.clone(),
            core.clone(),
        );
        let mut handlers = HandlerSet::new()
            .focusable(true)
            .on_pointer_event(move |event, ctx| pointer.pointer(reset, event, ctx))
            .on_pointer_cancel(move |_pointer, _reason, _ctx| cancel.release())
            .on_key(move |event, _ctx| key.key(event))
            .on_key_preview(move |event, ctx| preview.field_key(event, ctx))
            .on_access_action_request(move |action, _node, data, _ctx| {
                access.access(action, data.as_ref())
            });
        if let Some(value) = reset {
            let resetting = core.clone();
            handlers = handlers.access_custom_action(tr!(controls_reset_default()), move |_ctx| {
                if resetting.enabled.get() {
                    resetting.write(value);
                }
            });
        }
        ctx.apply_self_handlers(handlers);

        let mut stack = ZStack::new().alignment(Alignment::TOP_LEADING).child(face);
        if let Some(field) = field {
            stack = stack.child(field);
        }
        let root = ctx.add(stack);
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

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        let value = self.value.get();
        builder.set_role(Role::SpinButton);
        builder.set_name(self.label.resolve_now());
        builder.set_value(format_stepper_value(value, &self.unit));
        builder.set_numeric_value(value);
        builder.set_min_numeric_value(*self.range.start());
        builder.set_max_numeric_value(*self.range.end());
        builder.set_numeric_value_step(self.unit.step());
        if self.enabled_now.get() {
            builder.add_action(Action::Increment);
            builder.add_action(Action::Decrement);
            builder.add_action(Action::SetValue);
        } else {
            builder.set_disabled();
        }
    }
}
