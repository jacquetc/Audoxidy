//! [`MeterSlot`]: the meter in the library's bottom bar, and the popup it opens.
//!
//! The library's bottom bar reserves a 400 px slot for the meter
//! (`LibraryPanel::meter_slot`, `src/teksilo_ui/library/bottom_bar.rs`; Iced
//! `widgets.rs:4772`) 40 px high. This widget fills it:
//!
//! - **enabled**: the [`VuMeter`], centred (Iced centres it vertically,
//!   `library.rs:1943`; the tall meter is 48 px, the bar 40 px);
//! - **disabled**: nothing is drawn, but the slot keeps its size and stays a
//!   target (quirk fix 10: Iced drops the meter, and with it the only way to
//!   enable it).
//!
//! A right-click anywhere on the slot opens the [`MeterPopup`] (Iced:
//! `MeterRightClicked`, `library.rs:1910`), **anchored to the slot**, above it
//! (quirk fix 7: Iced passes `Point::ORIGIN`, so the popup always opened at the
//! window's top-left). A left click does nothing, as in Iced. Not in Iced:
//! the slot is also a keyboard stop, and Enter or Space on it opens the popup;
//! it is named "Ajustes del medidor" for assistive technology.
//!
//! The popup closes on Escape and on a click outside (quirk fix 4).

use teksilo::core::ButtonMask;
use teksilo::core::binding::BindingLevel;
use teksilo::core::overlay::OverlayPlacement;
use teksilo::prelude::*;
use teksilo::widgets::{Center, FixedSize, OverlayTrigger, PopoverCustom, RectWidget, ZStack};

use super::meter_vm::MeterViewModel;
use super::popup::MeterPopup;
use crate::teksilo_ui::widgets::VuMeter;

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
        let enabled = self.vm.enabled();
        enabled.bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        self.vm.wire(ctx);

        let mut body = ZStack::new().child(RectWidget::new());
        if enabled.get() {
            let vm = self.vm.clone();
            body = body.child(Center::new().child(VuMeter::new(
                self.vm.frame(),
                self.vm.active(),
                move |now| vm.tick(now),
            )));
        }
        // The popover's tap lands on the slot: only the secondary button counts.
        let slot = FixedSize::new()
            .width(SLOT_WIDTH)
            .height(SLOT_HEIGHT)
            .child(body)
            .accept_tap_buttons(ButtonMask::SECONDARY);
        let trigger = OverlayTrigger::around(slot).named(tr!(meter_slot_name()).resolve_now());
        let root = ctx.add(
            PopoverCustom::new(trigger)
                .content(MeterPopup::new(self.vm.clone()))
                .bare()
                .placement(OverlayPlacement::Above),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(SLOT_WIDTH, SLOT_HEIGHT))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
