//! [`GridDetail`]: the songs of the grid's open album, in the grid's detail
//! band, under the album's row of cards.
//!
//! Port of the Iced inline expansion (`LibraryGridElement::ExpandedSongs`,
//! `src/gui/library.rs:1653-1719`), shown by `GridView::detail_row`:
//!
//! - the block is a container on the background colour, padded 0 px on top,
//!   10 px after, 5 px under and 5 px before (`library.rs:1706-1717`; with the
//!   content's own 5 px, the grid's leading inset, the rows start 10 px in
//!   from the panel's edge), around a column padded 32 px above and below
//!   (`library.rs:1659`) of 32 px song rows with the Grid columns
//!   (`library.rs:1666-1699`). Iced draws no header in the block, and neither
//!   does this; the column header above the grid names the columns;
//! - the band is therefore `32 + 32 n + 32 + 5` px tall for `n` songs. With
//!   no song to show (still loading, or none the search keeps) Iced draws no
//!   block (`library.rs:563-571`), and the band takes no space;
//! - Iced realizes only the songs near the viewport, half a viewport above
//!   and below it (a quarter in low-resource mode), at least 100 px
//!   (`library.rs:507-515`, `:536-571`), with empty space for the others. The
//!   band does the same: a `Repeater` over the window of rows, between two
//!   spacers, reconciled by song id as the grid scrolls, so a scroll step
//!   builds only the rows that enter the window;
//! - the rows scroll sideways with the column header, by the shared offset
//!   (quirk fix 36), and a sideways wheel over them moves it;
//! - a click selects a song (Ctrl toggles, Shift extends), as `SelectSong`
//!   does in Grid mode, and gives the keyboard to the band;
//! - the band takes the keyboard (G2): it is a focus stop the grid's ↓ from
//!   the open card enters, and where the library's keys walk the songs
//!   (`LibraryViewModel::grid_key`). Its current song is the one the
//!   selection focuses, and that row, not the whole band, is what a focus
//!   change scrolls into view ([`Widget::focus_reveal_rect`]).
//!
//! The rows are the album's songs the search keeps, matched without regard
//! to case like the rest of the search (quirk fix 53,
//! [`visible_album_songs`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::core::event::{ScrollDelta, WidgetEvent};
use teksilo::core::widget::WidgetPlacement;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::data::ListModel;
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{FixedSize, GridSizing, Padding, Panel, Repeater, VStack};

use super::grid;
use super::hshift::HShift;
use super::library_vm::{LibraryViewModel, visible_album_songs};
use super::song_row::{ROW_HEIGHT, SongRow};
use crate::db::database::SongData;
use crate::logic::library_geometry::{LibraryViewMode, scroll_offset_to_reveal};
use crate::logic::library_selection::LibraryListItem;
use crate::teksilo_ui::widgets::column_header::columns_for;
use crate::teksilo_ui::widgets::stock_styles::SCROLL_LINE_HEIGHT;

/// The space before the songs, inside the band (`library.rs:1713`).
pub const LEADING: f32 = 5.0;
/// The space after the songs (`library.rs:1711`).
pub const TRAILING: f32 = 10.0;
/// The space above the songs (`library.rs:1659`).
pub const PADDING_TOP: f32 = 32.0;
/// The space under the songs: the column's 32 px (`library.rs:1659`) and the
/// container's 5 px (`library.rs:1712`).
pub const PADDING_BOTTOM: f32 = 32.0 + 5.0;
/// The fewest pixels realized above and below the viewport
/// (`library.rs:513`).
pub const MIN_MARGIN: f32 = 100.0;

/// The height of a band showing `count` songs: none for no song.
pub fn band_height(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        PADDING_TOP + count as f32 * ROW_HEIGHT + PADDING_BOTTOM
    }
}

/// The rows `start..end` of `count` rows starting at `first` (content space)
/// that Iced renders for the content span `low..high`: those with
/// `y + 32 >= low` and `y <= high` (`library.rs:553-559`).
pub fn rows_in_span(first: f32, count: usize, low: f32, high: f32) -> (usize, usize) {
    let clamp = |value: f32| {
        if value.is_finite() && value > 0.0 {
            // Saturating float-to-int cast; clamped to the rows.
            (value as usize).min(count)
        } else {
            0
        }
    };
    let start = clamp(((low - first) / ROW_HEIGHT - 1.0).ceil());
    let end = clamp(((high - first) / ROW_HEIGHT).floor() + 1.0);
    (start, end.max(start))
}

