//! The Audoxidy dialog: its frame, its buttons and its text.
//!
//! Port of `standard_modal` and `modal_text` (`src/gui/widgets.rs:2075-2199`).
//! The Iced dialog is a column, at least 200 px wide, of:
//!
//! - the title, Inter Medium 16 px, primary, centred;
//! - the content, with 10 px above and below;
//! - a footer, 5 px below, of centred buttons 10 px apart: "Cancelar" when
//!   the dialog can be cancelled, then the confirm button with the caller's
//!   label. A button is Inter Medium 14 px with 5 × 10 px of padding and a
//!   6 px radius, `COLOR_CONTRAST` with secondary text at rest, accent with
//!   primary text while hovered;
//!
//! in a panel with 15 px of padding, filled with `COLOR_BG`, with a 2 px
//! accent border, an 8 px radius and a soft black shadow 10 px below. There is
//! no dimming behind it, and a click outside closes it.
//!
//! Following the port's rules, dialogs opened from menus use the stock
//! `InputDialog` and `MessageBox`, or `ctx.present_modal` for anything else.
//! This module gives all three the Audoxidy look:
//!
//! - [`DialogFrameStyle`] is the panel and the (transparent) scrim, as a
//!   `DialogStyle`: install it theme-wide so `InputDialog` and `MessageBox` get
//!   it, and [`DialogFrame::present`] passes it explicitly;
//! - [`DialogButtonStyle`] is the button look as a `ButtonStyle`, for the
//!   stock dialogs' buttons (theme-wide), and [`text_button`] the same look
//!   on `Pressable` with a label that follows the hover;
//! - [`DialogInputStyle`] is the frame of the dialogs' text fields
//!   (`app.rs:5251`: `COLOR_CONTRAST`, a 1 px accent border, a 4 px radius,
//!   4 × 5 px of padding), as a `TextInputStyle`;
//! - [`DialogFrame`] is the Iced column itself, for `present_modal` content
//!   that needs the exact Iced layout (the export dialog);
//! - [`ModalText`] is `modal_text`: Inter 14 px, primary, every line centred;
//! - [`input_dialog`] presets the stock `InputDialog` with the Iced
//!   "Cancelar" button, and [`confirm_box`] is the Iced confirmation, a
//!   [`DialogFrame`] in which Enter does nothing ([`ConfirmBox`]).
//!
//! [`Frame`] is the painted panel the dialog and the menus share.
//!
//! What the stock dialogs lay out themselves is theirs: a `MessageBox` puts its
//! title at the leading edge and its buttons at the trailing edge, and its
//! buttons' labels keep one colour (a `ButtonStyle` cannot recolour a label it
//! is handed on hover).

use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::accesskit::Role;
use teksilo::core::modal::{ModalCloseBehavior, ModalRequest};
use teksilo::core::styles::{
    ButtonStyle, ButtonStyleConfig, ButtonVariant, DialogStyle, DialogStyleConfig, TextInputStyle,
    TextInputStyleConfig,
};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, Shadow, VAlignment};
use teksilo::widgets::{
    FixedSize, HStack, InputDialog, ModalContainer, Padding, Panel, RectWidget, Spacer,
    StandardButton, TextWidget, VStack, ZStack,
};

use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;
use crate::teksilo_ui::widgets::pressable::ActivateFn;

/// The panel's padding (`widgets.rs:2177`).
pub const PANEL_PADDING: f32 = 15.0;
/// The panel's corner radius (`widgets.rs:2184`).
pub const PANEL_RADIUS: f32 = 8.0;
/// The panel's accent border width (`widgets.rs:2183`).
pub const PANEL_BORDER: f32 = 2.0;
/// The content's minimum width: the 200 px space at the top of the column
/// (`widgets.rs:2160`).
pub const CONTENT_MIN_WIDTH: f32 = 200.0;
/// The space above and below the content (`widgets.rs:2170`).
pub const CONTENT_PADDING: f32 = 10.0;
/// The title size (`widgets.rs:2164`).
pub const TITLE_SIZE: f32 = 16.0;
/// The size of the buttons' labels and of [`ModalText`] (`widgets.rs:2091`,
/// `widgets.rs:2194`).
pub const TEXT_SIZE: f32 = 14.0;
/// The space between footer buttons (`widgets.rs:2083`).
pub const FOOTER_SPACING: f32 = 10.0;
/// The space above the footer (`widgets.rs:2085`).
pub const FOOTER_TOP: f32 = 5.0;
/// A button's padding: 5 px above and below, 10 px on the sides
/// (`widgets.rs:2092`).
pub const BUTTON_PADDING: (f32, f32) = (5.0, 10.0);
/// A button's corner radius (`widgets.rs:2110`).
pub const BUTTON_RADIUS: f32 = 6.0;
/// A dialog field's padding: 4 px above and below, 5 px on the sides
/// (`app.rs:5251`).
pub const INPUT_PADDING: (f32, f32) = (4.0, 5.0);
/// A dialog field's corner radius (`app.rs:5256`).
pub const INPUT_RADIUS: f32 = 4.0;

