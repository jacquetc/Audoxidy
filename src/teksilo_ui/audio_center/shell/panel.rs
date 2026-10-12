//! [`AudioCenterPanel`]: the Audio Center's floating panel.
//!
//! Port of `view` (`src/gui/audio_center.rs:1812-2029`) and of where the app
//! puts it (`src/gui/app.rs:5160-5190`). From the outside in:
//!
//! - the **layer**: a transparent widget filling the window that places the
//!   panel, centred while it was never dragged and at its dragged position
//!   otherwise, as Iced's `container(...).padding(top: y, left: x)`. It is
//!   invisible to the pointer, so the main layout below stays usable (the Audio
//!   Center is not modal), and it tells the view-model the window's size, which
//!   the drag clamps to. The leading offset counts from the trailing edge in a
//!   right-to-left layout. Mount the panel as a child of the `ZStack` over the
//!   main layout;
//! - the **frame**: 940 × 480, `COLOR_BG`, a 2 px accent border with an 8 px
//!   radius, and 2 px of inset for what is inside (`:1996-2018`). It takes the
//!   keyboard focus when it is pressed (so an open value field of a tab
//!   commits, as it does on any outside click in Iced, which deselects
//!   whatever was active: quirk 25), and Escape closes the panel when the
//!   focus is inside it (quirk fix 4);
//! - the **header**, 40 px: the two-tone logo 15 px from the leading edge and
//!   9 px lower than the centre line, the title centred (Inter Medium 16 px),
//!   and the 24 px close icon 6 px from the trailing edge, accent while
//!   hovered. The header is the drag handle (`:1946-1976`);
//! - the **tab strip**: four tabs 10 px apart, each Inter Medium 14 px with
//!   5 × 15 px of padding and a 2 px radius at the top, accent-filled with
//!   primary text when selected, otherwise secondary text that turns primary
//!   when hovered (`:1978-2018`), and a 2 px `COLOR_CONTRAST` line under them;
//! - the **body**: the selected tab's widget, with 15 px on both sides and
//!   below.
//!
//! The panel is built once and shown or hidden with the view-model's `open`
//! signal, so the tab bodies keep their state while it is closed. When it
//! opens, the close button takes the focus, so that Escape works at once.

use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::core::environment::LayoutDirection;
use teksilo::core::widget::WidgetPlacement;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, Padding, RectWidget, Spacer, Switcher, TextWidget, VStack,
    ZStack,
};

use super::audio_center_vm::{AudioCenterViewModel, HEIGHT, WIDTH};
use crate::teksilo_ui::app::services::audio_center_tab;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::widgets::{BrandLogo, Pressable};

/// The frame's border width and its inset for the content (`:2007`, `:2013`).
pub const BORDER: f32 = 2.0;
/// The frame's corner radius (`:2014`).
pub const RADIUS: f32 = 8.0;
/// The header's height (`:1904`).
pub const HEADER_HEIGHT: f32 = 40.0;
/// The header's padding at the leading edge (`:1893`) ...
pub const HEADER_LEADING: f32 = 15.0;
/// ... and at the trailing edge (`:1891`).
pub const HEADER_TRAILING: f32 = 6.0;
/// The logo's size (`:1822`) and the space above it (`:1840`).
pub const LOGO_SIZE: f32 = 18.0;
/// The space above the logo inside the header (`:1842`).
pub const LOGO_TOP: f32 = 9.0;
/// The title's size (`:1895`).
pub const TITLE_SIZE: f32 = 16.0;
/// The close icon's size (`:1850`) and the padding around it (`:1869`).
pub const CLOSE_SIZE: f32 = 24.0;
/// The padding around the close icon.
pub const CLOSE_PADDING: f32 = 4.0;
/// The tab labels' size (`:1927`).
pub const TAB_SIZE: f32 = 14.0;
/// A tab's padding: 5 px above and below, 15 px on the sides (`:1927`).
pub const TAB_PADDING: (f32, f32) = (5.0, 15.0);
/// The space between tabs (`:1920`).
pub const TAB_SPACING: f32 = 10.0;
/// The corner radius at the top of a tab (`:1939`).
pub const TAB_RADIUS: f32 = 2.0;
/// The line under the tabs (`:1962`).
pub const DIVIDER_HEIGHT: f32 = 2.0;
/// The padding of the tab body at the sides and below (`:1976`).
pub const BODY_PADDING: f32 = 15.0;

