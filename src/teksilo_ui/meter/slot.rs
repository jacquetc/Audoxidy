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
//! The popup is placed with `NearAnchor`, not `Above`: a popover placed
//! `Above` or `Below` its anchor is made at least as wide as the anchor, which
//! would stretch Iced's 280 px panel to the slot's 400 px.
//!
//! The popup closes on Escape and on a click outside (quirk fix 4).

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use teksilo::canvas::Vec2;
use teksilo::core::intent::Intent;
use teksilo::core::overlay::OverlayPlacement;
use teksilo::core::{ButtonMask, PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::VAlignment;
use teksilo::widgets::{
    FixedSize, HStack, OverlayTrigger, PopoverCustom, RectWidget, Spacer, ZStack,
};

use super::meter_vm::MeterViewModel;
use super::popup::MeterPopup;
use crate::teksilo_ui::widgets::VuMeter;

/// The intent a right-click sends to open the popup.
const OPEN_INTENT: &str = "meter.popup";

/// How long after the popup closed a press is taken as the one that closed it.
const JUST_CLOSED: Duration = Duration::from_millis(100);

/// The slot's width (`widgets.rs:4772`).
pub const SLOT_WIDTH: f32 = 400.0;
/// The slot's height: the bottom bar's (`library.rs:2017`).
pub const SLOT_HEIGHT: f32 = 40.0;

/// The meter and its popup; see the module documentation.
pub struct MeterSlot {
    vm: MeterViewModel,
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
        Self { vm, root: None }
    }
}

impl Widget for MeterSlot {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.wire(ctx);
        let closed_at: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
        let on_close = closed_at.clone();

        // The meter is hidden, not removed, while it is disabled: showing it
        // from the popup must not rebuild the popover that hosts the popup.
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
        // The popover's own tap (a primary press) must not open the popup, as in
        // Iced; Enter, Space and the assistive "click" still do, and a secondary
        // press asks for it by intent.
        let slot = FixedSize::new()
            .height(SLOT_HEIGHT)
            .child(body)
            .accept_tap_buttons(ButtonMask::NONE)
            .on_pointer_event(move |event, ctx| match event {
                WidgetEvent::PointerDown {
                    button: PointerButton::Secondary,
                    ..
                } => {
                    // A press outside an open popup closes it before this
                    // handler runs: that press must not open it again (Iced:
                    // a right-click on the blocker closes the popup).
                    let just_closed = closed_at.get().is_some_and(|at| at.elapsed() < JUST_CLOSED);
                    if !just_closed {
                        ctx.send_intent(Intent::new(OPEN_INTENT));
                    }
                    EventResponse::Handled
                }
                _ => EventResponse::Ignored,
            });
        let trigger = OverlayTrigger::around(slot).named(tr!(meter_slot_name()).resolve_now());
        let root = ctx.add(
            PopoverCustom::new(trigger)
                .content(MeterPopup::new(self.vm.clone()))
                .bare()
                .open_action(OPEN_INTENT)
                .on_close(move || on_close.set(Some(Instant::now())))
                .placement(OverlayPlacement::NearAnchor {
                    offset: Vec2::new(0.0, 0.0),
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

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