/// The panel's shadow: 10 px down, 30 px blur, black at 80 %
/// (`widgets.rs:2187`). A literal colour: a drop shadow is black whatever the
/// theme, and no colour role describes it.
pub fn panel_shadow() -> Shadow {
    Shadow {
        offset_x: 0.0,
        offset_y: 10.0,
        blur: 30.0,
        spread: 0.0,
        color: Color::new(0.0, 0.0, 0.0, 0.8),
    }
}

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

/// A painted panel around a child: an optional shadow, a fill, a border and
/// padding. The Audoxidy dialogs and menus are drawn with it.
pub struct Frame {
    content: Option<FrameContent>,
    fill: ColorProp,
    border: ColorProp,
    border_width: f32,
    radius: f32,
    shadow: Option<Shadow>,
    padding: (f32, f32, f32, f32),
    min_width: f32,
    root: Option<WidgetId>,
}

/// The child of a [`Frame`]: a widget to add, or one already added (a style
/// is handed its content as an id).
enum FrameContent {
    Widget(Box<dyn Widget>),
    Id(WidgetId),
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("border_width", &self.border_width)
            .field("radius", &self.radius)
            .field("padding", &self.padding)
            .finish_non_exhaustive()
    }
}

impl Frame {
    /// A frame around `content`: no fill, no border, no padding yet.
    pub fn new(content: impl Widget + 'static) -> Self {
        Self::with(FrameContent::Widget(Box::new(content)))
    }

    /// A frame around a widget already added to the tree.
    pub fn around(content: WidgetId) -> Self {
        Self::with(FrameContent::Id(content))
    }

    fn with(content: FrameContent) -> Self {
        Self {
            content: Some(content),
            fill: SurfaceRole::Transparent.into(),
            border: SurfaceRole::Transparent.into(),
            border_width: 0.0,
            radius: 0.0,
            shadow: None,
            padding: (0.0, 0.0, 0.0, 0.0),
            min_width: 0.0,
            root: None,
        }
    }

    /// The fill.
    pub fn fill(mut self, fill: impl Into<ColorProp>) -> Self {
        self.fill = fill.into();
        self
    }

    /// A border of `width` px in `color`, drawn inside the bounds.
    pub fn border(mut self, color: impl Into<ColorProp>, width: f32) -> Self {
        self.border = color.into();
        self.border_width = width;
        self
    }

    /// The corner radius.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// A drop shadow under the panel.
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// The space between the edges and the content.
    pub fn padding(mut self, top: f32, trailing: f32, bottom: f32, leading: f32) -> Self {
        self.padding = (top, trailing, bottom, leading);
        self
    }

    /// The narrowest the frame gets, padding included.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }
}

impl Widget for Frame {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let content = match self.content.take() {
            Some(FrameContent::Widget(widget)) => ctx.add_boxed(widget),
            Some(FrameContent::Id(id)) => id,
            None => return self.root.into_iter().collect(),
        };
        let (top, trailing, bottom, leading) = self.padding;
        let root = ctx.add(Padding::new(top, trailing, bottom, leading).child(content));
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let size = self
            .root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0));
        Size::new(size.width.max(self.min_width), size.height).into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let radius = CornerRadius::uniform(self.radius);
        if let Some(shadow) = &self.shadow {
            canvas.draw_shadow(bounds, radius, shadow);
        }
        let enabled = ctx.effective_enabled;
        canvas.fill_rounded_rect(bounds, radius, self.fill.resolve(ctx.theme, enabled));
        if self.border_width > 0.0 {
            let half = self.border_width / 2.0;
            let inner = Rect::new(
                bounds.x + half,
                bounds.y + half,
                (bounds.width - self.border_width).max(0.0),
                (bounds.height - self.border_width).max(0.0),
            );
            canvas.stroke_rounded_rect(
                inner,
                CornerRadius::uniform((self.radius - half).max(0.0)),
                self.border.resolve(ctx.theme, enabled),
                self.border_width,
            );
        }
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::GenericContainer);
    }
}

