//! The Audoxidy look of Teksilo's stock controls.
//!
//! The Iced UI restyles a few stock Iced widgets with style closures
//! (`src/gui/widgets.rs`). Teksilo restyles its stock widgets through style
//! protocols: a widget keeps its behaviour (pointer, keyboard, AccessKit) and
//! hands the drawing of its body to a `*Style` object. This module holds those
//! objects, one per restyled control:
//!
//! | Iced (`src/gui/widgets.rs`)                    | Teksilo widget | Here                         |
//! | ---------------------------------------------- | -------------- | ---------------------------- |
//! | `StandardCheckbox` (55–208)                    | `Checkbox`     | [`checkbox_style`]           |
//! | `standard_toggler` / `_full` (1240–1317)       | `Toggle`       | [`toggle`], [`toggle_style`], [`ToggleLook`] |
//! | `standard_radio` (1317–1357)                   | `RadioButton`  | [`radio_style`], [`radio`]   |
//! | `standard_pick_list` (1357–1421)               | `ComboBox`     | [`combo_box_style`], [`combo_box`] |
//! | `custom_scrollbar_style` / `standard_scrollbar` (1543–1675) | `ScrollArea` bars | [`scroll_bar_style`], [`scroll_area`] |
//!
//! Each style can be passed per call (`Toggle::style`, …) or installed for the
//! whole app through the theme's style slots (`theme.style_slots.checkbox`, …):
//! the Iced UI has one look per control except the toggler, whose size and
//! colours vary per call site (see [`ToggleLook`]).
//!
//! Colours are theme roles: the Iced accent is `SurfaceRole::Accent` /
//! `TextRole::Accent`, `COLOR_BG` is `SurfaceRole::Main`, `COLOR_CONTRAST` is
//! `SurfaceRole::Raised`, and the two text colours are `TextRole::Primary` and
//! `TextRole::Secondary` (also where Iced paints a surface or a border in a
//! text colour). Roles dim by themselves in a disabled subtree.
//!
//! Two differences the framework imposes, both on purpose:
//!
//! - `Checkbox` and `RadioButton` keep a 24 px hit area around the body (the
//!   WCAG 2.5.8 target floor), so a 13 px checkbox occupies a 24 × 24 slot.
//!   The drawing is the Iced size.
//! - A stock widget toggles on the release of the press; the Iced checkbox
//!   toggles on the press.

use std::rc::Rc;

use teksilo::core::binding::BindingLevel;
use teksilo::core::environment::LayoutDirection;
use teksilo::core::styles::{
    CheckboxState, CheckboxStyle, CheckboxStyleConfig, ComboBoxStyle, ComboBoxStyleConfig,
    ComponentStyleSlots, RadioStyle, RadioStyleConfig, ScrollBarOrientation, ScrollBarStyle,
    ScrollBarStyleConfig, ToggleStyle, ToggleStyleConfig,
};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, VAlignment};
use teksilo::widgets::{
    ComboBox, FixedSize, HStack, IconWidget, Padding, RadioButton, RectWidget, ScrollArea, Spacer,
    TextWidget, Toggle, ZStack,
};

use crate::teksilo_ui::style;

// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

/// Width of the track border of every Iced toggler (`widgets.rs:1290`).
const TOGGLE_BORDER: f32 = 1.0;
/// The thumb's inset as a fraction of the track height (`padding_ratio`,
/// `widgets.rs:1309`), rounded to whole pixels as Iced does.
const TOGGLE_PADDING_RATIO: f32 = 0.2;

/// The size and colours of one Iced toggler.
///
/// `standard_toggler_full(is_active, on_toggle, size, active_color,
/// inactive_color, thumb_active_color, thumb_inactive_color,
/// inactive_bg_color)` (`widgets.rs:1262`): the track is `2 × size` wide and
/// `size` high, fully rounded, with a 1 px border; on, it is filled and
/// bordered with `active`; off, it is filled with `inactive_fill` and
/// bordered with `inactive_border`. The thumb sits `round(0.2 × size)` inside
/// the track, at the trailing end when on.
#[derive(Debug, Clone)]
pub struct ToggleLook {
    /// The track height in pixels (Iced's `size`).
    pub size: f32,
    /// Track fill and border when on.
    pub active: ColorProp,
    /// Track border when off.
    pub inactive_border: ColorProp,
    /// Track fill when off.
    pub inactive_fill: ColorProp,
    /// Thumb colour when on.
    pub thumb_on: ColorProp,
    /// Thumb colour when off.
    pub thumb_off: ColorProp,
}

