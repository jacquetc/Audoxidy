//! [`EffectSlider`]: a parameter slider of the Audio Center's effects tab,
//! with the small value field beside it.
//!
//! The Iced UI builds it in `view_effect_with_secondary`
//! (`src/gui/audio_center.rs:3128-3215`) from the global `CustomSlider`
//! (`src/gui/widgets.rs:3139-4531`): a horizontal slider 18 px high that fills
//! the row, with the coloured track on, and a 40 × 18 text field 10 px after
//! it. Here the slider is the stock Teksilo `Slider` with the shared chrome
//! ([`CustomSliderStyle`](super::eq_slider::body::CustomSliderStyle),
//! [`SliderLook::EFFECT`]), and the field a `TextInputField` shown while
//! editing.
//!
//! - **Slider**: as the equalizer slider (press-to-jump, drag, secondary
//!   press resets, Left / Right step by the step, Up / Down inert on this
//!   horizontal slider), with a dark track filled in the accent colour up to
//!   the value, and no value bubble (the Iced call sites leave the tooltip
//!   off).
//! - **Field**: a 40 × 18 box (background colour, radius 4) showing the value
//!   with the call site's decimals, centred, 11 px. A primary press on it
//!   starts editing: the box gets an accent border and the value becomes an
//!   editable number, with the caret where Iced puts it
//!   (`widgets.rs:3779-3786`). Enter parses the text without its unit
//!   suffix, rounds it to the step and clamps it (`widgets.rs:3968-3987`);
//!   Escape cancels; leaving the field, or pressing the slider, commits it
//!   clamped but not rounded (`widgets.rs:3788-3797`, `3825-3836`), as Iced
//!   does. A secondary press on the box resets the value.
//! - **Keyboard**: Enter on the focused slider opens the field (the Iced field
//!   has no keyboard access); Enter or Escape in the field hands focus back to
//!   the slider.
//! - **Accessibility**: the slider carries the parameter's name, its numeric
//!   value, range and step, and a "Reset to default" action; the field, while
//!   open, is a text input with the same name.

#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use teksilo::canvas::{Canvas, Rect, Size};
use teksilo::core::binding::BindingLevel;
use teksilo::core::styles::SliderOrientation;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{CornerRadius, FontWeight, VAlignment};
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, Orientation, Padding, Slider, Switcher, TextFieldHandle,
    TextInputField, TextWidget, ZStack,
};

use super::eq_slider::body::{CustomSliderStyle, SliderLook, snap};
use super::eq_slider::chrome::Tone;
use super::eq_slider::{ChangeFn, PressHook, SliderPress, UserWrite, arrow_keys, track_selection};
use crate::logic::stepper::{EDIT_BUFFER_MAX_LEN, accepts_input_char, strip_suffix};
use crate::teksilo_ui::style;

/// The height of the slider and of the field (`audio_center.rs:3141`, `3146`).
pub const HEIGHT: f32 = 18.0;
/// The field's width (`audio_center.rs:3145`).
pub const INPUT_WIDTH: f32 = 40.0;
/// The space between the slider and the field (`widgets.rs:3237`).
pub const INPUT_GAP: f32 = 10.0;
/// The field's font size (`audio_center.rs:3153`).
pub const INPUT_FONT_SIZE: f32 = 11.0;
/// The field's corner radius (`audio_center.rs:3152`).
pub const INPUT_RADIUS: f32 = 4.0;
/// The field's border width (`audio_center.rs:3151`).
pub const INPUT_BORDER: f32 = 1.0;

/// One effect parameter: slider and value field; see the module documentation.
pub struct EffectSlider {
    value: Signal<f32>,
    range: (f32, f32),
    default_value: f32,
    step: f32,
    decimals: usize,
    label: LocalizedString,
    on_change: Option<ChangeFn>,
    selected: Signal<bool>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for EffectSlider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectSlider")
            .field("label", &self.label.resolve_now())
            .field("value", &self.value.get())
            .field("range", &self.range)
            .finish_non_exhaustive()
    }
}

impl EffectSlider {
    /// A parameter slider bound to `value` over `range`, reset to
    /// `default_value` by a secondary click, announced as `label`. The step
    /// defaults to 0.1 and the field shows one decimal, the Iced defaults
    /// (`audio_center.rs:3066-3068`).
    pub fn new(
        value: Signal<f32>,
        range: RangeInclusive<f32>,
        default_value: f32,
        label: impl Into<LocalizedString>,
    ) -> Self {
        Self {
            value,
            range: (*range.start(), *range.end()),
            default_value,
            step: 0.1,
            decimals: 1,
            label: label.into(),
            on_change: None,
            selected: Signal::new(false),
            root: None,
        }
    }