// ---------------------------------------------------------------------------
// Styles for the stock dialogs
// ---------------------------------------------------------------------------

/// The Iced dialog panel and scrim as a `DialogStyle`; see the module
/// documentation.
#[derive(Debug, Clone, Copy, Default)]
pub struct DialogFrameStyle;

impl DialogStyle for DialogFrameStyle {
    fn make_panel(&self, cfg: &DialogStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let padding = cfg.padding_override.unwrap_or(PANEL_PADDING);
        let min_width = cfg
            .min_width_override
            .unwrap_or(CONTENT_MIN_WIDTH + 2.0 * PANEL_PADDING);
        ctx.add(
            Frame::around(cfg.content)
                .fill(SurfaceRole::Main)
                .border(BorderRole::Accent, PANEL_BORDER)
                .radius(PANEL_RADIUS)
                .shadow(panel_shadow())
                .padding(padding, padding, padding, padding)
                .min_width(min_width),
        )
    }

    fn make_scrim(&self, ctx: &mut BuildContext) -> WidgetId {
        // The Iced dialog has an invisible click blocker, no dimming.
        ctx.add(RectWidget::new().background(SurfaceRole::Transparent))
    }
}

/// The Iced dialog button as a `ButtonStyle` (`widgets.rs:2093-2117`), for
/// the buttons of the stock dialogs: `COLOR_CONTRAST`, accent while hovered,
/// a 6 px radius, 5 × 10 px of padding. Every variant looks the same, as in
/// Iced. [`DialogButtonStyle::padded`] gives the larger buttons of the Audio
/// Center (8 × 12 px, `audio_center.rs:2624`) the same look.
#[derive(Debug, Clone, Copy)]
pub struct DialogButtonStyle {
    padding: (f32, f32),
}

impl Default for DialogButtonStyle {
    fn default() -> Self {
        Self {
            padding: BUTTON_PADDING,
        }
    }
}

impl DialogButtonStyle {
    /// The look with `vertical` × `horizontal` px of padding.
    pub fn padded(vertical: f32, horizontal: f32) -> Self {
        Self {
            padding: (vertical, horizontal),
        }
    }
}

impl ButtonStyle for DialogButtonStyle {
    fn make_body(&self, cfg: &ButtonStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let fill = cfg.is_hovered.map(|hovered| button_fill(*hovered));
        ctx.add(
            ZStack::new()
                .child(
                    RectWidget::new()
                        .background(fill)
                        .corner_radius(CornerRadius::uniform(BUTTON_RADIUS)),
                )
                .child(Padding::symmetric(self.padding.0, self.padding.1).child(cfg.label)),
        )
    }

    fn label_text_role(&self, _variant: ButtonVariant) -> Option<TextRole> {
        // The resting Iced colour; the label cannot follow the hover.
        Some(TextRole::Secondary)
    }
}

/// A dialog button's fill: accent while hovered, `COLOR_CONTRAST` otherwise.
pub fn button_fill(hovered: bool) -> SurfaceRole {
    if hovered {
        SurfaceRole::Accent
    } else {
        SurfaceRole::Raised
    }
}

/// A dialog button's label colour: primary while hovered, secondary otherwise.
pub fn button_text(hovered: bool) -> TextRole {
    if hovered {
        TextRole::Primary
    } else {
        TextRole::Secondary
    }
}

/// The frame of the Iced dialog text fields (`app.rs:5251-5262`).
#[derive(Debug, Clone, Copy, Default)]
pub struct DialogInputStyle;

impl TextInputStyle for DialogInputStyle {
    fn make_body(&self, cfg: &TextInputStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        ctx.add(
            Panel::new()
                .padding(0.0)
                .background(SurfaceRole::Raised)
                .border_color(BorderRole::Accent)
                .border_width(1.0)
                .corner_radius(INPUT_RADIUS)
                .a11y_presentational()
                .child(Padding::symmetric(INPUT_PADDING.0, INPUT_PADDING.1).child(cfg.editor)),
        )
    }
}

