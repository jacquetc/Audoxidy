//! The rows of the list modes: [`LibraryRowsSource`] and [`LibraryList`].
//!
//! The Iced list modes (`universal_song_list` and `detailed_song_list`,
//! `src/gui/widgets.rs:2573-3115`) cull their rows by hand around the scroll
//! offset (`logic::library_geometry::visible_rows`). Here a stock `ListView`
//! virtualizes them, over a [`LibraryRowsSource`]: the flat rows of the
//! view-model's [`ListStructure`] (`logic::library_geometry::list_rows`),
//! each with its height, given to the `ListView` through `item_height_fn`.
//! The source is replaced whole whenever the structure changes (a collapse,
//! an expansion, the sort, the search, the filters), so the `ListView`
//! re-reads every height.
//!
//! The rows ([`LibraryList`]'s delegate):
//!
//! - an artist header ([`HeaderRow::artist`]);
//! - Simple and Thumbnail modes: an album header ([`HeaderRow::album`],
//!   never marked as playing, `widgets.rs:2687`) and the song rows
//!   ([`SongRow`], with a cover in Thumbnail mode);
//! - Detailed mode: an album block ([`AlbumBlock`]).
//!
//! Every row is 10 px in from the list's leading edge and 15 px from its
//! trailing one (`widgets.rs:2714-2719`). The song rows and the album blocks
//! follow the column header's horizontal offset ([`HShift`], quirk fix 36);
//! the headers, which have no columns, stay put.
//!
//! [`LibraryList`] also carries what surrounds the rows in Iced: the sticky
//! artist header ([`StickyHeader`]), the
//! empty message, a click on the background (it deselects), the scroll
//! requests of the view-model, the horizontal wheel and the marquee.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use teksilo::core::Modifiers;
use teksilo::core::event::{ScrollDelta, WidgetEvent};
use teksilo::core::signal::ObserverHandle;
use teksilo::core::widget::WidgetPlacement;
use teksilo::data::{DataChange, ListDataSource, ListModel};
use teksilo::prelude::*;
use teksilo::tokens::FontWeight;
use teksilo::widgets::{Center, ListView, Padding, ScrollBarMode, TextWidget, ZStack};

use super::detailed_list::AlbumBlock;
use super::header_rows::HeaderRow;
use super::hshift::HShift;
use super::library_vm::{LibraryViewModel, ScrollRequest};
use super::marquee::{Marquee, MarqueeController};
use super::song_row::SongRow;
use super::sticky_header::StickyHeader;
use super::structure::ListStructure;
use crate::db::database::SongData;
use crate::logic::library_geometry::{
    LibraryViewMode, ListMetrics, ListRow, ListRowKind, scroll_offset_to_reveal,
};
use crate::logic::library_groups::album_composite_id;
use crate::logic::library_selection::LibraryListItem;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::column_header::columns_for;
use crate::teksilo_ui::widgets::stock_styles::SCROLL_LINE_HEIGHT;

/// The space before every row (`widgets.rs:2716`).
pub const LIST_LEADING: f32 = 10.0;
/// The space after every row (`widgets.rs:2717`).
pub const LIST_TRAILING: f32 = 15.0;
/// The empty list's text size (Iced's default text size).
pub const EMPTY_TEXT_SIZE: f32 = 16.0;

/// The flat rows of a [`ListStructure`] as a `ListDataSource`; see the module
/// documentation. Clones share one list.
#[derive(Clone)]
pub struct LibraryRowsSource {
    rows: ListModel<ListRow>,
    version: Signal<u64>,
}

impl std::fmt::Debug for LibraryRowsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryRowsSource")
            .field("rows", &self.rows.len())
            .field("version", &self.version.get())
            .finish()
    }
}

impl Default for LibraryRowsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryRowsSource {
    /// An empty source.
    pub fn new() -> Self {
        Self {
            rows: ListModel::new(),
            version: Signal::new(0),
        }
    }

    /// Replaces the rows with those of `structure` and bumps the version, so
    /// the list re-reads every height.
    pub fn set(&self, structure: &ListStructure) {
        self.rows.replace_all(structure.rows.clone());
        self.version.set(self.version.get().wrapping_add(1));
    }