/// A realized row's song. Two are equal when they are the same song value,
/// so a window that moves keeps the rows it still holds.
#[derive(Debug, Clone)]
pub struct BandSong(pub Arc<SongData>);

impl PartialEq for BandSong {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Where the band sits in the grid, for the window of rows it realizes.
#[derive(Clone)]
pub struct BandPlacement {
    /// The grid's vertical scroll offset.
    pub scroll_y: Signal<f32>,
    /// The height of the grid's viewport.
    pub viewport: Signal<f32>,
    /// The grid's sizing (its column count).
    pub sizing: Signal<GridSizing>,
    /// The tile whose band is open.
    pub expanded: Signal<Option<usize>>,
    /// Whether the margin is a quarter of the viewport instead of a half
    /// (low-resource mode, `library.rs:508-512`).
    pub low_resource: bool,
    /// The tile whose band has been laid out with its songs, if any.
    pub ready: Rc<Cell<Option<usize>>>,
    /// A song row to reveal once the band of its tile is laid out:
    /// `(tile, row, force_top)`.
    pub pending_reveal: Rc<Cell<Option<(usize, usize, bool)>>>,
}

impl std::fmt::Debug for BandPlacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BandPlacement")
            .field("scroll_y", &self.scroll_y.get())
            .field("expanded", &self.expanded.get())
            .finish_non_exhaustive()
    }
}

impl BandPlacement {
    /// The content-space y of the band's first song row: under the open
    /// tile's row of 252 px cards (rows above it have no band), and the
    /// band's 32 px of padding.
    pub fn first_row_y(&self) -> Option<f32> {
        let tile = self.expanded.get()?;
        let columns = match self.sizing.get() {
            GridSizing::FixedColumnCount { count, .. } => count.max(1),
            _ => 1,
        };
        Some((tile / columns + 1) as f32 * grid::ROW_HEIGHT + PADDING_TOP)
    }

    /// Reveals the song row `row` of the open band: at the top when
    /// `force_top`, as Iced's reveal does (`get_scroll_task(true)`,
    /// `src/gui/app.rs:3319`), else only as far as needed, as after a key
    /// (`get_scroll_task(false)`, `:3213`). Songs in Grid mode take no
    /// sticky-header margin.
    pub fn reveal_row(&self, row: usize, force_top: bool) {
        let Some(first) = self.first_row_y() else {
            return;
        };
        let viewport = self.viewport.get();
        let known = (viewport > 0.0).then(|| (self.scroll_y.get(), viewport));
        if let Some(offset) = scroll_offset_to_reveal(
            first + row as f32 * ROW_HEIGHT,
            ROW_HEIGHT,
            Some(&LibraryListItem::Song(0)),
            LibraryViewMode::Grid,
            force_top,
            known,
        ) {
            self.scroll_y.set(offset);
        }
    }

    /// Reveals the song row `row` of the band under `tile` (see
    /// [`reveal_row`](Self::reveal_row)): now when that band is laid out,
    /// else once it is. Revealing it before would let the grid's scroll
    /// anchoring move the view by the height of a band that opens above it.
    pub fn request_reveal(&self, tile: usize, row: usize, force_top: bool) {
        if self.ready.get() == Some(tile) && self.expanded.get() == Some(tile) {
            self.reveal_row(row, force_top);
        } else {
            self.pending_reveal.set(Some((tile, row, force_top)));
        }
    }

    /// The band of the open tile was laid out showing `count` songs.
    fn laid_out(&self, count: usize) {
        if count == 0 {
            return;
        }
        let tile = self.expanded.get();
        self.ready.set(tile);
        if let Some((wanted, row, force_top)) = self.pending_reveal.get()
            && Some(wanted) == tile
        {
            self.pending_reveal.set(None);
            self.reveal_row(row, force_top);
        }
    }

    /// The rows of `count` to realize now.
    fn window(&self, count: usize) -> (usize, usize) {
        let Some(first) = self.first_row_y() else {
            return (0, count);
        };
        let viewport = self.viewport.get();
        let ratio = if self.low_resource { 0.25 } else { 0.5 };
        let margin = (viewport * ratio).max(MIN_MARGIN);
        let scroll = self.scroll_y.get();
        rows_in_span(first, count, scroll - margin, scroll + viewport + margin)
    }
}