impl ToggleLook {
    /// "Activar Ecualizador" (`audio_center.rs:2792`): 18 px, accent when on,
    /// a secondary outline when off; the thumb primary when on, secondary off.
    pub fn equalizer() -> Self {
        Self {
            size: 18.0,
            active: SurfaceRole::Accent.into(),
            inactive_border: TextRole::Secondary.into(),
            inactive_fill: SurfaceRole::Transparent.into(),
            thumb_on: TextRole::Primary.into(),
            thumb_off: TextRole::Secondary.into(),
        }
    }

    /// The effect cards' switches (`audio_center.rs:3116`): 17 px, accent
    /// when on, a secondary outline when off; the thumb in the background
    /// colour when on, secondary off.
    pub fn effect() -> Self {
        Self {
            size: 17.0,
            active: SurfaceRole::Accent.into(),
            inactive_border: TextRole::Secondary.into(),
            inactive_fill: SurfaceRole::Transparent.into(),
            thumb_on: SurfaceRole::Main.into(),
            thumb_off: TextRole::Secondary.into(),
        }
    }

    /// The stereo expander's Natural / Surround switch
    /// (`audio_center.rs:3475`): 13 px, a secondary track in both states and
    /// a primary thumb, so only the thumb's side shows the state.
    pub fn expander_mode() -> Self {
        Self {
            size: 13.0,
            active: TextRole::Secondary.into(),
            inactive_border: TextRole::Secondary.into(),
            inactive_fill: TextRole::Secondary.into(),
            thumb_on: TextRole::Primary.into(),
            thumb_off: TextRole::Primary.into(),
        }
    }

    /// The thumb's inset from the track edge, in pixels.
    pub fn thumb_inset(&self) -> f32 {
        (TOGGLE_PADDING_RATIO * self.size).round()
    }

    /// The thumb's diameter, in pixels.
    pub fn thumb_diameter(&self) -> f32 {
        self.size - 2.0 * self.thumb_inset()
    }
}

/// A stock `Toggle` in the Iced look `look`, announced as `label`.
///
/// The Iced togglers have no label of their own (the text beside them is a
/// separate widget), so the toggle shows none: it is labelled externally and
/// carries `label` as its accessible name.
pub fn toggle(on: Signal<bool>, look: ToggleLook, label: LocalizedString) -> impl Widget {
    Toggle::new(on)
        .labelled_externally()
        .style(toggle_style(look))
        .access_label(label)
}

/// The Iced toggler drawn for a stock `Toggle`; see [`ToggleLook`].
#[derive(Debug, Clone)]
pub struct AudoxidyToggleStyle {
    look: ToggleLook,
}

/// The toggle style for `look` (per call: `Toggle::style(toggle_style(…))`).
pub fn toggle_style(look: ToggleLook) -> AudoxidyToggleStyle {
    AudoxidyToggleStyle { look }
}

impl ToggleStyle for AudoxidyToggleStyle {
    fn make_body(&self, cfg: &ToggleStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        ctx.add(ToggleBody {
            look: self.look.clone(),
            on: cfg.is_on.clone(),
        })
    }
}

/// The painted track and thumb. Iced does not animate the thumb.
#[derive(Debug)]
struct ToggleBody {
    look: ToggleLook,
    on: Signal<bool>,
}