    /// A counter that moves with every [`set`](Self::set).
    pub fn version(&self) -> Signal<u64> {
        self.version.clone()
    }

    /// The height of the row at `index` (0 past the end).
    pub fn height(&self, index: usize) -> f32 {
        self.rows.with_item(index, |row| row.height).unwrap_or(0.0)
    }

    /// The row at `index`.
    pub fn row(&self, index: usize) -> Option<ListRow> {
        self.rows.with_item(index, |row| *row)
    }
}

impl ListDataSource for LibraryRowsSource {
    type Item = ListRow;
    type Key = usize;

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn with_item<R>(&self, index: usize, f: impl FnOnce(&ListRow) -> R) -> Option<R> {
        self.rows.with_item(index, f)
    }

    fn key_at(&self, index: usize) -> Option<usize> {
        (index < self.rows.len()).then_some(index)
    }

    fn index_of(&self, key: &usize) -> Option<usize> {
        (*key < self.rows.len()).then_some(*key)
    }

    fn observe_changes(&self, f: impl Fn(&DataChange) + 'static) -> ObserverHandle {
        self.rows.observe_changes(f)
    }
}

/// The 10 px / 15 px list padding around a row.
pub fn padded(row: impl Widget + 'static) -> Padding {
    Padding::new(0.0, LIST_TRAILING, 0.0, LIST_LEADING).child(row)
}

/// Whether `item` is selected, as a signal a row binds.
pub fn selected_signal(vm: &LibraryViewModel, item: LibraryListItem) -> Signal<bool> {
    vm.selection().map(move |s| s.selected.contains(&item))
}

/// Whether the playing track is one of `songs`, as a signal a row binds.
pub fn playing_signal(vm: &LibraryViewModel, songs: &[Arc<SongData>]) -> Signal<bool> {
    let paths: Vec<Arc<str>> = songs.iter().map(|s| s.full_file_path.clone()).collect();
    vm.playing_path()
        .map(move |path| !path.is_empty() && paths.iter().any(|p| p.as_ref() == path.as_str()))
}

/// The artist header of the structure's artist `artist`.
pub fn artist_header(
    vm: &LibraryViewModel,
    structure: &ListStructure,
    artist: usize,
) -> Option<HeaderRow> {
    let va = structure.artists.get(artist)?;
    let songs: Vec<Arc<SongData>> = structure
        .groups
        .iter()
        .find(|g| g.name == va.artist_name)
        .map(|g| g.songs.clone())
        .unwrap_or_default();
    let item = LibraryListItem::Artist(va.artist_name.clone());
    let select_vm = vm.clone();
    let toggle_vm = vm.clone();
    let name = va.artist_name.clone();
    let toggle_name = name.clone();
    Some(HeaderRow::artist(
        &va.artist_name,
        va.is_collapsed,
        va.albums_count,
        va.songs_count,
        va.duration_secs,
        ListMetrics::for_mode(structure.mode).artist_header_h,
        selected_signal(vm, item.clone()),
        playing_signal(vm, &songs),
        Rc::new(move |modifiers: Modifiers, _ctx: &mut EventContext| {
            select_vm.click(item.clone(), modifiers, Instant::now());
        }),
        Rc::new(move |_ctx: &mut EventContext| toggle_vm.toggle_artist(&toggle_name)),
    ))
}

/// The album header of a Simple or Thumbnail list.
fn album_header(
    vm: &LibraryViewModel,
    structure: &ListStructure,
    artist: usize,
    album: usize,
) -> Option<HeaderRow> {
    let va = structure.artists.get(artist)?;
    let run = va.albums.get(album)?;
    let id = album_composite_id(&va.artist_name, &run.album_hash);
    let item = LibraryListItem::Album(id.clone());
    let expanded = !structure.collapsed_albums.contains(&id);
    let select_vm = vm.clone();
    let toggle_vm = vm.clone();
    Some(HeaderRow::album(
        &run.album_name,
        expanded,
        run.songs.len(),
        run.duration_secs,
        ListMetrics::for_mode(structure.mode).album_header_h,
        selected_signal(vm, item.clone()),
        // Iced never marks a Simple or Thumbnail album header as playing.
        Signal::new(false),
        Rc::new(move |modifiers: Modifiers, _ctx: &mut EventContext| {
            select_vm.click(item.clone(), modifiers, Instant::now());
        }),
        Rc::new(move |_ctx: &mut EventContext| toggle_vm.toggle_album(&id)),
    ))
}