/// The band's state, shared with its effects.
struct BandState {
    vm: LibraryViewModel,
    placement: Option<BandPlacement>,
    /// The songs the band shows (the search's), all of them.
    songs: RefCell<Vec<Arc<SongData>>>,
    count: Signal<usize>,
    /// The realized rows: `songs[range]`.
    window: ListModel<BandSong>,
    range: Cell<(usize, usize)>,
    top_space: Signal<f32>,
    bottom_space: Signal<f32>,
}

impl BandState {
    /// Re-reads the songs to show; the window follows.
    fn refresh_songs(&self) {
        let visible = visible_album_songs(&self.vm.expanded_songs().get(), &self.vm.search().get());
        let changed = {
            let songs = self.songs.borrow();
            songs.len() != visible.len()
                || songs.iter().zip(&visible).any(|(a, b)| !Arc::ptr_eq(a, b))
        };
        if !changed {
            return;
        }
        *self.songs.borrow_mut() = visible;
        self.update_window(true);
    }

    /// Moves the window of realized rows; `songs_changed` rebuilds it.
    fn update_window(&self, songs_changed: bool) {
        let songs = self.songs.borrow();
        let count = songs.len();
        let (start, end) = match &self.placement {
            Some(placement) => placement.window(count),
            None => (0, count),
        };
        if !songs_changed && self.range.get() == (start, end) {
            return;
        }
        self.range.set((start, end));
        let rows: Vec<BandSong> = songs[start..end].iter().cloned().map(BandSong).collect();
        drop(songs);
        if songs_changed {
            self.window.replace_all(rows);
        } else {
            self.window.reconcile_by_key(rows, |song| song.0.id);
        }
        self.top_space.set_if_changed(start as f32 * ROW_HEIGHT);
        self.bottom_space
            .set_if_changed((count - end) as f32 * ROW_HEIGHT);
        self.count.set_if_changed(count);
    }
}

/// An effect that moves `state`'s window of rows.
fn follow_window<T>(state: &Rc<BandState>) -> impl Fn(&T) + 'static {
    let state = state.clone();
    move |_| state.update_window(false)
}

/// The detail band; see the module documentation.
pub struct GridDetail {
    state: Rc<BandState>,
    focus: Rc<Cell<Option<WidgetId>>>,
    width: Rc<Cell<f32>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for GridDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridDetail")
            .field("songs", &self.state.songs.borrow().len())
            .field("window", &self.state.range.get())
            .finish_non_exhaustive()
    }
}

impl GridDetail {
    /// The band of `vm`'s open album, every row realized.
    pub fn new(vm: LibraryViewModel) -> Self {
        let state = Rc::new(BandState {
            vm,
            placement: None,
            songs: RefCell::new(Vec::new()),
            count: Signal::new(0),
            window: ListModel::new(),
            range: Cell::new((0, 0)),
            top_space: Signal::new(0.0),
            bottom_space: Signal::new(0.0),
        });
        Self {
            state,
            focus: Rc::new(Cell::new(None)),
            width: Rc::new(Cell::new(0.0)),
            root: None,
        }
    }

    /// The band inside a grid placed as `placement` says: only the rows near
    /// the viewport are realized.
    pub fn placed(self, placement: BandPlacement) -> Self {
        let state = Rc::new(BandState {
            vm: self.state.vm.clone(),
            placement: Some(placement),
            songs: RefCell::new(Vec::new()),
            count: Signal::new(0),
            window: ListModel::new(),
            range: Cell::new((0, 0)),
            top_space: Signal::new(0.0),
            bottom_space: Signal::new(0.0),
        });
        Self { state, ..self }
    }

    /// Writes the band's id to `focus` when it is built: the view moves the
    /// keyboard into the band through it (G2). A press on a row gives the
    /// keyboard to the band whether or not this is called.
    pub fn focus_target(mut self, focus: Rc<Cell<Option<WidgetId>>>) -> Self {
        self.focus = focus;
        self
    }

    /// The row of the band's current song (the song the selection focuses),
    /// if the band shows it.
    fn current_row(&self) -> Option<usize> {
        let LibraryListItem::Song(id) = self.state.vm.selection().get().focused? else {
            return None;
        };
        self.state.songs.borrow().iter().position(|s| s.id == id)
    }

    /// The rows realized now.
    pub fn rows(&self) -> ListModel<BandSong> {
        self.state.window.clone()
    }

