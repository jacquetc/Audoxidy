//! [`PlaylistPanel`]: the whole playlist column, as the app mounts it.
//!
//! The Iced view (`view`, `src/gui/playlist.rs:398`) is a column of the tab
//! strip, the list and the bottom bar, 400 px wide, on `COLOR_BG`; a press
//! anywhere no control takes clears the selection (`PlaylistDeselect`): the
//! panel sees every press first and lets those a control claims go.
//!
//! Building the panel also subscribes its view-model to the facade's events,
//! for as long as the panel lives (the life of the window):
//!
//! - `Playback`: the end of a track and the position (preload);
//! - `Session`: the autosave;
//! - `ExternalControl`: the system media controls;
//! - `LongOperation`, with a context, so the end of an export, an import or
//!   an added folder shows its toast (quirk fix 8).

use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::{PointerButton, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Expand, FixedSize, Panel, VStack};

use super::bottom_bar::{self, BottomBar};
use super::list::PlaylistList;
use super::notice;
use super::playlist_vm::PlaylistViewModel;
use super::tabs::{self, TabsStrip};
use crate::frontend::{Event, Origin};
use crate::teksilo_ui::models::load::prepare_runtime;

/// The playlist panel; see the module documentation.
pub struct PlaylistPanel {
    vm: PlaylistViewModel,
    list: Rc<Cell<Option<WidgetId>>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for PlaylistPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistPanel").finish_non_exhaustive()
    }
}

impl PlaylistPanel {
    /// The panel of `vm`.
    pub fn new(vm: PlaylistViewModel) -> Self {
        Self {
            vm,
            list: Rc::new(Cell::new(None)),
            root: None,
        }
    }

    /// The view-model.
    pub fn view_model(&self) -> &PlaylistViewModel {
        &self.vm
    }

    /// A handle on the list's id (the `ListView` that takes the keyboard),
    /// set when the panel is built: the app focuses it to route Ctrl+A.
    pub fn list_id(&self) -> Rc<Cell<Option<WidgetId>>> {
        self.list.clone()
    }

    /// Subscribes the view-model to the facade's events.
    fn subscribe(&self, ctx: &mut BuildContext) {
        prepare_runtime(ctx, &self.vm.services().runtime);
        for origin in [Origin::Playback, Origin::Session, Origin::ExternalControl] {
            let vm = self.vm.clone();
            ctx.subscribe_event(origin, move |event: &Event| vm.on_event(event));
        }
        let vm = self.vm.clone();
        ctx.subscribe_event_with_ctx(Origin::LongOperation, move |event: &Event, ctx| {
            if let Event::LongOperation(op) = event
                && let Some(notice) = vm.on_long_op(op)
            {
                notice::present(&notice, ctx);
            }
        });
    }
}

impl Widget for PlaylistPanel {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.subscribe(ctx);
        let list = PlaylistList::with_list_handle(self.vm.clone(), self.list.clone());
        let (vm, focus) = (self.vm.clone(), self.list.clone());
        let root = ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .background(SurfaceRole::Main)
                .child(
                    VStack::new()
                        .spacing(0.0)
                        .alignment(HAlignment::Leading)
                        .child(
                            Expand::horizontal().child(
                                FixedSize::new()
                                    .height(tabs::HEIGHT)
                                    .child(TabsStrip::new(self.vm.clone())),
                            ),
                        )
                        .child(Expand::new().child(Expand::horizontal().child(list)))
                        .child(
                            Expand::horizontal().child(
                                FixedSize::new()
                                    .height(bottom_bar::HEIGHT)
                                    .child(BottomBar::new(self.vm.clone())),
                            ),
                        ),
                )
                .on_pointer_event(move |event, ctx| {
                    // `PlaylistDeselect`: a press no control takes (a tab, an
                    // icon, the field and the list take theirs).
                    if let WidgetEvent::PointerDown {
                        button: PointerButton::Primary,
                        ..
                    } = event
                        && !ctx.press_claimed_by_interactive_child()
                    {
                        vm.deselect();
                        if let Some(list) = focus.get() {
                            ctx.request_focus(list);
                        }
                    }
                    EventResponse::Ignored
                }),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let child = self
            .root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0));
        // The panel fills its region.
        proposal.resolve(child.width, child.height).into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