/// Installs the dialog looks theme-wide: the panel and scrim, the buttons and
/// the text fields (the stock dialogs take theirs from the theme). Called by
/// `style::audoxidy_dark`.
pub fn install(slots: &mut teksilo::core::styles::ComponentStyleSlots) {
    slots.dialog = Some(Rc::new(DialogFrameStyle));
    slots.button = Some(Rc::new(DialogButtonStyle::default()));
    slots.text_input = Some(Rc::new(DialogInputStyle));
}

/// An `InputDialog` titled `title` asking `prompt`, with the Iced "Cancelar"
/// and `confirm` buttons (the create and rename dialogs, `app.rs:5243-5294`).
/// The caller adds the placeholder, the default text, `validate` and
/// `on_result`, then presents it.
pub fn input_dialog(
    title: impl Into<LocalizedString>,
    prompt: impl Into<LocalizedString>,
    confirm: impl Into<LocalizedString>,
) -> InputDialog {
    InputDialog::new(title)
        .prompt(prompt)
        .ok_label(confirm)
        .cancel_label(tr!(chrome_dialog_cancel()))
}

/// A confirmation titled `title` saying `text`, with the Iced "Cancelar" and
/// `confirm` buttons (the library's and the playlist's delete confirmations,
/// `app.rs:5296-5304`): the Iced `standard_modal` around `modal_text`, as a
/// [`DialogFrame`]. The caller adds `on_result` (`StandardButton::Ok` is the
/// confirmation) and presents it; see [`ConfirmBox`].
pub fn confirm_box(
    title: impl Into<LocalizedString>,
    text: impl Into<LocalizedString>,
    confirm: impl Into<LocalizedString>,
) -> ConfirmBox {
    ConfirmBox {
        title: title.into(),
        text: text.into(),
        confirm: confirm.into(),
        on_result: None,
    }
}

/// What closed a [`ConfirmBox`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfirmResult {
    /// `StandardButton::Ok` for the confirm button; `StandardButton::Cancel`
    /// for "Cancelar", Escape and a click outside.
    pub button: StandardButton,
    dismissed: bool,
}

impl ConfirmResult {
    /// Whether the box was closed without a button (Escape, a click
    /// outside).
    pub fn was_dismissed(&self) -> bool {
        self.dismissed
    }
}

/// The function a [`ConfirmBox`] reports its answer to.
type ResultFn = Rc<dyn Fn(ConfirmResult, &mut EventContext)>;

/// A confirmation in the Iced dialog ([`confirm_box`]). Its buttons answer
/// through [`on_result`](Self::on_result); Escape and a click outside close
/// it as "Cancelar" (quirk fix 4). **Enter does nothing** in it, as in the
/// Iced dialog, whose buttons take no key (`app.rs:5296-5304`), whatever has
/// the keyboard: a stock `MessageBox` presses its default button on Enter,
/// and the button the keyboard is on when there is none, so a confirmation
/// that deletes would take Enter for an answer. Space still presses the
/// button the keyboard is on.
pub struct ConfirmBox {
    title: LocalizedString,
    text: LocalizedString,
    confirm: LocalizedString,
    on_result: Option<ResultFn>,
}

impl std::fmt::Debug for ConfirmBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmBox")
            .field("title", &self.title.resolve_now())
            .finish_non_exhaustive()
    }
}

impl ConfirmBox {
    /// Runs `f` with the answer, once, whatever closes the box.
    pub fn on_result(mut self, f: impl Fn(ConfirmResult, &mut EventContext) + 'static) -> Self {
        self.on_result = Some(Rc::new(f));
        self
    }

    /// Presents the box as a modal ([`DialogFrame::present`]).
    pub fn present(self, ctx: &mut EventContext) {
        let report: ResultFn = self.on_result.unwrap_or_else(|| Rc::new(|_, _| {}));
        let answer = |button: StandardButton, dismissed: bool| {
            let report = report.clone();
            move |ctx: &mut EventContext| report(ConfirmResult { button, dismissed }, ctx)
        };
        DialogFrame::new(self.title, ModalText::new(self.text))
            .on_cancel(answer(StandardButton::Cancel, false))
            .on_confirm(self.confirm, answer(StandardButton::Ok, false))
            .on_dismissed(answer(StandardButton::Cancel, true))
            .ignore_enter()
            .present(ctx);
    }
}