impl Widget for ToggleBody {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.on.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::RepaintOnly,
        );
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(2.0 * self.look.size, self.look.size).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let on = self.on.get();
        let enabled = ctx.effective_enabled;
        let look = &self.look;
        let (fill, border, thumb) = if on {
            (&look.active, &look.active, &look.thumb_on)
        } else {
            (&look.inactive_fill, &look.inactive_border, &look.thumb_off)
        };
        let radius = bounds.height / 2.0;
        canvas.fill_rounded_rect(
            bounds,
            CornerRadius::uniform(radius),
            fill.resolve(ctx.theme, enabled),
        );
        canvas.stroke_rounded_rect(
            inset(bounds, TOGGLE_BORDER / 2.0),
            CornerRadius::uniform(radius - TOGGLE_BORDER / 2.0),
            border.resolve(ctx.theme, enabled),
            TOGGLE_BORDER,
        );
        let pad = look.thumb_inset();
        let diameter = look.thumb_diameter();
        let trailing = on != (ctx.layout_direction == LayoutDirection::RightToLeft);
        let x = if trailing {
            bounds.x + bounds.width - bounds.height + pad
        } else {
            bounds.x + pad
        };
        canvas.fill_rounded_rect(
            Rect::new(x, bounds.y + pad, diameter, diameter),
            CornerRadius::uniform(diameter / 2.0),
            thumb.resolve(ctx.theme, enabled),
        );
    }
}

// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

/// The Iced checkbox's side (`widgets.rs:83`).
pub const CHECKBOX_SIZE: f32 = 13.0;
/// The size of the "✓" glyph (`widgets.rs:178`).
const CHECK_GLYPH_SIZE: f32 = 15.0;
/// The checkbox's corner radius and border width (`widgets.rs:166`).
const CHECKBOX_RADIUS: f32 = 1.0;
const CHECKBOX_BORDER: f32 = 1.0;

/// The Iced `StandardCheckbox` (`widgets.rs:55`): a 13 px square, radius 1,
/// with a 1 px border. Off, it is filled with the background colour and
/// bordered in the secondary text colour; on, it is filled and bordered with
/// the accent and shows a "✓" (Inter Medium 15 px, primary) centred.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudoxidyCheckboxStyle;

/// The checkbox style (per call: `Checkbox::style(checkbox_style())`).
pub fn checkbox_style() -> AudoxidyCheckboxStyle {
    AudoxidyCheckboxStyle
}

impl CheckboxStyle for AudoxidyCheckboxStyle {
    fn make_body(&self, cfg: &CheckboxStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let checked = cfg.state.map(|state| *state == CheckboxState::Checked);
        let mixed = cfg
            .state
            .map(|state| *state == CheckboxState::Indeterminate);
        let glyph = |text: &'static str, shown: Signal<bool>| {
            TextWidget::new(lit!(text))
                .style(style::inter(CHECK_GLYPH_SIZE, FontWeight::MEDIUM))
                .color(TextRole::Primary)
                .single_line()
                .no_shrink()
                .a11y_hidden()
                .visible_when(shown)
        };
        ctx.add(
            FixedSize::new()
                .width(CHECKBOX_SIZE)
                .height(CHECKBOX_SIZE)
                .child(
                    ZStack::new()
                        .child(CheckboxBox {
                            state: cfg.state.clone(),
                        })
                        .child(glyph("✓", checked))
                        // Not in the Iced UI, which has no third state: a dash.
                        .child(glyph("–", mixed)),
                ),
        )
    }
}

/// The checkbox square.
#[derive(Debug)]
struct CheckboxBox {
    state: Signal<CheckboxState>,
}

impl Widget for CheckboxBox {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.state.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::RepaintOnly,
        );
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(CHECKBOX_SIZE, CHECKBOX_SIZE).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let on = self.state.get() != CheckboxState::Unchecked;
        let (fill, border): (ColorProp, ColorProp) = if on {
            (SurfaceRole::Accent.into(), SurfaceRole::Accent.into())
        } else {
            (SurfaceRole::Main.into(), TextRole::Secondary.into())
        };
        let enabled = ctx.effective_enabled;
        canvas.fill_rounded_rect(
            bounds,
            CornerRadius::uniform(CHECKBOX_RADIUS),
            fill.resolve(ctx.theme, enabled),
        );
        canvas.stroke_rounded_rect(
            inset(bounds, CHECKBOX_BORDER / 2.0),
            CornerRadius::uniform(CHECKBOX_RADIUS),
            border.resolve(ctx.theme, enabled),
            CHECKBOX_BORDER,
        );
    }
}

// ---------------------------------------------------------------------------
// Radio button
// ---------------------------------------------------------------------------

