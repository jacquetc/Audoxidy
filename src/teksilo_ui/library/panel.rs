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
//! Keys (quirk fix 2): while the list, the grid or the grid's band has the
//! keyboard, the panel takes Enter (play the selection: append it to the
//! active playlist), Delete (confirm, then delete) and the arrows in a
//! preview handler, before the list or the grid sees them. A control of the
//! panel that has the keyboard (an action of the bottom bar, the search
//! field, a sort header or a grip of the column header, a chevron, the meter
//! slot) keeps its own keys: Iced's controls take no key, and its
//! library keys act wherever the library has the focus. In the list modes ↑ and ↓ move the selection (Shift
//! extends it) and ← and → collapse and expand; in the grid Alt+↑ / Alt+↓ and
//! ↑ on the open album close and open it, ↓ from a card of the open album's
//! row enters its songs (the keyboard goes into the band), ↑ and ↓ walk them
//! and ↑ from the first goes back to the open card (the keyboard goes back to
//! the grid, on that card: the band lets the key through and the grid takes
//! it), and a step between cards is the grid's own navigation (G2,
//! [`LibraryViewModel::grid_key`]). Ctrl+A is the app's global shortcut, routed to
//! [`LibraryViewModel::select_all`]. None of these is a global shortcut: a key
//! acts on the panel that has the keyboard.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::binding::BindingLevel;
use teksilo::core::event::{ScrollDelta, WidgetEvent};
use teksilo::core::widget::WidgetPlacement;
use teksilo::core::{Key, Modifiers};
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Expand, FixedSize, Padding, Panel, VStack};

use super::bottom_bar::BottomBar;
use super::grid::LibraryGrid;
use super::hshift::HShift;
use super::library_vm::{GridKey, LibraryViewModel, content_width};
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

/// Whether the panel's keys apply with `focused` holding the keyboard: the
/// list or the grid (`view`), or the grid's band. A control inside the panel
/// (an action of the bottom bar, the search field, a sort header or a grip of
/// the column header, a chevron, the meter slot) keeps its own keys.
pub fn has_the_keys(
    focused: Option<WidgetId>,
    view: &Cell<Option<WidgetId>>,
    band: &Cell<Option<WidgetId>>,
) -> bool {
    focused.is_some() && (focused == view.get() || focused == band.get())
}

/// The panel's keys (see the module documentation), while the list, the
/// grid or the band has the keyboard ([`has_the_keys`]); `band` holds the
/// grid band's id once it is built.
pub fn handle_key(
    vm: &LibraryViewModel,
    band: &Cell<Option<WidgetId>>,
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
                let band = band.get();
                let in_band = band.is_some() && ctx.focused() == band;
                match vm.grid_key(key, modifiers, in_band) {
                    GridKey::Unused => EventResponse::Ignored,
                    GridKey::Used => EventResponse::Handled,
                    GridKey::EnteredBand => {
                        if let Some(band) = band {
                            ctx.request_focus(band);
                        }
                        EventResponse::Handled
                    }
                    // In the band, the key goes on: the band does not use it
                    // and the grid takes the keyboard back, its cursor on the
                    // band's card.
                    GridKey::LeftBand if in_band => EventResponse::Ignored,
                    GridKey::LeftBand => EventResponse::Handled,
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
    width: Rc<Cell<f32>>,
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
        // The shared offset stays within what overflows the header's width
        // (quirk fix 36): written from layout behind an equality guard, and
        // the offset never changes the header's own width.
        self.vm.set_h_viewport(bounds.width);
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// The body of the current mode, rebuilt when the mode changes.
struct ModeBody {
    vm: LibraryViewModel,
    band: Rc<Cell<Option<WidgetId>>>,
    /// The id of the list or the grid that has the panel's keys.
    view: Rc<Cell<Option<WidgetId>>>,
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
            LibraryViewMode::Grid => {
                self.band.set(None);
                ctx.add(
                    LibraryGrid::new(vm)
                        .band_focus(self.band.clone())
                        .view_focus(self.view.clone()),
                )
            }
            mode => ctx.add(
                Padding::new(0.0, 0.0, 0.0, CONTENT_LEADING)
                    .child(LibraryList::new(vm, mode).list_focus(self.view.clone())),
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

    /// Puts `meter` in the bottom bar's meter slot: the free space between the
    /// figures and the actions, never narrower than `meter`'s natural width
    /// (400 px for a shown meter, as in Iced; it can be 0 for a hidden one),
    /// 40 px high. `meter` fills the slot and handles its own right-click
    /// (quirk fix 10). Without one the slot stays, empty.
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
        let key_vm = vm.clone();
        let band = Rc::new(Cell::new(None));
        let key_band = band.clone();
        let view = Rc::new(Cell::new(None));
        let key_view = view.clone();
        let header_width = Rc::new(Cell::new(0.0));
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
                            band,
                            view,
                            root: None,
                        }))
                        .child(Expand::horizontal().child(BottomBar::new(vm, self.meter.take()))),
                )
                .access_role(teksilo::core::accesskit::Role::Region)
                .access_label(tr!(library_panel()))
                .on_key_preview(move |event, ctx| match event {
                    WidgetEvent::KeyDown { key, modifiers, .. }
                        if has_the_keys(ctx.focused(), &key_view, &key_band) =>
                    {
                        handle_key(&key_vm, &key_band, key, *modifiers, ctx)
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