// ---------------------------------------------------------------------------
// The Iced column
// ---------------------------------------------------------------------------

/// An Iced `standard_modal`: title, content and footer, for
/// `ctx.present_modal`; see the module documentation.
pub struct DialogFrame {
    title: LocalizedString,
    content: Option<Box<dyn Widget>>,
    on_cancel: Option<ActivateFn>,
    /// What Escape and a click outside run instead of `on_cancel`.
    on_dismissed: Option<ActivateFn>,
    confirm: Option<(LocalizedString, ActivateFn)>,
    confirm_enabled: Prop<bool>,
    ignore_enter: bool,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for DialogFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogFrame")
            .field("title", &self.title.resolve_now())
            .field("cancellable", &self.on_cancel.is_some())
            .finish_non_exhaustive()
    }
}

impl DialogFrame {
    /// A dialog titled `title` around `content`, with no buttons yet.
    pub fn new(title: impl Into<LocalizedString>, content: impl Widget + 'static) -> Self {
        Self {
            title: title.into(),
            content: Some(Box::new(content)),
            on_cancel: None,
            on_dismissed: None,
            confirm: None,
            confirm_enabled: Prop::Static(true),
            ignore_enter: false,
            root: None,
        }
    }

    /// Adds the "Cancelar" button; `f` also runs when the dialog is closed by
    /// Escape or a click outside.
    pub fn on_cancel(mut self, f: impl Fn(&mut EventContext) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// Adds the confirm button, labelled `label`.
    pub fn on_confirm(
        mut self,
        label: impl Into<LocalizedString>,
        f: impl Fn(&mut EventContext) + 'static,
    ) -> Self {
        self.confirm = Some((label.into(), Rc::new(f)));
        self
    }

    /// Runs `f`, instead of the cancel action, when Escape or a click
    /// outside closes the dialog.
    fn on_dismissed(mut self, f: impl Fn(&mut EventContext) + 'static) -> Self {
        self.on_dismissed = Some(Rc::new(f));
        self
    }

    /// Makes Enter do nothing in the dialog, even on a focused button (the
    /// Iced dialog's buttons take no key).
    pub fn ignore_enter(mut self) -> Self {
        self.ignore_enter = true;
        self
    }

    /// Whether the confirm button can be pressed (default `true`).
    pub fn confirm_enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.confirm_enabled = enabled.into();
        self
    }

    /// Presents the dialog as a modal, centred, in the Audoxidy frame. A
    /// button closes it after running its action; Escape and a click outside
    /// close it and run the cancel action.
    pub fn present(mut self, ctx: &mut EventContext) {
        let settled = Rc::new(Cell::new(false));
        let on_cancel = self.on_dismissed.take().or_else(|| self.on_cancel.clone());
        self.on_cancel = self.on_cancel.take().map(|f| settling(&settled, f));
        self.confirm = self
            .confirm
            .take()
            .map(|(label, f)| (label, settling(&settled, f)));
        let title = self.title.clone();
        let window_title = title.resolve_now();
        ctx.present_modal(
            ModalRequest::deferred(move |tree| {
                tree.add(
                    ModalContainer::new(self)
                        .style(DialogFrameStyle)
                        .title(title),
                )
            })
            .title(window_title)
            .close_behavior(ModalCloseBehavior::EscapeOrClickOutside)
            .on_dismiss(Rc::new(move |_reason, ctx| {
                if !settled.replace(true)
                    && let Some(cancel) = &on_cancel
                {
                    cancel(ctx);
                }
            })),
        );
    }
}

/// `f`, marking the dialog settled first so its dismissal runs no cancel.
fn settling(settled: &Rc<Cell<bool>>, f: ActivateFn) -> ActivateFn {
    let settled = settled.clone();
    Rc::new(move |ctx: &mut EventContext| {
        settled.set(true);
        ctx.dismiss_modal();
        f(ctx);
    })
}