/// The Iced radio's diameter (`widgets.rs:1332`).
pub const RADIO_SIZE: f32 = 16.0;
/// The Iced radio's border width (`widgets.rs:1346`).
const RADIO_BORDER: f32 = 1.3;
/// The space between the circle and the label (Iced's default spacing).
pub const RADIO_LABEL_SPACING: f32 = 8.0;
/// The label size (`widgets.rs:1333`).
pub const RADIO_TEXT_SIZE: f32 = 14.0;

/// The Iced `standard_radio` circle (`widgets.rs:1317`): 16 px, transparent,
/// a 1.3 px border; selected, the border and an 8 px centred dot are accent,
/// otherwise the border is primary.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudoxidyRadioStyle;

/// The radio style (per call: `RadioButton::style(radio_style())`).
pub fn radio_style() -> AudoxidyRadioStyle {
    AudoxidyRadioStyle
}

impl RadioStyle for AudoxidyRadioStyle {
    fn make_body(&self, cfg: &RadioStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        ctx.add(RadioCircle {
            selected: cfg.is_selected.clone(),
        })
    }
}

/// The painted circle and dot.
#[derive(Debug)]
struct RadioCircle {
    selected: Signal<bool>,
}

impl Widget for RadioCircle {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.selected.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::RepaintOnly,
        );
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(RADIO_SIZE, RADIO_SIZE).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let selected = self.selected.get();
        let enabled = ctx.effective_enabled;
        let border: ColorProp = if selected {
            SurfaceRole::Accent.into()
        } else {
            TextRole::Primary.into()
        };
        let center = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        canvas.stroke_circle(
            center,
            (bounds.width - RADIO_BORDER) / 2.0,
            border.resolve(ctx.theme, enabled),
            RADIO_BORDER,
        );
        if selected {
            // Iced insets the dot by half its own size: an 8 px dot.
            let dot = bounds.width / 2.0;
            canvas.fill_circle(
                center,
                dot / 2.0,
                ColorProp::from(SurfaceRole::Accent).resolve(ctx.theme, enabled),
            );
        }
    }
}

/// An Iced radio button: the styled `RadioButton` for `value` of `selected`,
/// followed 8 px later by `label` in Inter Medium 14 px, primary
/// (`widgets.rs:1317`). A click on the label selects it too, as in Iced.
///
/// The stock `RadioButton` draws its own label in the theme's body style, so
/// the label is a sibling here and the button carries the accessible name.
pub fn radio(value: usize, selected: Signal<usize>, label: LocalizedString) -> HStack {
    let select = selected.clone();
    // The button keeps a 24 px hit area with the 16 px circle at its leading
    // edge; the space after the circle counts towards the Iced spacing.
    let gap = (RADIO_LABEL_SPACING - (RADIO_HIT_AREA - RADIO_SIZE)).max(0.0);
    HStack::new()
        .spacing(gap)
        .alignment(VAlignment::Center)
        .child(
            RadioButton::new(value, selected)
                .style(radio_style())
                .access_label(label.clone()),
        )
        .child(
            TextWidget::new(label)
                .style(style::inter(RADIO_TEXT_SIZE, FontWeight::MEDIUM))
                .color(TextRole::Primary)
                .single_line()
                .a11y_hidden()
                .cursor(CursorIcon::Pointer)
                .on_tap(move |_, _| select.set(value)),
        )
}

/// The hit area the stock `RadioButton` keeps around its body.
const RADIO_HIT_AREA: f32 = 24.0;

// ---------------------------------------------------------------------------
// Combo box (Iced pick list)
// ---------------------------------------------------------------------------

/// The pick list's padding: 4 px above and below, 10 px on the sides
/// (`widgets.rs:1377`).
const COMBO_PADDING: (f32, f32) = (4.0, 10.0);
/// The pick list's text size (`widgets.rs:1379`).
pub const COMBO_TEXT_SIZE: f32 = 14.0;

/// The Iced `standard_pick_list` trigger (`widgets.rs:1357`): a square box
/// with no border, `COLOR_CONTRAST` at rest and accent while hovered or open,
/// the value and the arrow in the primary text colour.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudoxidyComboBoxStyle;

/// The combo box style (per call: `ComboBox::style(combo_box_style())`, or
/// through [`combo_box`]).
pub fn combo_box_style() -> AudoxidyComboBoxStyle {
    AudoxidyComboBoxStyle
}

impl ComboBoxStyle for AudoxidyComboBoxStyle {
    fn make_body(&self, cfg: &ComboBoxStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let lit = cfg.is_hovered.zip(&cfg.is_open).map(|(hovered, open)| {
            if *hovered || *open {
                SurfaceRole::Accent
            } else {
                SurfaceRole::Raised
            }
        });
        ctx.add(
            ZStack::new()
                .child(RectWidget::new().background(lit))
                .child(
                    Padding::symmetric(COMBO_PADDING.0, COMBO_PADDING.1).child(
                        HStack::new()
                            .spacing(0.0)
                            .alignment(VAlignment::Center)
                            .child(cfg.selected_label)
                            .child(Spacer::new())
                            // Iced's default handle is a down arrow at the text size.
                            .child(
                                IconWidget::chevron_down(COMBO_TEXT_SIZE).color(TextRole::Primary),
                            ),
                    ),
                ),
        )
    }
}

/// `combo` in the Iced pick-list look: the style above, the value in Inter
/// Medium 14 px, primary. The caller sizes it (the Iced call sites use fixed
/// widths, `audio_center.rs:2098`).
pub fn combo_box<T: Clone + PartialEq + 'static>(combo: ComboBox<T>) -> ComboBox<T> {
    combo
        .style(combo_box_style())
        .text_style(style::inter(COMBO_TEXT_SIZE, FontWeight::MEDIUM))
        .text_role(TextRole::Primary)
}

// ---------------------------------------------------------------------------
// Scroll bars
// ---------------------------------------------------------------------------

/// The bar's thickness: 8 px, no margin (`standard_scrollbar`,
/// `widgets.rs:1662`).
pub const SCROLL_BAR_THICKNESS: f32 = 8.0;
/// Pixels scrolled per wheel line (Iced's scrollable moves 60 px per line).
pub const SCROLL_LINE_HEIGHT: f32 = 60.0;

/// The Iced scrollbar (`custom_scrollbar_style`, `widgets.rs:1543`): no
/// track, and a square scroller of which only the trailing half is painted
/// (the gradient that is transparent up to 49 %): 4 px of an 8 px bar,
/// along the right edge of a vertical bar and the bottom edge of a horizontal
/// one. It is accent while dragged and primary while the pointer is on the
/// bar.
///
/// At rest the Iced scroller is transparent unless the pointer is anywhere over
/// the scrollable, where it turns secondary. A Teksilo scroll bar style is told
/// whether the pointer is over the *bar*, not over the area, so at rest it is
/// drawn secondary.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudoxidyScrollBarStyle;

/// The scroll bar style; install it theme-wide (`ScrollArea` has no per-call
/// bar style).
pub fn scroll_bar_style() -> AudoxidyScrollBarStyle {
    AudoxidyScrollBarStyle
}

impl ScrollBarStyle for AudoxidyScrollBarStyle {
    fn make_body(&self, cfg: &ScrollBarStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        ctx.add(ScrollThumb {
            cfg: ThumbState {
                scroll_ratio: cfg.scroll_ratio.clone(),
                viewport_ratio: cfg.viewport_ratio.clone(),
                hovered: cfg.is_hovered.clone(),
                dragging: cfg.is_dragging.clone(),
                idle: cfg.is_idle.clone(),
                vertical: cfg.orientation == ScrollBarOrientation::Vertical,
                min_length: cfg.min_thumb_length,
            },
        })
    }
}

/// What the thumb is drawn from.
#[derive(Debug)]
struct ThumbState {
    scroll_ratio: Signal<f32>,
    viewport_ratio: Signal<f32>,
    hovered: Signal<bool>,
    dragging: Signal<bool>,
    idle: Signal<bool>,
    vertical: bool,
    min_length: f32,
}

/// The painted scroller.
#[derive(Debug)]
struct ScrollThumb {
    cfg: ThumbState,
}

