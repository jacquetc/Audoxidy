//! [`LibraryPanel`]: the library, right of the filter column.
//!
//! Port of the Iced library view under its title strip (`src/gui/library.rs:1401-2027`;
//! the strip itself is the app's title bar). From top to bottom:
//!
//! - the column header of the view mode (28 px and its 2 px rule,
//!   [`ColumnHeader`]): its sort cycle and its resize go to the view-model,
//!   and it scrolls sideways with the rows, by one shared offset (quirk fix
//!   36; any wheel over it scrolls sideways, as over Iced's horizontal-only
//!   header);
//! - the body, 5 px in from the leading edge (`library.rs:1945-1954`): the
//!   grid ([`LibraryGrid`], whose open album shows its songs under its row
//!   of cards), or one of the three lists ([`LibraryList`]);
//! - the bottom bar ([`BottomBar`]).
//!
//! Keys (quirk fix 2): the panel takes Enter (play the selection: append it
//! to the active playlist), Delete (confirm, then delete) and the arrows in a
//! preview handler, before the list or the grid sees them, unless a text
//! field has the focus. In the list modes ↑ and ↓ move the selection (Shift
//! extends it) and ← and → collapse and expand; in the grid Alt+↑ / Alt+↓ and
//! ↑ on the open album close and open it, and the other arrows are the
//! grid's own navigation. Ctrl+A is the app's global shortcut, routed to
//! [`LibraryViewModel::select_all`]. None of these is a global shortcut: a key
//! acts on the panel that has the keyboard.

use teksilo::core::binding::BindingLevel;
use teksilo::core::event::{ScrollDelta, WidgetEvent};
use teksilo::core::{Key, Modifiers};
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Expand, FixedSize, Padding, Panel, VStack};

use super::bottom_bar::BottomBar;
use super::grid::LibraryGrid;
use super::hshift::HShift;
use super::library_vm::{LibraryViewModel, content_width};
use super::list_rows::LibraryList;
use crate::logic::library_geometry::LibraryViewMode;
use crate::logic::library_selection::LibraryNavDir;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::widgets::ColumnHeader;
use crate::teksilo_ui::widgets::column_header::{self, SortArrows, columns_for, leading_padding};
use crate::teksilo_ui::widgets::stock_styles::SCROLL_LINE_HEIGHT;

/// The space before the body (`library.rs:1951`; the grid has it as its
/// leading inset).
pub const CONTENT_LEADING: f32 = 5.0;

/// The direction of an arrow key.
pub fn nav_dir(key: &Key) -> Option<LibraryNavDir> {
    match key {
        Key::ArrowUp => Some(LibraryNavDir::Up),
        Key::ArrowDown => Some(LibraryNavDir::Down),
        Key::ArrowLeft => Some(LibraryNavDir::Left),
        Key::ArrowRight => Some(LibraryNavDir::Right),
        _ => None,
    }
}

/// The panel's keys (see the module documentation), for a key that does not
/// come from a text field.
pub fn handle_key(
    vm: &LibraryViewModel,
    key: &Key,
    modifiers: Modifiers,
    ctx: &mut EventContext,
) -> EventResponse {
    match key {
        Key::Enter => {
            vm.play_selection();
            EventResponse::Handled
        }
        Key::Delete => {
            vm.request_delete(ctx);
            EventResponse::Handled
        }
        _ => match nav_dir(key) {
            Some(_) if vm.view_mode().get() == LibraryViewMode::Grid => {
                if vm.grid_key(key, modifiers) {
                    EventResponse::Handled
                } else {
                    EventResponse::Ignored
                }
            }
            Some(dir) => {
                vm.navigate(dir, modifiers);
                EventResponse::Handled
            }
            None => EventResponse::Ignored,
        },
    }
}

/// The sort chevrons of the column header.
const ARROWS: SortArrows = SortArrows {
    up: icons::shared::arrow_up_chevron,
    down: icons::shared::arrow_down_chevron,
};

/// The column header of the current mode, rebuilt when the mode changes.
struct ModeHeader {
    vm: LibraryViewModel,
    width: std::rc::Rc<std::cell::Cell<f32>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ModeHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModeHeader").finish_non_exhaustive()
    }
}