/// The tab names in Iced's order, as accessible names and labels.
fn tab_label(tab: usize) -> LocalizedString {
    match tab {
        audio_center_tab::CONFIG => tr!(ac_shell_tab_config()),
        audio_center_tab::EQUALIZER => tr!(ac_shell_tab_equalizer()),
        audio_center_tab::EFFECTS => tr!(ac_shell_tab_effects()),
        _ => tr!(ac_shell_tab_volume_mix()),
    }
}

// ---------------------------------------------------------------------------
// The layer
// ---------------------------------------------------------------------------

/// Places the panel in the window; see the module documentation.
struct Layer {
    vm: AudioCenterViewModel,
    content: Option<Box<dyn Widget>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer").finish_non_exhaustive()
    }
}

impl Widget for Layer {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.position().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Relayout,
        );
        if let Some(content) = self.content.take() {
            self.root = Some(ctx.add_boxed(content));
        }
        self.root.into_iter().collect()
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(
            proposal.width.unwrap_or(WIDTH),
            proposal.height.unwrap_or(HEIGHT),
        )
        .into()
    }

    fn place_children(
        &self,
        bounds: Rect,
        _proposal: SizeProposal,
        children: &mut [WidgetPlacement],
        ctx: &LayoutContext,
    ) {
        self.vm.set_viewport(bounds.width, bounds.height);
        let (leading, top) = self.vm.origin();
        let x = if ctx.is_rtl() {
            bounds.x + bounds.width - leading - WIDTH
        } else {
            bounds.x + leading
        };
        for child in children {
            child.origin = Point::new(x, bounds.y + top);
            child.size = Size::new(WIDTH, HEIGHT);
        }
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// The header
// ---------------------------------------------------------------------------

/// Takes the focus for its child when the panel opens, so the keyboard works
/// at once (Escape closes the panel). It rebuilds when `open` changes; while
/// the panel is closed it holds nothing.
struct FocusOnOpen {
    open: Signal<bool>,
    child: Box<dyn Fn() -> Box<dyn Widget>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for FocusOnOpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FocusOnOpen").finish_non_exhaustive()
    }
}