impl Widget for DialogFrame {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let mut footer = HStack::new()
            .spacing(FOOTER_SPACING)
            .alignment(VAlignment::Center);
        if let Some(cancel) = self.on_cancel.clone() {
            footer = footer.child(dialog_button(tr!(chrome_dialog_cancel()), cancel, true));
        }
        if let Some((label, confirm)) = self.confirm.clone() {
            footer = footer.child(dialog_button(label, confirm, self.confirm_enabled.clone()));
        }
        let mut column = VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Center)
            .child(
                FixedSize::new()
                    .width(CONTENT_MIN_WIDTH)
                    .height(0.0)
                    .child(Spacer::new()),
            )
            .child(
                TextWidget::new(self.title.clone())
                    .style(style::inter(TITLE_SIZE, FontWeight::MEDIUM))
                    .color(TextRole::Primary)
                    .single_line(),
            );
        if let Some(content) = self.content.take() {
            let content = ctx.add_boxed(content);
            column = column.child(Padding::symmetric(CONTENT_PADDING, 0.0).child(content));
        }
        let column = column.child(Padding::new(FOOTER_TOP, 0.0, 0.0, 0.0).child(footer));
        let root = if self.ignore_enter {
            // Before the focused button sees it.
            ctx.add(column.on_key_preview(|event, _ctx| match event {
                WidgetEvent::KeyDown {
                    key: teksilo::core::Key::Enter,
                    ..
                } => EventResponse::Handled,
                _ => EventResponse::Ignored,
            }))
        } else {
            ctx.add(column)
        };
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, _proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        // The Iced column shrinks to its content whatever room it is offered.
        self.root
            .and_then(|id| ctx.child_size(id, SizeProposal::unspecified()))
            .unwrap_or(Size::ZERO)
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// One footer button (`widgets.rs:2090-2152`).
fn dialog_button(
    label: LocalizedString,
    action: ActivateFn,
    enabled: impl Into<Prop<bool>>,
) -> Pressable {
    text_button(label, TEXT_SIZE, BUTTON_PADDING)
        .enabled(enabled)
        .on_activate(move |ctx| action(ctx))
}

/// A button in the Iced dialog-button look, label included: Inter Medium
/// `text_size` px, `padding` (vertical, horizontal) around it, a 6 px radius;
/// `COLOR_CONTRAST` with a secondary label at rest, accent with a primary
/// label while hovered (`widgets.rs:2090-2117`; the Audio Center's bottom
/// buttons use 15 px and 8 × 12 px, `audio_center.rs:2619`). Chain
/// `on_activate` and `enabled` onto it.
pub fn text_button(label: LocalizedString, text_size: f32, padding: (f32, f32)) -> Pressable {
    let hovered = Signal::new(false);
    let body = ZStack::new()
        .child(
            RectWidget::new()
                .background(hovered.map(|hovered| button_fill(*hovered)))
                .corner_radius(CornerRadius::uniform(BUTTON_RADIUS)),
        )
        .child(
            Padding::symmetric(padding.0, padding.1).child(
                TextWidget::new(label.clone())
                    .style(style::inter(text_size, FontWeight::MEDIUM))
                    .color(hovered.map(|hovered| button_text(*hovered)))
                    .single_line()
                    .a11y_hidden(),
            ),
        );
    Pressable::new(body, label)
        .hovered(hovered)
        .corner_radius(BUTTON_RADIUS)
}

// ---------------------------------------------------------------------------
// modal_text
// ---------------------------------------------------------------------------

/// The Iced `modal_text` (`widgets.rs:2190`): Inter 14 px, primary, every
/// line centred. One accessibility node reads the whole text.
pub struct ModalText {
    text: LocalizedString,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ModalText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModalText")
            .field("text", &self.text.resolve_now())
            .finish_non_exhaustive()
    }
}

impl ModalText {
    /// `text`, whose `\n`s break lines.
    pub fn new(text: impl Into<LocalizedString>) -> Self {
        Self {
            text: text.into(),
            root: None,
        }
    }
}

impl Widget for ModalText {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        // A locale change re-splits the lines.
        ctx.locale_signal().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            teksilo::core::binding::BindingLevel::Rebuild,
        );
        let text = self.text.resolve_now();
        let lines = text.split('\n').map(|line| {
            TextWidget::new(lit!(line.to_string()))
                .style(style::inter(TEXT_SIZE, FontWeight::REGULAR))
                .color(TextRole::Primary)
        });
        let root = ctx.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Center)
                .children(lines)
                .access_role(Role::Label)
                .access_label(self.text.clone())
                .access_exclude_subtree(),
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