/// A song row of the list modes, shifted with the columns.
pub fn song_row(
    vm: &LibraryViewModel,
    song: &Arc<SongData>,
    mode: LibraryViewMode,
    list: Rc<Cell<Option<WidgetId>>>,
) -> HShift {
    let item = LibraryListItem::Song(song.id);
    let mut row = SongRow::new(song.clone(), columns_for(mode), vm.column_widths())
        .selected(selected_signal(vm, item.clone()))
        .playing(playing_signal(vm, std::slice::from_ref(song)));
    if mode == LibraryViewMode::ThumbnailList {
        row = row.thumbnail(vm.services().covers.clone());
    }
    let press_vm = vm.clone();
    let row = row.on_press(move |modifiers, ctx| {
        press_vm.click(item.clone(), modifiers, Instant::now());
        if let Some(list) = list.get() {
            ctx.request_focus(list);
        }
    });
    HShift::new(vm.h_offset(), row)
}

/// The widget of the row `row` of `structure`.
fn row_widget(
    vm: &LibraryViewModel,
    structure: &ListStructure,
    row: &ListRow,
    list: Rc<Cell<Option<WidgetId>>>,
) -> Box<dyn Widget> {
    let built: Option<Box<dyn Widget>> = match row.kind {
        ListRowKind::ArtistHeader { artist } => artist_header(vm, structure, artist)
            .map(|header| Box::new(padded(header)) as Box<dyn Widget>),
        ListRowKind::AlbumHeader { artist, album } => album_header(vm, structure, artist, album)
            .map(|header| Box::new(padded(header)) as Box<dyn Widget>),
        ListRowKind::Song {
            artist,
            album,
            song,
        } => structure
            .artists
            .get(artist)
            .and_then(|va| va.albums.get(album))
            .and_then(|run| run.songs.get(song))
            .map(|(song, _)| {
                Box::new(padded(song_row(vm, song, structure.mode, list.clone())))
                    as Box<dyn Widget>
            }),
        ListRowKind::AlbumBlock { artist, album } => {
            AlbumBlock::from_structure(vm, structure, artist, album, list)
                .map(|block| Box::new(padded(block)) as Box<dyn Widget>)
        }
    };
    built.unwrap_or_else(|| Box::new(teksilo::widgets::Spacer::new()))
}

/// A list mode's body: the rows, the sticky artist header, the empty message;
/// see the module documentation.
pub struct LibraryList {
    vm: LibraryViewModel,
    mode: LibraryViewMode,
    source: LibraryRowsSource,
    list: Rc<Cell<Option<WidgetId>>>,
    scroll_y: Rc<std::cell::RefCell<Option<Signal<f32>>>>,
    viewport: Rc<Cell<(f32, f32)>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for LibraryList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryList")
            .field("mode", &self.mode)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl LibraryList {
    /// The `mode` list of `vm` (a list mode).
    pub fn new(vm: LibraryViewModel, mode: LibraryViewMode) -> Self {
        Self {
            vm,
            mode,
            source: LibraryRowsSource::new(),
            list: Rc::new(Cell::new(None)),
            scroll_y: Rc::new(std::cell::RefCell::new(None)),
            viewport: Rc::new(Cell::new((0.0, 0.0))),
            root: None,
        }
    }

    /// The rows the list shows.
    pub fn source(&self) -> LibraryRowsSource {
        self.source.clone()
    }

    /// A handle on the `ListView`'s vertical offset, filled when the list is
    /// built.
    pub fn scroll_handle(&self) -> Rc<std::cell::RefCell<Option<Signal<f32>>>> {
        self.scroll_y.clone()
    }

    /// A handle on the `ListView`'s id, filled when the list is built.
    pub fn list_handle(&self) -> Rc<Cell<Option<WidgetId>>> {
        self.list.clone()
    }

    /// The empty list's message (`widgets.rs:2589-2600`).
    fn empty_message(shown: Signal<bool>) -> impl Widget + 'static {
        Center::new()
            .child(
                TextWidget::new(tr!(library_empty_or_loading()))
                    .style(style::inter(EMPTY_TEXT_SIZE, FontWeight::MEDIUM))
                    .color(TextRole::Secondary),
            )
            .visible_when(shown)
    }