impl Widget for FocusOnOpen {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.open
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let id = ctx.add_boxed((self.child)());
        self.root = Some(id);
        if self.open.get() {
            ctx.focus(id);
        }
        vec![id]
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

/// The close button: the 24 px icon in the primary colour, accent while
/// hovered (`:1844-1870`).
fn close_button(vm: &AudioCenterViewModel) -> Pressable {
    let hovered = Signal::new(false);
    let tint = hovered.map(|hovered| {
        if *hovered {
            TextRole::Accent
        } else {
            TextRole::Primary
        }
    });
    let vm = vm.clone();
    Pressable::new(
        Padding::uniform(CLOSE_PADDING).child(
            FixedSize::new().width(CLOSE_SIZE).height(CLOSE_SIZE).child(
                icons::audio_center::close_big()
                    .icon_size(CLOSE_SIZE)
                    .color(tint),
            ),
        ),
        tr!(ac_shell_close()),
    )
    .hovered(hovered)
    .on_activate(move |_| vm.close())
}

/// The drag handle: the header's pointer contract. A primary press captures
/// the pointer and starts the drag, moves while it is captured move the panel,
/// the release or a cancel ends it (`DragStart`, `app.rs:4482`). The pointer
/// is read in window coordinates (`EventContext::pointer_position`): an event's
/// own position is local to the header, which moves with the drag, so it would
/// lose the panel's own movement on every step. In a right-to-left layout `x`
/// is mirrored, since positions count from the leading edge.
fn header(vm: &AudioCenterViewModel) -> impl Widget + use<> {
    let logo = Padding::new(LOGO_TOP, 0.0, 0.0, 0.0)
        .child(BrandLogo::new(LOGO_SIZE).two_tone(TextRole::Accent));
    let row = HStack::new()
        .spacing(0.0)
        .alignment(VAlignment::Center)
        .child(Padding::new(0.0, 0.0, 0.0, HEADER_LEADING).child(logo))
        .child(Spacer::new())
        .child(
            Padding::new(0.0, HEADER_TRAILING, 0.0, 0.0).child(FocusOnOpen {
                open: vm.open(),
                child: {
                    let vm = vm.clone();
                    Box::new(move || Box::new(close_button(&vm)))
                },
                root: None,
            }),
        );
    let title = Center::new().child(
        TextWidget::new(tr!(ac_shell_title()))
            .style(crate::teksilo_ui::style::inter(
                TITLE_SIZE,
                FontWeight::MEDIUM,
            ))
            .color(TextRole::Primary)
            .single_line()
            .no_shrink()
            .a11y_hidden(),
    );
    let (down, moved, up, cancel) = (vm.clone(), vm.clone(), vm.clone(), vm.clone());
    let mirrored = |x: f32, ctx: &EventContext| {
        if ctx.layout_direction() == LayoutDirection::RightToLeft {
            -x
        } else {
            x
        }
    };
    // The drag handle is a layer under the title and the row, which let the
    // pointer through to it: the close button, being above it and not inside
    // it, keeps its own press.
    let handle = RectWidget::new()
        .on_pointer_event(move |event, ctx| match event {
            WidgetEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                let Some(at) = ctx.pointer_position() else {
                    return EventResponse::Ignored;
                };
                ctx.capture_pointer();
                down.begin_drag((mirrored(at.x, ctx), at.y));
                EventResponse::Handled
            }
            WidgetEvent::PointerMove { .. } if moved.is_dragging() && ctx.owns_pointer() => {
                if let Some(at) = ctx.pointer_position() {
                    moved.drag_to((mirrored(at.x, ctx), at.y));
                }
                EventResponse::Handled
            }
            WidgetEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } if up.is_dragging() => {
                up.end_drag();
                ctx.release_pointer();
                EventResponse::Handled
            }
            _ => EventResponse::Ignored,
        })
        .on_pointer_cancel(move |_, _, _| cancel.end_drag());
    Expand::horizontal().child(
        FixedSize::new().height(HEADER_HEIGHT).child(
            ZStack::new()
                .child(handle)
                .child(title.event_pass_through(true))
                .child(row.event_pass_through(true)),
        ),
    )
}

// ---------------------------------------------------------------------------
// The tabs
// ---------------------------------------------------------------------------

/// One tab of the strip.
fn tab_button(vm: &AudioCenterViewModel, tab: usize) -> Pressable {
    let hovered = Signal::new(false);
    let selected = vm.tab().map(move |selected| *selected == tab);
    let fill = selected.map(|selected| {
        if *selected {
            SurfaceRole::Accent
        } else {
            SurfaceRole::Transparent
        }
    });
    let tint = selected.zip(&hovered).map(|(selected, hovered)| {
        if *selected || *hovered {
            TextRole::Primary
        } else {
            TextRole::Secondary
        }
    });
    let label = tab_label(tab);
    let vm = vm.clone();
    Pressable::new(
        ZStack::new()
            .child(
                RectWidget::new()
                    .background(fill)
                    .corner_radius(CornerRadius {
                        top_left: TAB_RADIUS,
                        top_right: TAB_RADIUS,
                        bottom_left: 0.0,
                        bottom_right: 0.0,
                    }),
            )
            .child(
                Padding::symmetric(TAB_PADDING.0, TAB_PADDING.1).child(
                    TextWidget::new(label.clone())
                        .style(crate::teksilo_ui::style::inter(
                            TAB_SIZE,
                            FontWeight::MEDIUM,
                        ))
                        .color(tint)
                        .single_line()
                        .no_shrink()
                        .a11y_hidden(),
                ),
            ),
        label,
    )
    .tab(selected)
    .hovered(hovered)
    .on_activate(move |_| vm.select_tab(tab))
}

