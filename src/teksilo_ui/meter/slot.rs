//! [`MeterSlot`]: the meter in the library's bottom bar, and the popup it opens.
//!
//! The library's bottom bar gives the meter the free space between the figures
//! and the actions: this widget's natural width is 400 px when the meter is shown
//! (Iced's size) and 0 when it is hidden, and it fills the width it is given,
//! drawing the meter in the leading 400 px while the whole slot takes the
//! right-click (`LibraryPanel::meter_slot`; Iced `widgets.rs:4772`). 40 px high:
//!
//! - **enabled**: the [`VuMeter`] in the leading 400 px, centred vertically
//!   (Iced `library.rs:1943`; the tall meter is 48 px, the bar 40 px);
//! - **disabled**: nothing is drawn and the natural width is 0, but the slot
//!   still takes the width it is given and stays a target (quirk fix 10: Iced drops the meter, and with it the only way to
//!   enable it).
//!
//! A right-click anywhere on the slot opens the [`MeterPopup`] (Iced:
//! `MeterRightClicked`, `library.rs:1910`), **anchored to the slot**, just above it
//! (quirk fix 7: Iced passes `Point::ORIGIN`, so the popup always opened at the
//! window's top-left). A left click does nothing, as in Iced. Not in Iced:
//! the slot is also a keyboard stop, and Enter or Space on it opens the popup;
//! it is named "Ajustes del medidor" for assistive technology.
//!
//! The popup is the content of one overlay that covers the whole window, as
//! Iced's popup sits over a full-window blocker (`app.rs:5850-5880`): a press
//! anywhere outside the panel, primary or secondary, lands on the blocker,
//! which closes the popup and keeps the press (`MeterPopupClose`): nothing
//! under it is clicked, and a right press on the slot closes the popup rather
//! than opening it again. Escape closes it too (quirk fix 4). A window-wide
//! overlay is also one the framework does not close when the keyboard moves
//! inside it in a way it reads as leaving (the steppers' value fields, which
//! are focused from their rebuild, closed a popover anchored to the slot).
//! The panel sits just above the slot, its leading edge on the slot's, kept
//! 5 px inside the window as Iced keeps it.

use std::rc::Rc;

use teksilo::core::accesskit::HasPopup;
use teksilo::core::binding::BindingLevel;
use teksilo::core::overlay::{DismissBehavior, OverlayPlacement, OverlayRequest};
use teksilo::core::widget::WidgetPlacement;
use teksilo::core::{PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::VAlignment;
use teksilo::widgets::{FixedSize, HStack, RectWidget, Spacer, ZStack};

use super::meter_vm::MeterViewModel;
use super::popup::MeterPopup;
use crate::teksilo_ui::widgets::{Pressable, VuMeter};

/// The slot's width (`widgets.rs:4772`).
pub const SLOT_WIDTH: f32 = 400.0;
/// The slot's height: the bottom bar's (`library.rs:2017`).
pub const SLOT_HEIGHT: f32 = 40.0;
/// How far inside the window the panel is kept (`app.rs:5866-5870`).
pub const WINDOW_MARGIN: f32 = 5.0;

/// Where the panel of size `panel` goes in a window `window`, for a slot at
/// `slot`: just above the slot, its leading edge on the slot's, at least
/// [`WINDOW_MARGIN`] inside the window.
pub fn panel_origin(slot: Rect, panel: Size, window: Rect) -> Point {
    let max_x = window.x + window.width - panel.width - WINDOW_MARGIN;
    let x = slot.x.min(max_x).max(window.x + WINDOW_MARGIN);
    let y = (slot.y - panel.height).max(window.y + WINDOW_MARGIN);
    Point::new(x, y)
}

/// The popup's overlay: the whole window, the blocker under the panel, and
/// the panel above the slot; see the module documentation.
struct MeterLayer {
    slot: Signal<Rect>,
    popup: Option<Box<dyn Widget>>,
    blocker: Option<WidgetId>,
    panel: Option<WidgetId>,
}

impl std::fmt::Debug for MeterLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeterLayer").finish_non_exhaustive()
    }
}