impl ScrollThumb {
    /// The scroller's colour: accent dragged, primary hovered, secondary at
    /// rest.
    fn role(&self) -> TextRole {
        if self.cfg.dragging.get() {
            TextRole::Accent
        } else if self.cfg.hovered.get() {
            TextRole::Primary
        } else {
            TextRole::Secondary
        }
    }
}

/// The painted part of a scroller occupying `bar` (the trailing half across the
/// bar), for a content scrolled to `scroll_ratio` with `viewport_ratio` of it
/// visible. `None` when the content fits.
pub fn thumb_rect(
    bar: Rect,
    vertical: bool,
    scroll_ratio: f32,
    viewport_ratio: f32,
    min_length: f32,
    rtl: bool,
) -> Option<Rect> {
    if viewport_ratio >= 1.0 {
        return None;
    }
    let ratio = scroll_ratio.clamp(0.0, 1.0);
    if vertical {
        let length = (bar.height * viewport_ratio)
            .max(min_length)
            .min(bar.height);
        let y = bar.y + ratio * (bar.height - length);
        let half = bar.width / 2.0;
        Some(Rect::new(bar.x + half, y, half, length))
    } else {
        let length = (bar.width * viewport_ratio).max(min_length).min(bar.width);
        let travel = ratio * (bar.width - length);
        let x = if rtl {
            bar.x + bar.width - length - travel
        } else {
            bar.x + travel
        };
        let half = bar.height / 2.0;
        Some(Rect::new(x, bar.y + half, length, half))
    }
}

impl Widget for ScrollThumb {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let id = ctx.self_id();
        let registry = ctx.binding_registry();
        let level = BindingLevel::RepaintOnly;
        self.cfg.scroll_ratio.bind_to(id, registry, level);
        self.cfg.viewport_ratio.bind_to(id, registry, level);
        self.cfg.hovered.bind_to(id, registry, level);
        self.cfg.dragging.bind_to(id, registry, level);
        self.cfg.idle.bind_to(id, registry, level);
        Vec::new()
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal
            .resolve(SCROLL_BAR_THICKNESS, SCROLL_BAR_THICKNESS)
            .into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        if self.cfg.idle.get() {
            return;
        }
        let rtl = ctx.layout_direction == LayoutDirection::RightToLeft;
        if let Some(thumb) = thumb_rect(
            bounds,
            self.cfg.vertical,
            self.cfg.scroll_ratio.get(),
            self.cfg.viewport_ratio.get(),
            self.cfg.min_length,
            rtl,
        ) {
            let color = ColorProp::from(self.role()).resolve(ctx.theme, ctx.effective_enabled);
            canvas.fill_rect(thumb, color);
        }
    }
}

/// An Iced `standard_scrollable` (`widgets.rs:1648`): overlay bars 8 px
/// thick, 60 px per wheel line and no smoothing. The bars take their look
/// from the theme's scroll-bar slot ([`scroll_bar_style`]).
pub fn scroll_area() -> ScrollArea {
    ScrollArea::new()
        .scroll_bar_thickness(SCROLL_BAR_THICKNESS)
        .line_height(SCROLL_LINE_HEIGHT)
        .smooth_scrolling(false)
}

// ---------------------------------------------------------------------------

/// `rect` shrunk by `by` on every side (a stroke of width `2 × by` along the
/// result stays inside `rect`, as an Iced border does).
fn inset(rect: Rect, by: f32) -> Rect {
    Rect::new(
        rect.x + by,
        rect.y + by,
        (rect.width - 2.0 * by).max(0.0),
        (rect.height - 2.0 * by).max(0.0),
    )
}

/// Installs these looks theme-wide: the checkbox, radio, combo-box and
/// scroll-bar styles, and the equalizer toggler as the default toggle (the
/// other toggler looks are per call). Called by `style::audoxidy_dark`.
pub fn install(slots: &mut ComponentStyleSlots) {
    slots.checkbox = Some(Rc::new(checkbox_style()));
    slots.radio = Some(Rc::new(radio_style()));
    slots.combo_box = Some(Rc::new(combo_box_style()));
    slots.scroll_bar = Some(Rc::new(scroll_bar_style()));
    slots.toggle = Some(Rc::new(toggle_style(ToggleLook::equalizer())));
}

#[cfg(test)]
mod tests;