/// The tab strip.
fn tab_strip(vm: &AudioCenterViewModel) -> impl Widget + use<> {
    HStack::new()
        .spacing(TAB_SPACING)
        .alignment(VAlignment::Center)
        .children(
            (audio_center_tab::CONFIG..=audio_center_tab::VOLUME_MIX)
                .map(|tab| tab_button(vm, tab)),
        )
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

/// The Audio Center panel; see the module documentation.
pub struct AudioCenterPanel {
    vm: AudioCenterViewModel,
    bodies: Option<[Box<dyn Widget>; 4]>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for AudioCenterPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCenterPanel")
            .field("vm", &self.vm)
            .finish_non_exhaustive()
    }
}

impl AudioCenterPanel {
    /// The panel over `vm`, showing `config`, `equalizer`, `effects` or
    /// `volume_mix` as the selected tab. Each body is built when its tab is
    /// first shown and keeps its state after that. Mount the panel as a layer
    /// over the main layout; it is hidden while the view-model is closed.
    pub fn new(
        vm: AudioCenterViewModel,
        config: Box<dyn Widget>,
        equalizer: Box<dyn Widget>,
        effects: Box<dyn Widget>,
        volume_mix: Box<dyn Widget>,
    ) -> Self {
        Self {
            vm,
            bodies: Some([config, equalizer, effects, volume_mix]),
            root: None,
        }
    }
}

impl Widget for AudioCenterPanel {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        // The panel's own node fills the window as its layer does: neither
        // takes the pointer, only the frame inside does.
        ctx.apply_self_handlers(HandlerSet::new().event_pass_through(true));
        let Some(bodies) = self.bodies.take() else {
            return self.root.into_iter().collect();
        };
        let vm = &self.vm;
        let escape = vm.clone();
        let [config, equalizer, effects, volume_mix] = bodies;
        let divider = Expand::horizontal().child(
            FixedSize::new()
                .height(DIVIDER_HEIGHT)
                .child(RectWidget::new().background(SurfaceRole::Raised)),
        );
        let body = Padding::new(0.0, BODY_PADDING, BODY_PADDING, BODY_PADDING).child(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(tab_strip(vm))
                .child(divider)
                .child(
                    Expand::new().child(
                        Switcher::new(vm.tab())
                            .child(config)
                            .child(equalizer)
                            .child(effects)
                            .child(volume_mix),
                    ),
                ),
        );
        let frame = ZStack::new()
            .child(
                RectWidget::new()
                    .background(SurfaceRole::Main)
                    .border_color(BorderRole::Accent)
                    .border_width(BORDER)
                    .corner_radius(CornerRadius::uniform(RADIUS)),
            )
            .child(
                Padding::uniform(BORDER).child(
                    VStack::new()
                        .spacing(0.0)
                        .alignment(HAlignment::Leading)
                        .child(header(vm))
                        .child(Expand::new().child(body)),
                ),
            )
            .focusable(true)
            .access_role(Role::Dialog)
            .access_label(tr!(ac_shell_title()))
            .on_key(move |event, _| match event {
                WidgetEvent::KeyDown {
                    key: Key::Escape, ..
                } => {
                    escape.close();
                    EventResponse::Handled
                }
                _ => EventResponse::Ignored,
            });
        let layer = Layer {
            vm: vm.clone(),
            content: Some(Box::new(
                FixedSize::new()
                    .width(WIDTH)
                    .height(HEIGHT)
                    .child(frame)
                    .visible_when(vm.open()),
            )),
            root: None,
        };
        let root = ctx.add(layer.event_pass_through(true));
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