impl Widget for MeterLayer {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let layer = ctx.self_id();
        self.slot
            .bind_to(layer, ctx.binding_registry(), BindingLevel::Relayout);
        // Iced's blocker: a press outside the panel closes the popup and goes
        // no further (`MeterPopupClose` on press and right press).
        let blocker = ctx.add(
            RectWidget::new().on_pointer_event(move |event, ctx| match event {
                WidgetEvent::PointerDown { .. } => {
                    ctx.dismiss_overlay_by_content(layer);
                    EventResponse::Handled
                }
                WidgetEvent::PointerUp { .. } => EventResponse::Handled,
                _ => EventResponse::Ignored,
            }),
        );
        let panel = match self.popup.take() {
            Some(popup) => ctx.add_boxed(popup),
            None => ctx.add(Spacer::new()),
        };
        self.blocker = Some(blocker);
        self.panel = Some(panel);
        vec![blocker, panel]
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }

    fn place_children(
        &self,
        bounds: Rect,
        _proposal: SizeProposal,
        children: &mut [WidgetPlacement],
        ctx: &LayoutContext,
    ) {
        let panel_size = self
            .panel
            .and_then(|id| ctx.child_size(id, SizeProposal::unspecified()))
            .unwrap_or(Size::ZERO);
        let origin = panel_origin(self.slot.get(), panel_size, bounds);
        for child in children.iter_mut() {
            if Some(child.id) == self.panel {
                child.origin = origin;
                child.size = panel_size;
            } else {
                child.origin = bounds.origin();
                child.size = bounds.size();
            }
        }
    }

    fn children(&self) -> Vec<WidgetId> {
        self.blocker.into_iter().chain(self.panel).collect()
    }
}

/// The meter and its popup; see the module documentation.
pub struct MeterSlot {
    vm: MeterViewModel,
    /// Where the slot is in the window, for the popup.
    bounds: Signal<Rect>,
    layer: Rc<std::cell::Cell<Option<WidgetId>>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for MeterSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeterSlot").finish_non_exhaustive()
    }
}

impl MeterSlot {
    /// The slot of `vm`.
    pub fn new(vm: MeterViewModel) -> Self {
        Self {
            vm,
            bounds: Signal::new(Rect::new(0.0, 0.0, 0.0, 0.0)),
            layer: Rc::new(std::cell::Cell::new(None)),
            root: None,
        }
    }

    /// The popup overlay's content root, once built (tests find the panel
    /// under it).
    pub fn layer_handle(&self) -> Rc<std::cell::Cell<Option<WidgetId>>> {
        self.layer.clone()
    }
}

impl Widget for MeterSlot {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.wire(ctx);
        let open = Signal::new(false);

        // Built once and parked; the overlay shows it.
        let layer = ctx.add_detached(MeterLayer {
            slot: self.bounds.clone(),
            popup: Some(Box::new(MeterPopup::new(self.vm.clone()))),
            blocker: None,
            panel: None,
        });
        ctx.set_dormant(layer);
        self.layer.set(Some(layer));
        let anchor = ctx.self_id();
        let show: Rc<dyn Fn(&mut EventContext)> = {
            let open = open.clone();
            Rc::new(move |ctx: &mut EventContext| {
                if open.get() {
                    return;
                }
                let closed = open.clone();
                ctx.activate(layer);
                ctx.show_overlay(
                    OverlayRequest::new(
                        layer,
                        anchor,
                        OverlayPlacement::FullViewport,
                        DismissBehavior::EscapeKey,
                    )
                    .on_dismiss(Rc::new(move |_reason, ctx| {
                        closed.set(false);
                        ctx.set_dormant(layer);
                    })),
                );
                open.set(true);
                ctx.request_focus_into(layer);
            })
        };

        // The meter is hidden, not removed, while it is disabled: showing it
        // from the popup must not rebuild what hosts the popup.
        let vm = self.vm.clone();
        let body = ZStack::new().child(RectWidget::new()).child(
            HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Center)
                .child(
                    VuMeter::new(self.vm.frame(), self.vm.active(), move |now| vm.tick(now))
                        .visible_when(self.vm.enabled()),
                )
                .child(Spacer::new()),
        );
        // A left click does nothing, as in Iced: the body takes the tap before
        // the button around it. A right press opens the popup.
        let right_press = show.clone();
        let body = FixedSize::new()
            .height(SLOT_HEIGHT)
            .child(body)
            .on_tap(|_, _| {})
            .on_pointer_event(move |event, ctx| match event {
                WidgetEvent::PointerDown {
                    button: PointerButton::Secondary,
                    ..
                } => {
                    right_press(ctx);
                    EventResponse::Handled
                }
                _ => EventResponse::Ignored,
            });
        let activate = show.clone();
        let expanded = open.clone();
        open.bind_to(
            anchor,
            ctx.binding_registry(),
            BindingLevel::AccessibilityOnly,
        );
        let root = ctx.add(
            Pressable::new(body, tr!(meter_slot_name()))
                .on_activate(move |ctx| activate(ctx))
                .access_customize(move |builder| {
                    builder.set_has_popup(HasPopup::Dialog);
                    builder.set_expanded(expanded.get());
                }),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, SLOT_HEIGHT))
            .into()
    }

    fn place_children(
        &self,
        bounds: Rect,
        _proposal: SizeProposal,
        children: &mut [WidgetPlacement],
        _ctx: &LayoutContext,
    ) {
        for child in children.iter_mut() {
            child.origin = bounds.origin();
            child.size = bounds.size();
        }
        // Behind an equality guard; the popup's place never changes the
        // slot's.
        self.bounds.set_if_changed(bounds);
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