    /// The step of a drag, of the arrow keys and of the rounding on Enter
    /// (the call site's `primary_step_size`).
    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    /// How many decimals the field shows (Iced's `"{:.1}"` or `"{:.2}"`).
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// What runs for every value the user produces (press, drag, key,
    /// assistive technology, reset, a committed field). Not for writes to the
    /// bound signal.
    pub fn on_change(mut self, f: impl Fn(f32, &mut EventContext) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

/// The value as the field shows it.
pub fn format_value(value: f32, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

/// The value a field commits, or `None` when its text is not a number: the
/// unit suffix stripped, rounded to `step` when `round` is set (Enter), then
/// clamped to the range (`widgets.rs:3968-3987`, `3788-3797`).
pub fn parse_field(text: &str, range: (f32, f32), step: f32, round: bool) -> Option<f32> {
    let value = strip_suffix(text).parse::<f32>().ok()?;
    let value = if round { snap(value, step) } else { value };
    Some(value.clamp(range.0, range.1))
}

/// Where Iced puts the caret when the field is pressed `x` px from its left
/// edge: one character per `0.6 × font size` px (`widgets.rs:3780-3783`).
pub fn caret_at(x: f32, text: &str) -> usize {
    let char_width = INPUT_FONT_SIZE * 0.6;
    ((x / char_width).round().max(0.0) as usize).min(text.chars().count())
}

/// The editing state of the field, shared by its handlers.
struct Editor {
    editing: Signal<bool>,
    text: Signal<String>,
    handle: TextFieldHandle,
    value: Signal<f32>,
    writer: UserWrite,
    range: (f32, f32),
    step: f32,
    decimals: usize,
    from_pointer: Cell<bool>,
    quiet: Rc<Cell<bool>>,
    slider: Cell<Option<WidgetId>>,
    field: Cell<Option<WidgetId>>,
    set_caret: Rc<dyn Fn(usize)>,
}

impl Editor {
    /// Opens the field on the current value, the caret at `caret` (or at the
    /// end), and gives it the focus.
    fn begin(&self, caret: Option<usize>, from_pointer: bool, ctx: &mut EventContext) {
        let text = format_value(self.value.get(), self.decimals);
        let caret = caret.unwrap_or(text.chars().count());
        self.text.set(text);
        self.editing.set(true);
        self.from_pointer.set(from_pointer);
        if let Some(field) = self.field.get() {
            ctx.request_focus(field);
        }
        (self.set_caret)(caret);
    }

    /// Closes the field and writes its value, rounded to the step when
    /// `round` is set. Does nothing when the field is not open.
    fn commit(&self, round: bool, ctx: &mut EventContext) {
        if !self.editing.get() {
            return;
        }
        self.editing.set(false);
        if let Some(value) = parse_field(&self.typed(), self.range, self.step, round) {
            self.writer.set(value, ctx);
        }
    }

    /// The field's text as typed. The bound signal catches up with the field
    /// only on the next frame, so a commit reads the field itself.
    fn typed(&self) -> String {
        if self.handle.is_live() {
            self.handle.text()
        } else {
            self.text.get()
        }
    }

    /// Closes the field without writing.
    fn cancel(&self) {
        self.editing.set(false);
    }

    /// Hands the focus back to the slider after Enter or Escape. After an
    /// edit started with the pointer the slider is not selected, as in Iced.
    fn hand_back(&self, ctx: &mut EventContext) {
        if let Some(slider) = self.slider.get() {
            self.quiet.set(self.from_pointer.get());
            ctx.request_focus(slider);
        }
    }
}

impl Widget for EffectSlider {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let painted_size = Rc::new(Cell::new(Size::new(0.0, 0.0)));
        let writer = UserWrite::new(self.value.clone(), self.on_change.clone());
        let editing = ctx.signal(false);
        let text = ctx.signal(String::new());
        let quiet = Rc::new(Cell::new(false));

        let field = TextInputField::new(text.clone())
            .char_filter(accepts_input_char)
            .max_length(EDIT_BUFFER_MAX_LEN)
            .text_height(HEIGHT - 2.0 * INPUT_BORDER);
        let editor = Rc::new(Editor {
            editing: editing.clone(),
            text,
            handle: field.handle(),
            value: self.value.clone(),
            writer: writer.clone(),
            range: self.range,
            step: self.step,
            decimals: self.decimals,
            from_pointer: Cell::new(false),
            quiet: quiet.clone(),
            slider: Cell::new(None),
            field: Cell::new(None),
            set_caret: field.caret_setter(),
        });
        let (submit, blur) = (editor.clone(), editor.clone());
        let field = ctx.add(
            field
                .on_submit_fn(move |ctx| {
                    submit.commit(true, ctx);
                    submit.hand_back(ctx);
                })
                .on_blur_fn(move |ctx| blur.commit(false, ctx))
                .access_label(self.label.clone()),
        );
        editor.field.set(Some(field));

        let commit_first = editor.clone();
        let before_press: PressHook = Rc::new(move |ctx| commit_first.commit(false, ctx));
        let press = SliderPress {
            orientation: SliderOrientation::Horizontal,
            range: self.range,
            step: self.step,
            default_value: self.default_value,
            painted_size: painted_size.clone(),
            writer: writer.clone(),
            selected: self.selected.clone(),
            before_press: Some(before_press),
        };
        let (on_change, reset) = (writer.clone(), writer.clone());
        let default_value = self.default_value;
        let slider = ctx.add(
            Slider::new(self.value.clone(), self.range.0, self.range.1)
                .orientation(Orientation::Horizontal)
                .step(self.step)
                .style(CustomSliderStyle {
                    look: Signal::new(SliderLook::EFFECT),
                    selected: self.selected.clone(),
                    painted_size,
                })
                .label(self.label.clone())
                .on_change(move |value, ctx| on_change.notify(value, ctx))
                .on_pointer_event(move |event, ctx| press.handle(event, ctx))
                .access_custom_action(tr!(controls_reset_default()), move |ctx| {
                    reset.set(default_value, ctx)
                }),
        );
        editor.slider.set(Some(slider));
        let slider_focused = Signal::new(false);
        track_selection(ctx, &slider_focused, &self.selected, Some(quiet));

        let input = self.input(ctx, &editor, field, writer);
        let keys = editor.clone();
        let selected = self.selected.clone();
        ctx.apply_self_handlers(
            HandlerSet::new()
                .on_key_preview(move |event, ctx| field_keys(&keys, &selected, event, ctx)),
        );
        let root = ctx.add(
            FixedSize::new().height(HEIGHT).child(
                HStack::new()
                    .spacing(INPUT_GAP)
                    .alignment(VAlignment::Center)
                    .child(
                        Expand::horizontal().child(
                            FixedSize::new()
                                .height(HEIGHT)
                                .child(slider)
                                .focus_within(slider_focused),
                        ),
                    )
                    .child(input),
            ),
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

impl EffectSlider {
    /// The 40 × 18 value box: the frame, then the value or the open field.
    fn input(
        &self,
        ctx: &mut BuildContext,
        editor: &Rc<Editor>,
        field: WidgetId,
        writer: UserWrite,
    ) -> WidgetId {
        let decimals = self.decimals;
        let shown = self.value.map(move |value| format_value(*value, decimals));
        let page = editor.editing.map(|editing| usize::from(*editing));
        let frame = ctx.add(InputFrame {
            editing: editor.editing.clone(),
        });
        editor
            .editing
            .bind_to(frame, ctx.binding_registry(), BindingLevel::RepaintOnly);
        let press = editor.clone();
        let default_value = self.default_value;
        ctx.add(
            FixedSize::new()
                .width(INPUT_WIDTH)
                .height(HEIGHT)
                .child(
                    ZStack::new().child(frame).child(
                        Switcher::new(page)
                            .child(
                                Center::new().child(
                                    TextWidget::new(lit!(""))
                                        .text(shown)
                                        .style(style::inter(INPUT_FONT_SIZE, FontWeight::MEDIUM))
                                        .color(TextRole::Primary)
                                        .single_line(),
                                ),
                            )
                            .child(Padding::uniform(INPUT_BORDER).child(field)),
                    ),
                )
                .on_pointer_event(move |event, ctx| match event {
                    WidgetEvent::PointerDown {
                        button: PointerButton::Secondary,
                        ..
                    } => {
                        writer.set(default_value, ctx);
                        EventResponse::Handled
                    }
                    WidgetEvent::PointerDown {
                        button: PointerButton::Primary,
                        position,
                        ..
                    } if !press.editing.get() => {
                        let text = format_value(press.value.get(), press.decimals);
                        press.begin(Some(caret_at(position.x, &text)), true, ctx);
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                }),
        )
    }
}

/// The keys the widget handles before the focused slider or field does:
/// Escape cancels an open field; Enter on the slider opens it; the arrows
/// across this horizontal slider do nothing.
fn field_keys(
    editor: &Editor,
    selected: &Signal<bool>,
    event: &WidgetEvent,
    ctx: &mut EventContext,
) -> EventResponse {
    let WidgetEvent::KeyDown { key, modifiers, .. } = event else {
        return EventResponse::Ignored;
    };
    if editor.editing.get() {
        if *key == Key::Escape {
            editor.cancel();
            editor.hand_back(ctx);
            return EventResponse::Handled;
        }
        return EventResponse::Ignored;
    }
    if *key == Key::Enter && *modifiers == teksilo::core::Modifiers::NONE {
        editor.begin(None, false, ctx);
        return EventResponse::Handled;
    }
    arrow_keys(event, SliderOrientation::Horizontal, selected)
}

/// The field's box: the background colour, and a 1 px border that is the
/// background colour at rest and the accent while editing
/// (`widgets.rs:4462-4487`, `audio_center.rs:3147-3150`).
#[derive(Debug)]
struct InputFrame {
    editing: Signal<bool>,
}

impl Widget for InputFrame {
    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(INPUT_WIDTH, HEIGHT).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let colors = &ctx.theme.colors;
        let radius = CornerRadius::uniform(INPUT_RADIUS);
        canvas.fill_rounded_rect(bounds, radius, Tone::Background.resolve(colors));
        let border = if self.editing.get() {
            Tone::Accent
        } else {
            Tone::Background
        };
        let half = INPUT_BORDER / 2.0;
        canvas.stroke_rounded_rect(
            Rect::new(
                bounds.x + half,
                bounds.y + half,
                bounds.width - INPUT_BORDER,
                bounds.height - INPUT_BORDER,
            ),
            radius,
            border.resolve(colors),
            INPUT_BORDER,
        );
    }
}