    /// How many songs the band shows (realized or not).
    pub fn count(&self) -> Signal<usize> {
        self.state.count.clone()
    }

    /// One song row.
    fn row(
        vm: &LibraryViewModel,
        song: &Arc<SongData>,
        focus: &Rc<Cell<Option<WidgetId>>>,
    ) -> Box<dyn Widget> {
        let item = LibraryListItem::Song(song.id);
        let selected = {
            let item = item.clone();
            vm.selection().map(move |s| s.selected.contains(&item))
        };
        let path = song.full_file_path.clone();
        let playing = vm
            .playing_path()
            .map(move |playing| playing.as_str() == path.as_ref());
        let press_vm = vm.clone();
        let focus = focus.clone();
        let row = SongRow::new(
            song.clone(),
            columns_for(LibraryViewMode::Grid),
            vm.column_widths(),
        )
        .selected(selected)
        .playing(playing)
        .on_press(move |modifiers, ctx| {
            press_vm.click(item.clone(), modifiers, Instant::now());
            if let Some(target) = focus.get() {
                ctx.request_focus(target);
            }
        });
        Box::new(HShift::new(vm.h_offset(), row))
    }
}

impl Widget for GridDetail {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let state = self.state.clone();
        let vm = state.vm.clone();
        // The band is a focus stop (G2): the grid's ↓ from the open card
        // enters it, and the library's keys walk its songs.
        ctx.apply_self_handlers(HandlerSet::new().focusable(true));
        self.focus.set(Some(ctx.self_id()));
        state.count.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Relayout,
        );
        state.refresh_songs();
        {
            let state = state.clone();
            ctx.effect(&vm.search(), move |_| state.refresh_songs());
        }
        {
            let state = state.clone();
            ctx.effect(&vm.expanded_songs(), move |_| state.refresh_songs());
        }
        if let Some(placement) = state.placement.clone() {
            // The window follows the scroll, the viewport, the column count
            // and the band's tile.
            ctx.effect(&placement.scroll_y, follow_window(&state));
            ctx.effect(&placement.viewport, follow_window(&state));
            ctx.effect(&placement.sizing, follow_window(&state));
            ctx.effect(&placement.expanded, follow_window(&state));
        }

        let row_vm = vm.clone();
        let focus = self.focus.clone();
        let rows = Repeater::new(state.window.clone(), move |song: &BandSong| {
            Self::row(&row_vm, &song.0, &focus)
        })
        .spacing(0.0)
        .access_role(Role::List)
        .access_label(tr!(library_album_songs()));
        let content = VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(FixedSize::new().height(state.top_space.clone()))
            .child(rows)
            .child(FixedSize::new().height(state.bottom_space.clone()));
        let wheel_vm = vm.clone();
        let width = self.width.clone();
        let root = ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .background(SurfaceRole::Main)
                .child(Padding::new(PADDING_TOP, TRAILING, PADDING_BOTTOM, LEADING).child(content))
                .on_scroll(move |event, _ctx| match event {
                    WidgetEvent::Scroll { delta, .. } => {
                        let dx = match delta {
                            ScrollDelta::Lines { x, .. } => x * SCROLL_LINE_HEIGHT,
                            ScrollDelta::Pixels { x, .. } => *x,
                        };
                        if dx == 0.0 {
                            return EventResponse::Ignored;
                        }
                        wheel_vm.scroll_horizontally(-dx, width.get());
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                }),
        );
        self.root = Some(root);
        vec![root]
    }

    fn focus_reveal_rect(&self, bounds: Rect) -> Option<Rect> {
        // The current song's row, not the whole band, which can be taller
        // than the grid.
        let row = self.current_row()?;
        Some(Rect::new(
            bounds.x,
            bounds.y + PADDING_TOP + row as f32 * ROW_HEIGHT,
            bounds.width,
            ROW_HEIGHT,
        ))
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let count = self.state.count.get();
        if count == 0 {
            // No block at all, as Iced draws none.
            return Size::new(proposal.width.unwrap_or(0.0), 0.0).into();
        }
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, band_height(count)))
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
        // The viewport the sideways offset is clamped against: the grid's
        // width, which the band spans.
        self.width.set(bounds.width + grid::LEADING_INSET);
        if let Some(placement) = &self.state.placement {
            placement.laid_out(self.state.count.get());
        }
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