impl Widget for ModeHeader {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        vm.view_mode()
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let mode = vm.view_mode().get();
        let (sort_vm, resize_vm) = (vm.clone(), vm.clone());
        let width = vm
            .column_widths()
            .map(move |widths| content_width(mode, widths));
        let header = ColumnHeader::new(columns_for(mode), vm.column_widths(), vm.sort(), ARROWS)
            .leading_padding(leading_padding(mode))
            .on_sort(move |_column, next, _ctx| sort_vm.set_sort(next))
            .on_resize(move |column, width, _ctx| resize_vm.set_column_width(column, width));
        let wheel_vm = vm.clone();
        let viewport = self.width.clone();
        let root = ctx.add(
            FixedSize::new()
                .height(column_header::HEIGHT + column_header::RULE_HEIGHT)
                .child(HShift::new(vm.h_offset(), header).min_width(width))
                .on_scroll(move |event, _ctx| match event {
                    WidgetEvent::Scroll { delta, .. } => {
                        // Over the header any wheel scrolls sideways.
                        let step = match delta {
                            ScrollDelta::Lines { x, y } => {
                                (if *x != 0.0 { *x } else { *y }) * SCROLL_LINE_HEIGHT
                            }
                            ScrollDelta::Pixels { x, y } => {
                                if *x != 0.0 {
                                    *x
                                } else {
                                    *y
                                }
                            }
                        };
                        wheel_vm.scroll_horizontally(-step, viewport.get());
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                }),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        if let Some(width) = proposal.width {
            self.width.set(width);
        }
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| {
                proposal.resolve(0.0, column_header::HEIGHT + column_header::RULE_HEIGHT)
            })
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// The body of the current mode, rebuilt when the mode changes.
struct ModeBody {
    vm: LibraryViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ModeBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModeBody").finish_non_exhaustive()
    }
}

impl Widget for ModeBody {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        vm.view_mode()
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let root = match vm.view_mode().get() {
            LibraryViewMode::Grid => ctx.add(LibraryGrid::new(vm)),
            mode => ctx.add(
                Padding::new(0.0, 0.0, 0.0, CONTENT_LEADING).child(LibraryList::new(vm, mode)),
            ),
        };
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

/// The library panel; see the module documentation.
pub struct LibraryPanel {
    vm: LibraryViewModel,
    meter: Option<Box<dyn Widget>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for LibraryPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryPanel")
            .field("vm", &self.vm)
            .field("meter", &self.meter.is_some())
            .finish_non_exhaustive()
    }
}

impl LibraryPanel {
    /// The panel of `vm`.
    pub fn new(vm: LibraryViewModel) -> Self {
        Self {
            vm,
            meter: None,
            root: None,
        }
    }

    /// Puts `meter` in the bottom bar's 400 px meter slot. It fills the slot
    /// (400 × 40 px) and handles its own right-click (quirk fix 10). Without
    /// one the slot stays, empty.
    pub fn meter_slot(mut self, meter: impl Widget + 'static) -> Self {
        self.meter = Some(Box::new(meter));
        self
    }

    /// The panel's view-model.
    pub fn view_model(&self) -> &LibraryViewModel {
        &self.vm
    }
}

impl Widget for LibraryPanel {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        vm.wire(ctx);
        let surfaces = ctx.text_surfaces();
        let key_vm = vm.clone();
        let header_width = std::rc::Rc::new(std::cell::Cell::new(0.0));
        let root = ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .background(SurfaceRole::Main)
                .child(
                    VStack::new()
                        .spacing(0.0)
                        .alignment(HAlignment::Leading)
                        .child(Expand::horizontal().child(ModeHeader {
                            vm: vm.clone(),
                            width: header_width,
                            root: None,
                        }))
                        .child(Expand::new().child(ModeBody {
                            vm: vm.clone(),
                            root: None,
                        }))
                        .child(Expand::horizontal().child(BottomBar::new(vm, self.meter.take()))),
                )
                .access_role(teksilo::core::accesskit::Role::Region)
                .access_label(tr!(library_panel()))
                .on_key_preview(move |event, ctx| match event {
                    WidgetEvent::KeyDown { key, modifiers, .. }
                        if !surfaces.focused_is_text_surface() =>
                    {
                        handle_key(&key_vm, key, *modifiers, ctx)
                    }
                    _ => EventResponse::Ignored,
                }),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let size = self
            .root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0));
        // The panel fills its region.
        proposal.resolve(size.width, size.height).into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