    /// Applies a scroll request to the list.
    fn apply_scroll(
        request: &ScrollRequest,
        mode: LibraryViewMode,
        scroll_y: &Signal<f32>,
        viewport: (f32, f32),
    ) {
        match request {
            ScrollRequest::Top => scroll_y.set(0.0),
            ScrollRequest::Reveal {
                item,
                y,
                height,
                force_top,
            } => {
                let (_, viewport_h) = viewport;
                let known = (viewport_h > 0.0).then_some((scroll_y.get(), viewport_h));
                if let Some(offset) =
                    scroll_offset_to_reveal(*y, *height, Some(item), mode, *force_top, known)
                {
                    scroll_y.set(offset);
                }
            }
            ScrollRequest::GridTile(_) | ScrollRequest::GridSong { .. } => {}
        }
    }
}

impl Widget for LibraryList {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        let mode = self.mode;
        self.source.set(&vm.structure().get());
        {
            let source = self.source.clone();
            ctx.effect(&vm.structure(), move |structure| source.set(structure));
        }
        let heights = self.source.clone();
        let rows_vm = vm.clone();
        let rows_list = self.list.clone();
        let list = ListView::from_source(self.source.clone(), move |_index, row, _selected| {
            let structure = rows_vm.structure().get();
            row_widget(&rows_vm, &structure, row, rows_list.clone())
        })
        .item_height_fn(move |index| heights.height(index))
        .scroll_bar_style(ScrollBarMode::Overlay)
        .smooth_scrolling(false);
        let scroll_y = list.scroll_y_signal().clone();
        *self.scroll_y.borrow_mut() = Some(scroll_y.clone());
        let list = ctx.add(list.access_label(tr!(library_list())));
        self.list.set(Some(list));

        {
            let viewport = self.viewport.clone();
            let scroll_y = scroll_y.clone();
            ctx.effect(&vm.scroll_request(), move |request| {
                if let Some((_, request)) = request {
                    Self::apply_scroll(request, mode, &scroll_y, viewport.get());
                }
            });
        }

        let empty = vm.structure().map(|s| s.groups.is_empty());
        let sticky = StickyHeader::new(vm.clone(), mode, scroll_y.clone());
        let marquee = MarqueeController::new(vm.clone(), scroll_y.clone(), self.viewport.clone());
        let pointer_marquee = marquee.clone();
        let focus_list = self.list.clone();
        let cancel_marquee = marquee.clone();
        let deselect_vm = vm.clone();
        let wheel_vm = vm.clone();
        let wheel_viewport = self.viewport.clone();
        let root = ctx.add(
            ZStack::new()
                .alignment(teksilo::tokens::Alignment::TOP_LEADING)
                .child(list)
                .child(sticky)
                .child(Marquee::new(marquee))
                .child(Self::empty_message(empty))
                .on_tap(move |_tap, _ctx| deselect_vm.deselect())
                .on_pointer_event(move |event, ctx| {
                    // A press anywhere in the list gives it the keyboard, as
                    // Iced focuses the library on a click (`app.rs:3037`).
                    if let WidgetEvent::PointerDown { .. } = event
                        && let Some(list) = focus_list.get()
                    {
                        ctx.request_focus(list);
                    }
                    pointer_marquee.on_pointer(event, ctx)
                })
                .on_pointer_cancel(move |_, _, _| cancel_marquee.cancel())
                .on_scroll(move |event, _ctx| match event {
                    WidgetEvent::Scroll { delta, .. } => {
                        let dx = match delta {
                            ScrollDelta::Lines { x, .. } => x * SCROLL_LINE_HEIGHT,
                            ScrollDelta::Pixels { x, .. } => *x,
                        };
                        if dx == 0.0 {
                            return EventResponse::Ignored;
                        }
                        wheel_vm.scroll_horizontally(-dx, wheel_viewport.get().0);
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                }),
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
        self.viewport.set((bounds.width, bounds.height));
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
