//! [`LibraryViewModel`]: the state and the actions of the library panel.
//!
//! Port of the Iced `LibraryManager` (`src/gui/library.rs`) and of the library
//! handlers of `AudoxidyApp::update` (`src/gui/app.rs`): `ChangeLibraryViewMode`
//! and `ToggleLibraryViewDropdown`, `LibrarySortChanged`, the column resize,
//! `LibrarySearchQueryChanged`, `ToggleAlbumExpansion`, `ToggleArtistExpansion`,
//! `SelectAlbum`, `SelectSong`, `SelectArtistHeader`, `LibraryDeselect`,
//! `LibraryKeyNav`, the marquee, `PlayLibrarySelection`, `LibraryDeleteSelection`,
//! `OpenFolderPicker`, `LibraryShowPlaying`, the reveal of a song by path
//! (`handle_library_reveal`), and what `InternalLibraryRefreshed` does to the
//! selection ([`LibraryViewModel::on_library_changed`]).
//!
//! # State
//!
//! - the view mode, cycled Grid → Detailed → Thumbnail → Simple;
//! - the column widths and the three-state sort, which the column header shows
//!   and changes (the header never writes them itself);
//! - the search text, which the library model queries
//!   (`LibrarySongsModel::set_query`, which drops stale answers);
//! - the collapsed artists and albums of the list modes, and the
//!   [`ListStructure`] they give;
//! - the selection: [`LibrarySelection`] for the rows of the list modes and the
//!   songs of the grid's open album, and a keyed selection of album ids for
//!   the grid tiles, which `GridView` writes and this view-model observes. In
//!   Grid mode the two are one selection, as Iced's `selected_items`: a Shift
//!   click, the band walk of the keys ([`LibraryViewModel::grid_key`], G2)
//!   and the rubber band ([`LibraryViewModel::select_grid_marquee`], G3) work
//!   on the cards and the open album's songs together, in Iced's order
//!   (`logic::library_selection::grid_items`);
//! - the grid's expanded album and its songs, which the grid shows in its
//!   detail band, under the album's row of cards, as Iced does;
//! - the figures of the status bar, by priority: the selection, then the
//!   songs the search and the filters let through, then the library totals
//!   (before anything is loaded).
//!
//! # Teksilo-only fixes
//!
//! - Quirk fix 1: Delete asks for confirmation ([`LibraryViewModel::request_delete`]).
//! - Quirk fix 2: the Play action plays the library selection only; Iced
//!   first plays the playlist's focused row (`src/gui/app.rs:3456`).
//! - Quirk fix 8: a delete that fails shows an error toast
//!   ([`LibraryViewModel::on_long_op`]).
//! - Quirk fix 12: playing the selection appends it to the active playlist;
//!   the playlist reloads its rows from the `ContentChanged` event without
//!   touching the playing track.
//! - Quirk fix 36: the column header and the rows share one horizontal offset
//!   ([`LibraryViewModel::h_offset`]).
//! - Defects D9 to D13 (quirk fixes 37 to 41): the view-model calls the
//!   `*_fixed` variants of `logic::library_selection` and
//!   `logic::library_geometry`.
//! - Defect D22 (quirk fix 42): the grid's expanded album is resolved from
//!   its parsed `(artist, hash)`, in memory as Iced does, then from the
//!   database, and a sort change only re-sorts it, so clearing the sort never
//!   empties it.
//! - Quirk fix 49: Delete takes the songs Play takes: songs picked inside a
//!   selected album or artist narrow it ([`delete_target_ids`]).
//! - Quirk fix 50: the Grid reveal finds a compilation track's album, by the
//!   artist the grid groups it under ([`LibraryViewModel::reveal_path`]).
//! - Quirk fix 51: a reveal in the Simple and Thumbnail modes opens the
//!   song's collapsed album too, as the Detailed mode does.
//! - Quirk fix 52: with nothing focused, Down selects the first row
//!   ([`LibraryViewModel::navigate`]).
//! - Quirk fix 53: the open grid album's songs match the search whatever its
//!   case ([`visible_album_songs`]).
//! - Quirk fix 54: the database fallback for the grid's album finds a
//!   compilation, by its album artist
//!   (`commands::library::album_songs_by_album_artist`).
//!
//! # For the App
//!
//! The filter column does not know the library: when its type, letter,
//! restore icon or node changes what the library shows, the App calls
//! [`LibraryViewModel::on_filter_changed`] (back to the top, the figures to
//! the filter until its songs arrive; G1, G6).
//!
//! A reveal whose song the search or the filters hide clears them all. The
//! library's search field follows [`LibraryViewModel::search`]; the filter
//! column does not listen to the library, so the App resets its highlight
//! (its sub-filter letter and its selected node) when
//! [`LibraryViewModel::reveal_cleared`] fires, as Iced clears
//! `selected_subfilter` and `selected_tree_node` (`src/gui/app.rs:6108-6110`).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, Instant};

use teksilo::core::signal::ObserverHandle;
use teksilo::core::{Key, Modifiers};
use teksilo::data::{KeyedSelectionModel, SelectionMode};
use teksilo::prelude::*;
use teksilo::widgets::{StandardButton, Toast};

use super::structure::ListStructure;
use crate::db::database::{LibrarySearchParams, SongData};
use crate::frontend::commands::{library, playlist};
use crate::frontend::events::{LongOpEvent, LongOpKind, OpId};
use crate::frontend::{Event, Latest, Origin};
use crate::logic::library_geometry::{ContentRect, LibraryViewMode};
use crate::logic::library_groups::{
    AlbumEntry, LibraryStats, UNKNOWN_ALBUM, album_composite_id, compute_stats, selection_stats,
};
use crate::logic::library_selection::{
    GridShape, GridSpot, LibraryListItem, LibraryNavDir, LibrarySelection, LibraryStructure,
    apply_bulk_toggle_fixed, artist_to_toggle_fixed, bulk_toggle_targets_fixed, grid_items,
    grid_nav_target, list_nav_target, selected_songs,
};
use crate::logic::library_sort::{SortState, sort_songs};
use crate::logic::selection_modifiers::SelectModifiers;
use crate::teksilo_ui::app::{AppServices, Request};
use crate::teksilo_ui::models::has_any_filter;
use crate::teksilo_ui::models::library_songs_model::is_reload_event;
use crate::teksilo_ui::models::load::{finish_reply, finish_tracked};
use crate::teksilo_ui::widgets::column_header::{
    self, ColumnWidths, columns_for, default_widths, width_of,
};
use crate::teksilo_ui::widgets::dialog_frame::{ConfirmResult, confirm_box};
use crate::utils::{SortColumn, get_effective_artist, song_matches_search};

/// Two clicks on the same header within this make a double click, which
/// toggles it (`src/gui/app.rs:3007`, `:3109`).
pub const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// The width a grid card takes, spacing included (`src/gui/library.rs:1504`).
pub const GRID_CARD_PITCH: f32 = 187.0;
/// The fewest grid columns (`src/gui/library.rs:1506`).
pub const GRID_MIN_COLUMNS: usize = 2;
/// The most grid columns (`src/gui/library.rs:1512`).
pub const GRID_MAX_COLUMNS: usize = 10;
/// The most grid columns in low-resource mode (`src/gui/library.rs:1510`).
pub const GRID_MAX_COLUMNS_LOW_RESOURCE: usize = 5;

/// The number of grid columns for a grid `width` px wide:
/// `clamp(floor(width / 187), 2, 10)`, at most 5 in low-resource mode
/// (`src/gui/library.rs:1503-1517`).
pub fn grid_columns(width: f32, low_resource: bool) -> usize {
    let max = if low_resource {
        GRID_MAX_COLUMNS_LOW_RESOURCE
    } else {
        GRID_MAX_COLUMNS
    };
    let fitting = if width.is_finite() && width > 0.0 {
        // Saturating float-to-int cast: absurd widths clamp.
        (width / GRID_CARD_PITCH).floor() as usize
    } else {
        0
    };
    fitting.clamp(GRID_MIN_COLUMNS, max)
}

/// The view mode the view-mode button switches to
/// (`ToggleLibraryViewDropdown`, `src/gui/app.rs:3433-3448`).
pub fn next_view_mode(mode: LibraryViewMode) -> LibraryViewMode {
    match mode {
        LibraryViewMode::Grid => LibraryViewMode::DetailedList,
        LibraryViewMode::DetailedList => LibraryViewMode::ThumbnailList,
        LibraryViewMode::ThumbnailList => LibraryViewMode::SimpleList,
        LibraryViewMode::SimpleList => LibraryViewMode::Grid,
    }
}

/// The selection gesture of `modifiers`: Ctrl (Cmd on macOS) toggles, Shift
/// extends (`src/gui/library.rs:49`).
pub fn select_modifiers(modifiers: Modifiers) -> SelectModifiers {
    SelectModifiers {
        toggle: modifiers.command() || modifiers.ctrl(),
        range: modifiers.shift(),
    }
}

/// The width of the column header's content in `mode`: the leading padding,
/// every column, and the 5 px after the last one. The rows have the same
/// columns, so this is how far the shared horizontal offset can go.
pub fn content_width(mode: LibraryViewMode, widths: &ColumnWidths) -> f32 {
    let columns: f32 = columns_for(mode)
        .into_iter()
        .map(|column| f32::from(width_of(widths, column)))
        .sum();
    column_header::leading_padding(mode) + columns + column_header::ROW_TRAILING
}

/// The songs a delete of `selected` removes: the songs Play takes
/// ([`selected_songs`] over `songs`, the songs the search and the filters let
/// through), so songs picked inside a selected artist or album narrow it
/// (quirk fix 49), and a selected song the search or a filter hides stays.
/// Iced's `LibraryDeleteSelection` (`src/gui/app.rs:3717-3790`) also looks
/// only among `filtered_songs` (the loop at `:3759`), but adds every song of
/// a selected album or artist even when songs are picked inside it, so Delete
/// took more than Play. The ids come sorted, without duplicates.
pub fn delete_target_ids(
    selected: &HashSet<LibraryListItem>,
    structure: &impl LibraryStructure,
    songs: &[Arc<SongData>],
) -> Vec<i64> {
    let mut ids: Vec<i64> = selected_songs(selected, structure, songs)
        .iter()
        .map(|s| s.id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The delete confirmation's message: how many songs go, `count`.
pub fn delete_text(count: usize) -> LocalizedString {
    tr!(library_delete_text(
        count = i64::try_from(count).unwrap_or(i64::MAX),
        count_text = count.to_string()
    ))
}

/// The songs of the grid's album `composite_id` among `songs`, as Iced's
/// `ToggleAlbumExpansion` finds them in memory (`src/gui/app.rs:2928-2948`):
/// the album is looked up in `albums` by its composite id (or a bare hash),
/// and its songs are those whose album tag is its title and whose effective
/// artist is its artist. `None` when the album is not in `albums`.
pub fn album_songs_in_memory(
    composite_id: &str,
    songs: &[Arc<SongData>],
    albums: &[AlbumEntry],
) -> Option<Vec<Arc<SongData>>> {
    let entry = albums
        .iter()
        .find(|a| album_composite_id(&a.artist, &a.id) == composite_id || a.id == composite_id)?;
    Some(
        songs
            .iter()
            .filter(|s| {
                s.album.as_deref().unwrap_or(UNKNOWN_ALBUM) == entry.title
                    && get_effective_artist(s) == entry.artist
            })
            .cloned()
            .collect(),
    )
}

/// The songs of the expanded album that the grid shows under a search:
/// those `song_matches_search` keeps for the trimmed search text
/// (`src/gui/library.rs:481-489`, `:547-551`), lowercased (quirk fix 53).
/// `song_matches_search` compares lowercased tags with a lowercase query;
/// the Iced UI passes the text as typed, so a capital letter in the search
/// hides every song of the open album while the rest of the search, which
/// the database runs, ignores case.
pub fn visible_album_songs(songs: &[Arc<SongData>], search: &str) -> Vec<Arc<SongData>> {
    let query = search.trim().to_lowercase();
    if query.is_empty() {
        return songs.to_vec();
    }
    songs
        .iter()
        .filter(|s| song_matches_search(s, &query))
        .cloned()
        .collect()
}

/// Where a list view should scroll.
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollRequest {
    /// Back to the top (a new search, another view mode).
    Top,
    /// Reveal the content-space span `y .. y + height` of `item`: scrolled to
    /// the top when `force_top`, else only as far as needed
    /// (`logic::library_geometry::scroll_offset_to_reveal`).
    Reveal {
        /// The item revealed (it decides the sticky-header margin).
        item: LibraryListItem,
        /// Top of its span, in content space.
        y: f32,
        /// Height of its span.
        height: f32,
        /// Whether to put it at the top whatever the viewport shows.
        force_top: bool,
    },
    /// Reveal the grid tile at this index.
    GridTile(usize),
    /// Reveal the song row `row` of the band under the tile `tile` (the grid
    /// knows where the band is): at the top when `force_top` (a reveal by
    /// path), else only as far as needed (the keys).
    GridSong {
        /// The open tile.
        tile: usize,
        /// The row among the band's songs.
        row: usize,
        /// Whether to put it at the top whatever the viewport shows.
        force_top: bool,
    },
}

/// What a grid key did ([`LibraryViewModel::grid_key`]), so that the view
/// moves the keyboard focus with the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKey {
    /// Not the view-model's: the grid's own navigation takes it (a step
    /// between cards, with the grid's focus ring and scroll).
    Unused,
    /// Done; the keyboard stays where it is.
    Used,
    /// The selection went from a card into the open album's songs: the
    /// keyboard goes into the band.
    EnteredBand,
    /// The selection went from the first song back to the open album's
    /// card: the keyboard goes back to the grid, on that card.
    LeftBand,
}

struct Inner {
    services: AppServices,
    low_resource: bool,
    view_mode: Signal<LibraryViewMode>,
    column_widths: Signal<ColumnWidths>,
    sort: Signal<SortState>,
    search: Signal<String>,
    collapsed_artists: RefCell<HashSet<String>>,
    collapsed_albums: RefCell<HashSet<String>>,
    structure: Signal<Rc<ListStructure>>,
    selection: Signal<LibrarySelection>,
    grid_selection: KeyedSelectionModel<String>,
    /// The grid's selected album ids as last seen, and the album the last
    /// grid gesture was on (Iced's `selected_album`).
    grid_seen: RefCell<HashSet<String>>,
    grid_anchor: RefCell<Option<String>>,
    /// Set while the view-model itself writes the grid selection.
    writing_grid: Cell<bool>,
    /// The card a press in the grid was on, with its modifiers, until the
    /// grid's selection answers it ([`LibraryViewModel::grid_press`]).
    grid_press: RefCell<Option<(String, Modifiers)>>,
    /// Set by a key the grid has: the grid's next selection change is its
    /// keyboard's.
    grid_key: Cell<bool>,
    /// The grid's cards to a row, as the grid lays them out (Iced's
    /// `albums_per_row`).
    grid_per_row: Cell<usize>,
    expanded_album: Signal<Option<String>>,
    expanded_songs: Signal<Rc<Vec<Arc<SongData>>>>,
    detail_latest: Latest,
    stats: Signal<LibraryStats>,
    h_offset: Signal<f32>,
    /// The width the column header is laid out at, the viewport the shared
    /// offset scrolls in (0 before the first layout).
    h_viewport: Cell<f32>,
    scroll: Request<ScrollRequest>,
    last_artist_click: RefCell<Option<(String, Instant)>>,
    last_album_click: RefCell<Option<(String, Instant)>>,
    pending_reveal: RefCell<Option<String>>,
    reveal_cleared: Request<LibrarySearchParams>,
    /// A reveal asks the panel to give the library the keyboard.
    focus_request: Request<()>,
    /// Set by a library change (a scan, a delete): the next snapshot is a
    /// refresh, which clears the selection (G4).
    refresh_pending: Cell<bool>,
    /// Set by a filter change while its songs are on their way: the status
    /// bar shows the filter's figures, not the selection's (G6).
    filter_figures: Cell<bool>,
    deletes: RefCell<HashSet<OpId>>,
    observers: RefCell<Vec<ObserverHandle>>,
}

/// The library panel's view-model; see the module documentation. A `Clone`
/// handle: clones share one state. UI thread only.
#[derive(Clone)]
pub struct LibraryViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for LibraryViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryViewModel")
            .field("view_mode", &self.inner.view_mode.get())
            .field("sort", &self.inner.sort.get())
            .field("expanded_album", &self.inner.expanded_album.get())
            .finish_non_exhaustive()
    }
}

/// Runs `f` on the view-model behind `weak`, if it still lives.
fn with_vm(weak: &Weak<Inner>, f: impl FnOnce(&LibraryViewModel)) {
    if let Some(inner) = weak.upgrade() {
        f(&LibraryViewModel { inner });
    }
}

impl LibraryViewModel {
    /// The view-model over `services`, in Grid mode with the Iced default
    /// column widths and no sort. Low-resource mode is read from the process
    /// flag `startup` sets (`utils::is_low_resource`).
    pub fn new(services: &AppServices) -> Self {
        Self::with_low_resource(services, crate::utils::is_low_resource())
    }

    /// [`new`](Self::new) with an explicit low-resource mode (tests; the flag
    /// is process-wide).
    pub fn with_low_resource(services: &AppServices, low_resource: bool) -> Self {
        let inner = Rc::new(Inner {
            services: services.clone(),
            low_resource,
            view_mode: Signal::new(LibraryViewMode::Grid),
            column_widths: Signal::new(default_widths()),
            sort: Signal::new(SortState::default()),
            search: Signal::new(String::new()),
            collapsed_artists: RefCell::new(HashSet::new()),
            collapsed_albums: RefCell::new(HashSet::new()),
            structure: Signal::new(Rc::new(ListStructure::empty(LibraryViewMode::Grid))),
            selection: Signal::new(LibrarySelection::default()),
            grid_selection: KeyedSelectionModel::new(SelectionMode::Multi),
            grid_seen: RefCell::new(HashSet::new()),
            grid_anchor: RefCell::new(None),
            writing_grid: Cell::new(false),
            grid_press: RefCell::new(None),
            grid_key: Cell::new(false),
            grid_per_row: Cell::new(GRID_MIN_COLUMNS),
            expanded_album: Signal::new(None),
            expanded_songs: Signal::new(Rc::new(Vec::new())),
            detail_latest: Latest::new(),
            stats: Signal::new(LibraryStats::default()),
            h_offset: Signal::new(0.0),
            h_viewport: Cell::new(0.0),
            scroll: Request::default(),
            last_artist_click: RefCell::new(None),
            last_album_click: RefCell::new(None),
            pending_reveal: RefCell::new(None),
            reveal_cleared: Request::default(),
            focus_request: Request::default(),
            refresh_pending: Cell::new(false),
            filter_figures: Cell::new(false),
            deletes: RefCell::new(HashSet::new()),
            observers: RefCell::new(Vec::new()),
        });
        let vm = Self { inner };
        vm.observe();
        vm.rebuild();
        vm.refresh_stats();
        vm
    }

    /// Subscribes the view-model to what it follows for its whole life.
    fn observe(&self) {
        let inner = &self.inner;
        let mut handles = Vec::new();
        let weak = Rc::downgrade(inner);
        handles.push(inner.services.library_songs.version().observe({
            let weak = weak.clone();
            move |_| with_vm(&weak, LibraryViewModel::on_snapshot)
        }));
        handles.push(inner.sort.observe({
            let weak = weak.clone();
            move |_| with_vm(&weak, LibraryViewModel::on_sort)
        }));
        handles.push(inner.search.observe({
            let weak = weak.clone();
            move |text: &String| {
                let text = text.clone();
                with_vm(&weak, move |vm| vm.on_search(&text));
            }
        }));
        handles.push(inner.grid_selection.selection_signal().observe({
            let weak = weak.clone();
            move |keys: &HashSet<String>| {
                let keys = keys.clone();
                with_vm(&weak, move |vm| vm.on_grid_selection(&keys));
            }
        }));
        handles.push(inner.selection.observe({
            let weak = weak.clone();
            move |_| with_vm(&weak, LibraryViewModel::on_selection_changed)
        }));
        handles.push(inner.services.library_songs.loading().observe({
            let weak = weak.clone();
            move |loading: &bool| {
                if !*loading {
                    with_vm(&weak, LibraryViewModel::on_answer);
                }
            }
        }));
        handles.push(inner.services.library_stats.stats().observe({
            let weak = weak.clone();
            move |_| with_vm(&weak, LibraryViewModel::refresh_stats)
        }));
        handles.push(inner.services.links.reveal_in_library.latest().observe({
            let weak = weak.clone();
            move |request: &Option<(u64, String)>| {
                if let Some((_, path)) = request.clone() {
                    with_vm(&weak, move |vm| vm.reveal_path(&path));
                }
            }
        }));
        *inner.observers.borrow_mut() = handles;
    }

    // -----------------------------------------------------------------
    // Signals the views bind
    // -----------------------------------------------------------------

    /// The services this view-model was built from.
    pub fn services(&self) -> &AppServices {
        &self.inner.services
    }

    /// Whether low-resource mode caps the grid at five columns.
    pub fn low_resource(&self) -> bool {
        self.inner.low_resource
    }

    /// The view mode.
    pub fn view_mode(&self) -> Signal<LibraryViewMode> {
        self.inner.view_mode.clone()
    }

    /// The column widths the column header and the rows use.
    pub fn column_widths(&self) -> Signal<ColumnWidths> {
        self.inner.column_widths.clone()
    }

    /// The sort the column header shows.
    pub fn sort(&self) -> Signal<SortState> {
        self.inner.sort.clone()
    }

    /// The search text (the search field edits it).
    pub fn search(&self) -> Signal<String> {
        self.inner.search.clone()
    }

    /// What the list modes show.
    pub fn structure(&self) -> Signal<Rc<ListStructure>> {
        self.inner.structure.clone()
    }

    /// The selection of the list rows and of the grid band's songs.
    pub fn selection(&self) -> Signal<LibrarySelection> {
        self.inner.selection.clone()
    }

    /// The grid tiles' selection, by album id: the `GridView` writes it.
    pub fn grid_selection(&self) -> KeyedSelectionModel<String> {
        self.inner.grid_selection.clone()
    }

    /// The grid's expanded album (its composite id).
    pub fn expanded_album(&self) -> Signal<Option<String>> {
        self.inner.expanded_album.clone()
    }

    /// Every song of the expanded album, sorted (the grid's band shows those
    /// [`visible_album_songs`] keeps).
    pub fn expanded_songs(&self) -> Signal<Rc<Vec<Arc<SongData>>>> {
        self.inner.expanded_songs.clone()
    }

    /// The figures of the status bar.
    pub fn stats(&self) -> Signal<LibraryStats> {
        self.inner.stats.clone()
    }

    /// The horizontal offset the column header and the rows share (quirk
    /// fix 36).
    pub fn h_offset(&self) -> Signal<f32> {
        self.inner.h_offset.clone()
    }

    /// Scroll requests for the views.
    pub fn scroll_request(&self) -> Signal<Option<(u64, ScrollRequest)>> {
        self.inner.scroll.latest()
    }

    /// What a reveal cleared to show its song: the search and the filters as
    /// they were (`query` holds the search text), sent before the library
    /// queries the unfiltered songs. The App resets the filter column's
    /// highlight from it; see the module documentation.
    pub fn reveal_cleared(&self) -> Signal<Option<(u64, LibrarySearchParams)>> {
        self.inner.reveal_cleared.latest()
    }

    /// The reveal's requests for the keyboard: the panel moves it to the list
    /// or the grid on each, as Iced's reveal sets `AppFocus::Library`
    /// (`src/gui/app.rs:6086`), so that Enter, Delete and the arrows act on
    /// the song revealed.
    pub fn focus_request(&self) -> Signal<Option<(u64, ())>> {
        self.inner.focus_request.latest()
    }

    /// The playing track's path (the playing markers).
    pub fn playing_path(&self) -> Signal<String> {
        self.inner.services.now_playing.path()
    }

    /// Whether the selection holds `item` (a row's highlight).
    pub fn is_selected(&self, item: &LibraryListItem) -> bool {
        self.inner.selection.get().selected.contains(item)
    }

    // -----------------------------------------------------------------
    // View mode, columns, sort, search
    // -----------------------------------------------------------------

    /// Switches to `mode` (`ChangeLibraryViewMode`, `src/gui/app.rs:3365`),
    /// keeping the selection (the grid's selected cards are the lists'
    /// selected albums): the covers that failed to decode are retried when
    /// next shown (G5:
    /// `clear_all_cover_cache`, `:3367`; [`CoverStore::forget_failed`]), the
    /// grid's expanded album closes, the view scrolls back to the top and the
    /// horizontal offset returns to 0.
    ///
    /// [`CoverStore::forget_failed`]: crate::teksilo_ui::shared::cover_store::CoverStore::forget_failed
    pub fn set_view_mode(&self, mode: LibraryViewMode) {
        let inner = &self.inner;
        // One selection across the modes, as Iced's `selected_items` (11c):
        // the grid's cards become the lists' albums and back.
        let items = self.selected_items();
        self.forget_grid_gesture();
        // Before the views of the new mode ask for their covers.
        inner.services.covers.forget_failed();
        inner.view_mode.set(mode);
        let (albums, rest): (HashSet<LibraryListItem>, HashSet<LibraryListItem>) =
            items.into_iter().partition(|item| {
                mode == LibraryViewMode::Grid && matches!(item, LibraryListItem::Album(_))
            });
        let mut selection = inner.selection.get();
        selection.selected = rest;
        inner.selection.set_if_changed(selection);
        let albums: Vec<String> = albums
            .into_iter()
            .filter_map(|item| match item {
                LibraryListItem::Album(id) => Some(id),
                _ => None,
            })
            .collect();
        self.write_grid_selection(|keyed| {
            if albums.is_empty() {
                keyed.clear();
            } else {
                keyed.select_keys(albums, false);
            }
        });
        inner.expanded_album.set(None);
        inner.expanded_songs.set(Rc::new(Vec::new()));
        inner.detail_latest.invalidate();
        inner.h_offset.set(0.0);
        self.rebuild();
        inner.scroll.send(ScrollRequest::Top);
    }

    /// The view-mode button: Grid → Detailed → Thumbnail → Simple → Grid.
    pub fn cycle_view_mode(&self) {
        self.set_view_mode(next_view_mode(self.inner.view_mode.get()));
    }

    /// Stores the width of `column` (a drag or a keyboard step of its grip,
    /// at least 30 px; the header computes it).
    pub fn set_column_width(&self, column: SortColumn, width: u16) {
        let mut widths = self.inner.column_widths.get();
        if widths.get(&column) == Some(&width) {
            return;
        }
        widths.insert(column, width);
        self.inner.column_widths.set(widths);
        // A narrower column can leave the offset past the content's end.
        self.clamp_h_offset(self.inner.h_viewport.get());
    }

    /// Applies the sort a header click produced (`LibrarySortChanged`; the
    /// header computes the three-state cycle).
    pub fn set_sort(&self, sort: SortState) {
        self.inner.sort.set_if_changed(sort);
    }

    fn on_sort(&self) {
        let sort = self.inner.sort.get();
        self.inner
            .services
            .albums
            .set_sort(sort.column, sort.ascending);
        let songs = self.inner.expanded_songs.get();
        if !songs.is_empty() {
            let mut songs = (*songs).clone();
            sort_songs(&mut songs, sort.column, sort.ascending);
            self.inner.expanded_songs.set(Rc::new(songs));
        }
        self.rebuild();
    }

    fn on_search(&self, text: &str) {
        // What the grid's selection does next is the search's, not a
        // gesture's.
        self.forget_grid_gesture();
        let services = &self.inner.services;
        let current = services.library_songs.params().query.unwrap_or_default();
        if current != text {
            services.library_songs.set_query(text);
        }
        self.inner.scroll.send(ScrollRequest::Top);
    }

    /// Moves the shared horizontal offset by `dx`, within the content that
    /// overflows a viewport `viewport_width` wide.
    pub fn scroll_horizontally(&self, dx: f32, viewport_width: f32) {
        let max = (content_width(self.inner.view_mode.get(), &self.inner.column_widths.get())
            - viewport_width)
            .max(0.0);
        let next = (self.inner.h_offset.get() + dx).clamp(0.0, max);
        self.inner.h_offset.set_if_changed(next);
    }

    /// Clamps the shared offset to the content that overflows a viewport
    /// `viewport_width` wide.
    pub fn clamp_h_offset(&self, viewport_width: f32) {
        self.scroll_horizontally(0.0, viewport_width);
    }

    /// The column header was laid out `width` px wide (a resize of the
    /// window, a mode change): the offset is clamped to what overflows it
    /// now, so a wider window shows the leading columns again (quirk fix 36).
    /// A column resize clamps against the same width
    /// ([`set_column_width`](Self::set_column_width)). Called from layout;
    /// it writes the offset only when it changes.
    pub fn set_h_viewport(&self, width: f32) {
        if !width.is_finite() || width <= 0.0 {
            return;
        }
        self.inner.h_viewport.set(width);
        self.clamp_h_offset(width);
    }

    // -----------------------------------------------------------------
    // Derived data
    // -----------------------------------------------------------------

    /// Rebuilds the list structure from the current snapshot and state.
    pub fn rebuild(&self) {
        let inner = &self.inner;
        let snapshot = inner.services.library_songs.snapshot().get();
        let structure = ListStructure::build(
            &snapshot.songs,
            &snapshot.albums,
            inner.sort.get(),
            inner.view_mode.get(),
            &inner.collapsed_artists.borrow(),
            &inner.collapsed_albums.borrow(),
        );
        inner.structure.set(Rc::new(structure));
        self.refresh_stats();
    }

    fn on_snapshot(&self) {
        if self.inner.refresh_pending.replace(false) {
            self.clear_selected();
        }
        self.rebuild();
        self.refresh_expanded_from_snapshot();
        let pending = self.inner.pending_reveal.borrow_mut().take();
        if let Some(path) = pending {
            self.reveal_found(&path);
        }
    }

    /// The selected items of both selections: the list selection, plus the
    /// grid's albums in Grid mode.
    pub fn selected_items(&self) -> HashSet<LibraryListItem> {
        let mut items = self.inner.selection.get().selected;
        if self.inner.view_mode.get() == LibraryViewMode::Grid {
            items.extend(
                self.inner
                    .grid_selection
                    .selected_keys()
                    .into_iter()
                    .map(LibraryListItem::Album),
            );
        }
        items
    }

    /// The songs the selection stands for, in library order
    /// (`get_selected_songs`, `src/gui/library.rs:991`).
    pub fn selected_songs(&self) -> Vec<Arc<SongData>> {
        let structure = self.inner.structure.get();
        let snapshot = self.inner.services.library_songs.snapshot().get();
        selected_songs(&self.selected_items(), &structure.data(), &snapshot.songs)
    }

    /// Recomputes the status bar's figures: the selection's, unless a filter
    /// change gave them to the filter until its songs arrive (G6), then the
    /// songs', then the totals.
    pub fn refresh_stats(&self) {
        let inner = &self.inner;
        let selection = if inner.filter_figures.get() {
            None
        } else {
            selection_stats(&self.selected_songs())
        };
        let stats = selection.unwrap_or_else(|| {
            if inner.services.library_songs.version().get() > 0 {
                compute_stats(&inner.services.library_songs.snapshot().get().songs)
            } else {
                inner.services.library_stats.stats().get()
            }
        });
        inner.stats.set_if_changed(stats);
    }

    /// The selection changed: its figures show again (Iced's
    /// `update_selection_stats`, which every selection gesture calls).
    fn on_selection_changed(&self) {
        self.inner.filter_figures.set(false);
        self.refresh_stats();
    }

    /// The library model answered (or failed): the selection's figures,
    /// over the new songs, show again, as Iced's `LibraryFilteredLoaded`
    /// recomputes them (`src/gui/app.rs:3909-3912`).
    fn on_answer(&self) {
        if self.inner.filter_figures.replace(false) {
            self.refresh_stats();
        }
    }

    /// The filter column changed what the library shows: a type, a letter,
    /// the restore icon or a node was chosen (`ChangeGeneralFilter`,
    /// `SelectSubfilter`, `SelectTreeNode`, `src/gui/app.rs:4245-4354`). The
    /// App calls it on `FiltersViewModel::library_reset`.
    ///
    /// - G1: the view goes back to its top (`:4276-4282`, `:4313-4316`,
    ///   `:4349-4354`).
    /// - G6: Iced sets `selection_stats` to `None` "so that `filter_stats`
    ///   has priority" (`:4265-4269`, `:4296-4299`, `:4330-4334`) and keeps
    ///   `selected_items`; the search it starts answers with
    ///   `LibraryFilteredLoaded`, which recomputes the selection's figures
    ///   over the new songs (`:3909-3912`). So while the filter's songs are on
    ///   their way the bar shows the filter's figures (those of the songs
    ///   shown), and once they arrive the selection's again, or the new
    ///   songs' when none of the selection is left. A letter that leaves the
    ///   library's filters as they were queries nothing here (Iced queries
    ///   the same songs again), so the selection's figures stay.
    pub fn on_filter_changed(&self) {
        let inner = &self.inner;
        self.forget_grid_gesture();
        inner.scroll.send(ScrollRequest::Top);
        if inner.services.library_songs.loading().get() {
            inner.filter_figures.set(true);
            self.refresh_stats();
        }
    }

    // -----------------------------------------------------------------
    // List-mode selection, clicks and collapse
    // -----------------------------------------------------------------

    fn update_selection(&self, f: impl FnOnce(&mut LibrarySelection, &ListStructure)) {
        let structure = self.inner.structure.get();
        let mut selection = self.inner.selection.get();
        f(&mut selection, &structure);
        self.inner.selection.set_if_changed(selection);
    }

    /// A click on a row of the list modes or of the grid's band
    /// (`SelectArtistHeader`, `SelectAlbum`, `SelectSong`). A second click on
    /// the same artist or album header within [`DOUBLE_CLICK`] toggles it
    /// instead.
    pub fn click(&self, item: LibraryListItem, modifiers: Modifiers, now: Instant) {
        let inner = &self.inner;
        let double = |last: &RefCell<Option<(String, Instant)>>, key: &str| {
            let hit = last.borrow().as_ref().is_some_and(|(k, at)| {
                k == key && now.saturating_duration_since(*at) < DOUBLE_CLICK
            });
            *last.borrow_mut() = if hit {
                None
            } else {
                Some((key.to_string(), now))
            };
            hit
        };
        match &item {
            LibraryListItem::Artist(name) => {
                if double(&inner.last_artist_click, name) {
                    self.toggle_artist(name);
                    return;
                }
            }
            LibraryListItem::Album(id) => {
                if double(&inner.last_album_click, id) {
                    self.toggle_album(id);
                    return;
                }
            }
            LibraryListItem::Song(_) => {}
        }
        let modifiers = select_modifiers(modifiers);
        if inner.view_mode.get() == LibraryViewMode::Grid {
            // The cards and the band's songs are one selection, as Iced's
            // `selected_items`: a plain click replaces both, and a Shift click
            // ranges over both in Iced's order (`handle_click` over
            // `get_visible_items`, `src/gui/library.rs:913-922`).
            self.update_grid(|selection, items, structure| {
                selection.click(item, modifiers, || items.to_vec(), &structure.data());
            });
            return;
        }
        self.update_selection(|selection, structure| {
            selection.click(
                item,
                modifiers,
                || structure.item_rects(1.0),
                &structure.data(),
            );
        });
    }

    /// Empties the selection and leaves the focus and the pivot, as Iced's
    /// `InternalLibraryRefreshed` empties `selected_items` (`src/gui/app.rs:3944`).
    fn clear_selected(&self) {
        let mut selection = self.inner.selection.get();
        selection.selected.clear();
        self.inner.selection.set_if_changed(selection);
        self.write_grid_selection(|keyed| keyed.clear());
    }

    /// The library changed under the view-model (`LibraryEvent::Changed` or
    /// `ScanFinished`: a scan, a delete): the snapshot the library model
    /// reloads on it is a refresh, and a refresh clears the selection (G4),
    /// as Iced's `InternalLibraryRefreshed` does (`src/gui/app.rs:3921-3944`;
    /// the reply of `LibraryRefresh`, and of the scanner's dirty flag,
    /// `:1340-1365`). A search or a filter answer (Iced's
    /// `LibraryFilteredLoaded`, `:3909`) keeps it. The model reloads on the
    /// next frame and its reload supersedes the requests before it, so the
    /// next snapshot is the refresh (or, at worst, a search answer already
    /// on its way, a frame earlier).
    pub fn on_library_changed(&self) {
        self.inner.refresh_pending.set(true);
    }

    /// A press on the empty background (`LibraryDeselect`): nothing selected.
    pub fn deselect(&self) {
        self.inner
            .selection
            .set_if_changed(LibrarySelection::default());
        self.write_grid_selection(|keyed| keyed.clear());
    }

    /// Selects every item (Ctrl+A): the rows of the list modes, collapsed
    /// headers included; in Grid mode every album and the expanded album's
    /// songs (`select_all`, `src/gui/library.rs:926`).
    pub fn select_all(&self) {
        if self.inner.view_mode.get() == LibraryViewMode::Grid {
            let list = self.inner.services.albums.list();
            let ids: Vec<String> = (0..list.len())
                .filter_map(|i| list.with_item(i, |card| card.id.clone()))
                .collect();
            self.write_grid_selection(|keyed| keyed.select_keys(ids, false));
            let songs: Vec<LibraryListItem> = self
                .inner
                .expanded_songs
                .get()
                .iter()
                .map(|s| LibraryListItem::Song(s.id))
                .collect();
            self.update_selection(|selection, _| selection.select_all(songs));
        } else {
            self.update_selection(|selection, structure| selection.select_all(structure.items()));
        }
    }

    /// Applies a marquee drag over the rows: `rect` in content space, against
    /// rows `width` wide (`select_marquee`, `src/gui/library.rs:900`).
    pub fn select_marquee(&self, rect: ContentRect, width: f32, modifiers: Modifiers) {
        let modifiers = select_modifiers(modifiers);
        self.update_selection(|selection, structure| {
            selection.select_marquee(&structure.item_rects(width), rect, modifiers);
        });
    }

    /// Collapses or expands an artist header (`ToggleArtistExpansion`, then
    /// `set_artist_collapsed`, `src/gui/library.rs:359`): the header becomes
    /// the selection and the focus.
    pub fn toggle_artist(&self, name: &str) {
        {
            let mut collapsed = self.inner.collapsed_artists.borrow_mut();
            if !collapsed.remove(name) {
                collapsed.insert(name.to_string());
            }
        }
        self.select_header(LibraryListItem::Artist(name.to_string()));
        self.rebuild();
    }

    /// The album toggle (`ToggleAlbumExpansion`, `src/gui/app.rs:2891`). In
    /// the list modes it collapses or expands the album, which becomes the
    /// selection; in Grid mode it opens or closes the album under its row of
    /// cards ([`show_album`](Self::show_album)).
    pub fn toggle_album(&self, id: &str) {
        if self.inner.view_mode.get() == LibraryViewMode::Grid {
            if self.inner.expanded_album.get().as_deref() == Some(id) {
                self.show_album(None);
            } else {
                self.show_album(Some(id));
            }
            return;
        }
        {
            let mut collapsed = self.inner.collapsed_albums.borrow_mut();
            if !collapsed.remove(id) {
                collapsed.insert(id.to_string());
            }
        }
        self.select_header(LibraryListItem::Album(id.to_string()));
        self.rebuild();
    }

    /// Selects and focuses a toggled header, as `set_*_collapsed` do (the
    /// pivot is left alone).
    fn select_header(&self, item: LibraryListItem) {
        let mut selection = self.inner.selection.get();
        selection.selected = std::iter::once(item.clone()).collect();
        selection.focused = Some(item);
        self.inner.selection.set_if_changed(selection);
    }

    /// Whether the artist header `name` is collapsed.
    pub fn is_artist_collapsed(&self, name: &str) -> bool {
        self.inner.collapsed_artists.borrow().contains(name)
    }

    /// Whether the album `id` is collapsed (list modes).
    pub fn is_album_collapsed(&self, id: &str) -> bool {
        self.inner.collapsed_albums.borrow().contains(id)
    }

    /// The arrow keys of the list modes (`LibraryKeyNav`, `src/gui/app.rs:3140`):
    /// Up and Down move the selection (Shift extends it), Left and Right
    /// collapse and expand the selected headers and the albums of the selected
    /// songs, all of them (D9 fixed, quirk fix 37). With nothing focused (or
    /// the focused row hidden), Up and Down select the first row (quirk fix
    /// 52): Iced starts from row 0 as if it were focused
    /// (`current_index`, `src/gui/library.rs:778-800`), so Down selected the
    /// second. Returns whether anything moved.
    pub fn navigate(&self, dir: LibraryNavDir, modifiers: Modifiers) -> bool {
        match dir {
            LibraryNavDir::Left | LibraryNavDir::Right => self.bulk_toggle(dir),
            LibraryNavDir::Up | LibraryNavDir::Down => self.step(dir, modifiers),
            LibraryNavDir::None => false,
        }
    }

    fn bulk_toggle(&self, dir: LibraryNavDir) -> bool {
        let structure = self.inner.structure.get();
        let mut selection = self.inner.selection.get();
        let targets = bulk_toggle_targets_fixed(
            &selection.selected,
            selection.focused.as_ref(),
            dir,
            &structure.data(),
            |song| structure.album_id_for_song(song),
            &structure.items(),
        );
        let applied = apply_bulk_toggle_fixed(
            &mut selection,
            &targets,
            dir,
            &mut self.inner.collapsed_artists.borrow_mut(),
            &mut self.inner.collapsed_albums.borrow_mut(),
        );
        if !applied {
            return false;
        }
        self.inner.selection.set_if_changed(selection);
        self.rebuild();
        self.reveal_focused(false);
        true
    }

    fn step(&self, dir: LibraryNavDir, modifiers: Modifiers) -> bool {
        let structure = self.inner.structure.get();
        let items = structure.item_rects(1.0);
        if items.is_empty() {
            return false;
        }
        let mut selection = self.inner.selection.get();
        let focused = selection
            .focused
            .as_ref()
            .and_then(|focused| items.iter().position(|(item, _)| item == focused));
        let (current, target) = match focused {
            Some(current) => (current, list_nav_target(current, items.len(), dir)),
            None => (0, 0),
        };
        let moved = selection.navigate(
            &items,
            current,
            target,
            select_modifiers(modifiers),
            &structure.data(),
        );
        if moved.is_none() {
            return false;
        }
        self.inner.selection.set_if_changed(selection);
        let (item, rect) = &items[target.min(items.len() - 1)];
        self.inner.scroll.send(ScrollRequest::Reveal {
            item: item.clone(),
            y: rect.y,
            height: rect.height,
            force_top: false,
        });
        true
    }

    /// Asks the list to show the focused item.
    fn reveal_focused(&self, force_top: bool) {
        let Some(item) = self.inner.selection.get().focused else {
            return;
        };
        if let Some((y, height)) = self.inner.structure.get().item_y_range(&item) {
            self.inner.scroll.send(ScrollRequest::Reveal {
                item,
                y,
                height,
                force_top,
            });
        }
    }

    // -----------------------------------------------------------------
    // Grid
    // -----------------------------------------------------------------

    /// Writes the grid selection without treating it as a user gesture.
    fn write_grid_selection(&self, f: impl FnOnce(&KeyedSelectionModel<String>)) {
        self.inner.filter_figures.set(false);
        self.inner.writing_grid.set(true);
        f(&self.inner.grid_selection);
        self.inner.writing_grid.set(false);
        *self.inner.grid_seen.borrow_mut() = self
            .inner
            .grid_selection
            .selected_keys()
            .into_iter()
            .collect();
        self.refresh_stats();
    }

    /// Selects only the tile `id`.
    fn select_tile(&self, id: &str) {
        self.write_grid_selection(|keyed| keyed.select(id.to_string()));
        *self.inner.grid_anchor.borrow_mut() = Some(id.to_string());
    }

    /// The grid selection changed. `GridView` does not say which gesture
    /// changed it, so the view tells the view-model first:
    ///
    /// - a press on a card ([`grid_press`](Self::grid_press)): the change is
    ///   replaced by Iced's `handle_click` on that card, over the grid's
    ///   items, cards and the open album's songs together
    ///   (`src/gui/library.rs:913-922`): a plain click selects the card
    ///   alone, Ctrl toggles it and keeps the rest, Shift ranges from the
    ///   pivot over the cards and the songs between (11b);
    /// - a key ([`grid_key_pressed`](Self::grid_key_pressed)), or a change
    ///   that adds cards with neither (an assistive technology's select): the
    ///   change is read from its size. One card added to the selection, or
    ///   one removed from three or more, is a toggle and keeps the band's
    ///   songs; anything else replaces the selection and drops them;
    /// - cards dropped with neither: the grid dropped cards its albums no
    ///   longer hold (a search, a filter). The songs picked in the open album
    ///   stay selected, as Iced's selection survives a search
    ///   (`src/gui/app.rs:2797`; 11d).
    fn on_grid_selection(&self, keys: &HashSet<String>) {
        if self.inner.writing_grid.get() {
            return;
        }
        let previous = std::mem::replace(&mut *self.inner.grid_seen.borrow_mut(), keys.clone());
        let press = self.inner.grid_press.borrow_mut().take();
        if let Some((card, modifiers)) = press {
            self.inner.grid_key.set(false);
            let modifiers = select_modifiers(modifiers);
            let item = LibraryListItem::Album(card.clone());
            self.update_grid_from(
                previous.into_iter().collect(),
                |selection, items, structure| {
                    selection.click(item, modifiers, || items.to_vec(), &structure.data());
                },
            );
            *self.inner.grid_anchor.borrow_mut() = Some(card);
            return;
        }
        let added: Vec<&String> = keys.difference(&previous).collect();
        let removed = previous.difference(keys).count();
        if !self.inner.grid_key.replace(false) && added.is_empty() {
            // Cards dropped with no gesture: the grid's albums changed.
            self.inner.filter_figures.set(false);
            self.refresh_stats();
            return;
        }
        if let Some(last) = added.last() {
            *self.inner.grid_anchor.borrow_mut() = Some((*last).clone());
        } else if keys.len() == 1 {
            *self.inner.grid_anchor.borrow_mut() = keys.iter().next().cloned();
        }
        let single_toggle = !previous.is_empty()
            && ((added.len() == 1 && removed == 0)
                || (added.is_empty() && removed == 1 && keys.len() > 1));
        if single_toggle {
            // A toggle makes the card the current item (Iced's
            // `focused_item`): the keys no longer walk from a band song.
            let mut selection = self.inner.selection.get();
            if matches!(selection.focused, Some(LibraryListItem::Song(_))) {
                selection.focused = None;
                self.inner.selection.set_if_changed(selection);
            }
        } else {
            self.inner
                .selection
                .set_if_changed(LibrarySelection::default());
        }
        self.inner.filter_figures.set(false);
        self.refresh_stats();
    }

    /// A primary press in the grid at `at`, in content space, with
    /// `modifiers`; `rect_of` gives each card's and band song's rectangle in
    /// the grid as laid out (as for [`select_grid_marquee`](Self::select_grid_marquee)).
    /// On a card, the grid's coming selection change is taken as Iced's click
    /// on it ([`on_grid_selection`](Self::on_grid_selection)); on a song of
    /// the band, the row handles it; on the background, nothing is selected
    /// any more, before the rubber band starts (Iced's `LibraryDeselect` on
    /// the grid's background, `src/gui/library.rs:1956`,
    /// `src/gui/app.rs:3343-3362`; 11a).
    pub fn grid_press(
        &self,
        at: (f32, f32),
        modifiers: Modifiers,
        rect_of: impl Fn(GridSpot, GridShape) -> ContentRect,
    ) {
        let cards = self.grid_cards();
        let songs = self.band_song_ids();
        let shape = self.grid_shape(&cards, &songs);
        let point = ContentRect {
            x: at.0,
            y: at.1,
            width: 0.01,
            height: 0.01,
        };
        let hit = |spot: GridSpot| rect_of(spot, shape).intersects(&point);
        self.inner.grid_key.set(false);
        if let Some(card) = (0..cards.len()).find(|i| hit(GridSpot::Card(*i))) {
            *self.inner.grid_press.borrow_mut() = Some((cards[card].clone(), modifiers));
            return;
        }
        *self.inner.grid_press.borrow_mut() = None;
        let in_band =
            shape.band_start().is_some() && (0..songs.len()).any(|row| hit(GridSpot::Song(row)));
        if !in_band {
            self.deselect();
        }
    }

    /// A key reached the grid: its next selection change is the keyboard's
    /// ([`on_grid_selection`](Self::on_grid_selection)).
    pub fn grid_key_pressed(&self) {
        *self.inner.grid_press.borrow_mut() = None;
        self.inner.grid_key.set(true);
    }

    /// Forgets the gesture the grid's next selection change was waiting
    /// for.
    fn forget_grid_gesture(&self) {
        *self.inner.grid_press.borrow_mut() = None;
        self.inner.grid_key.set(false);
    }

    /// The album the last grid gesture was on (Iced's `selected_album`).
    pub fn grid_anchor(&self) -> Option<String> {
        self.inner.grid_anchor.borrow().clone()
    }

    /// Tells the view-model how many cards a row of the grid holds (the grid
    /// sets it from its width).
    pub fn set_grid_per_row(&self, per_row: usize) {
        self.inner.grid_per_row.set(per_row.max(1));
    }

    /// The grid's cards, by composite id, in the grid's order.
    fn grid_cards(&self) -> Vec<String> {
        let list = self.inner.services.albums.list();
        (0..list.len())
            .filter_map(|i| list.with_item(i, |card| card.id.clone()))
            .collect()
    }

    /// The ids of the songs the band shows.
    fn band_song_ids(&self) -> Vec<i64> {
        visible_album_songs(&self.inner.expanded_songs.get(), &self.inner.search.get())
            .iter()
            .map(|s| s.id)
            .collect()
    }

    /// The grid's shape over `cards` and the band's `songs`.
    fn grid_shape(&self, cards: &[String], songs: &[i64]) -> GridShape {
        let open = self
            .inner
            .expanded_album
            .get()
            .and_then(|id| cards.iter().position(|card| *card == id));
        GridShape {
            cards: cards.len(),
            per_row: self.inner.grid_per_row.get(),
            open,
            songs: songs.len(),
        }
    }

    /// Applies `f` to the Grid mode's selection as Iced holds it: one
    /// selection of the cards and of the band's songs, over the grid's items
    /// in Iced's order ([`grid_items`]); the result is written back to the
    /// tiles' keyed selection (the cards) and to the rows' selection (the
    /// songs, the focus and the pivot). A pivot that is not one of the items
    /// is dropped.
    fn update_grid(
        &self,
        f: impl FnOnce(&mut LibrarySelection, &[LibraryListItem], &ListStructure),
    ) {
        self.update_grid_from(self.inner.grid_selection.selected_keys(), f);
    }

    /// [`update_grid`](Self::update_grid) from the selected cards `keys`
    /// rather than the tiles' selection as it is (a click the grid already
    /// applied its own way).
    fn update_grid_from(
        &self,
        keys: Vec<String>,
        f: impl FnOnce(&mut LibrarySelection, &[LibraryListItem], &ListStructure),
    ) {
        let cards = self.grid_cards();
        let songs = self.band_song_ids();
        let items = grid_items(&cards, &songs, self.grid_shape(&cards, &songs));
        let mut selection = self.inner.selection.get();
        selection
            .selected
            .extend(keys.into_iter().map(LibraryListItem::Album));
        if selection
            .pivot
            .as_ref()
            .is_some_and(|pivot| !items.contains(pivot))
        {
            selection.pivot = None;
        }
        f(&mut selection, &items, &self.inner.structure.get());
        let (albums, rest): (HashSet<LibraryListItem>, HashSet<LibraryListItem>) = selection
            .selected
            .into_iter()
            .partition(|item| matches!(item, LibraryListItem::Album(_)));
        let albums: Vec<String> = albums
            .into_iter()
            .filter_map(|item| match item {
                LibraryListItem::Album(id) => Some(id),
                _ => None,
            })
            .collect();
        self.write_grid_selection(|keyed| {
            if albums.is_empty() {
                keyed.clear();
            } else {
                keyed.select_keys(albums, false);
            }
        });
        self.inner.selection.set_if_changed(LibrarySelection {
            selected: rest,
            focused: selection.focused,
            pivot: selection.pivot,
        });
    }

    /// The current item of the grid's walk (Iced's `focused_item`): the
    /// focused song when it is one of the band's, else the card the last
    /// gesture was on; with the keyboard in the band (`in_band`) and no
    /// song current, the open album's card, where the grid's cursor is when
    /// it moved the keyboard there.
    fn grid_spot(&self, cards: &[String], songs: &[i64], in_band: bool) -> Option<GridSpot> {
        if let Some(LibraryListItem::Song(id)) = self.inner.selection.get().focused
            && let Some(row) = songs.iter().position(|song| *song == id)
        {
            return Some(GridSpot::Song(row));
        }
        let card = if in_band {
            self.inner.expanded_album.get()?
        } else {
            self.grid_anchor()?
        };
        cards.iter().position(|id| *id == card).map(GridSpot::Card)
    }

    /// A sweep of the grid's rubber band (G3), `rect` in content space: it
    /// selects the cards and the open album's songs it crosses, `rect_of`
    /// giving each one's rectangle in the grid as laid out. Iced sweeps the
    /// grid's items, cards and songs, on release (`select_marquee` over
    /// `get_visible_items`, `src/gui/library.rs:900-910`, from
    /// `GlobalMouseRelease`, `src/gui/app.rs:4800-4812`): it replaces the
    /// selection, adds to it with Shift, toggles with Ctrl, and leaves the
    /// focus and the pivot.
    pub fn select_grid_marquee(
        &self,
        rect: ContentRect,
        modifiers: Modifiers,
        rect_of: impl Fn(GridSpot, GridShape) -> ContentRect,
    ) {
        let cards = self.grid_cards();
        let songs = self.band_song_ids();
        let shape = self.grid_shape(&cards, &songs);
        let band = if shape.band_start().is_some() {
            songs.as_slice()
        } else {
            &[]
        };
        let rects: Vec<(LibraryListItem, ContentRect)> = cards
            .iter()
            .enumerate()
            .map(|(i, id)| {
                (
                    LibraryListItem::Album(id.clone()),
                    rect_of(GridSpot::Card(i), shape),
                )
            })
            .chain(band.iter().enumerate().map(|(row, id)| {
                (
                    LibraryListItem::Song(*id),
                    rect_of(GridSpot::Song(row), shape),
                )
            }))
            .collect();
        let modifiers = select_modifiers(modifiers);
        self.update_grid(|selection, _, _| selection.select_marquee(&rects, rect, modifiers));
    }

    /// The grid's arrow keys (`LibraryKeyNav` in Grid mode,
    /// `src/gui/app.rs:3170-3213`, then `handle_key_nav`,
    /// `src/gui/library.rs:760-880`), `in_band` when the keyboard is in the
    /// open album's band:
    ///
    /// - Alt+↑ closes the selected album and Alt+↓ opens it; ↑ alone on the
    ///   open album's card closes it;
    /// - the walk of [`grid_nav_target`] over the cards and the band's songs:
    ///   ↓ from a card of the open album's row enters its first song, ↓ and ↑
    ///   move song by song (↓ stops at the last), ↑ from the first song goes
    ///   back to the open album's card, ← and → stay on a song; Shift extends
    ///   the selection from the pivot over the cards and the songs, as in the
    ///   list modes (`LibrarySelection::navigate`). A song moved to is
    ///   revealed ([`ScrollRequest::GridSong`]);
    /// - a step between two cards is left to the grid's own navigation
    ///   ([`GridKey::Unused`]), which moves its cursor and focus ring, selects
    ///   and scrolls, and steps over the band.
    pub fn grid_key(&self, key: &Key, modifiers: Modifiers, in_band: bool) -> GridKey {
        let dir = match key {
            Key::ArrowUp => LibraryNavDir::Up,
            Key::ArrowDown => LibraryNavDir::Down,
            Key::ArrowLeft => LibraryNavDir::Left,
            Key::ArrowRight => LibraryNavDir::Right,
            _ => return GridKey::Unused,
        };
        let (up, down) = (dir == LibraryNavDir::Up, dir == LibraryNavDir::Down);
        let expanded = self.inner.expanded_album.get();
        if (up || down)
            && modifiers.alt()
            && !modifiers.ctrl()
            && let Some(selected) = self.grid_anchor()
        {
            let is_expanded = expanded.as_deref() == Some(selected.as_str());
            if (up && is_expanded) || (down && !is_expanded) {
                self.toggle_album(&selected);
                return GridKey::Used;
            }
        }
        let cards = self.grid_cards();
        let songs = self.band_song_ids();
        let shape = self.grid_shape(&cards, &songs);
        let Some(spot) = self.grid_spot(&cards, &songs, in_band) else {
            return GridKey::Unused;
        };
        if up
            && !modifiers.shift()
            && !modifiers.alt()
            && !modifiers.ctrl()
            && let GridSpot::Card(card) = spot
            && expanded.as_deref() == Some(cards[card].as_str())
        {
            self.toggle_album(&cards[card]);
            return GridKey::Used;
        }
        let target = grid_nav_target(spot, dir, shape);
        let outcome = match (spot, target) {
            (GridSpot::Card(_), GridSpot::Card(_)) => return GridKey::Unused,
            (GridSpot::Card(_), GridSpot::Song(_)) => GridKey::EnteredBand,
            (GridSpot::Song(_), GridSpot::Song(_)) => GridKey::Used,
            (GridSpot::Song(_), GridSpot::Card(_)) => GridKey::LeftBand,
        };
        let (Some(current), Some(next), Some(open)) =
            (shape.index_of(spot), shape.index_of(target), shape.open)
        else {
            return GridKey::Unused;
        };
        let modifiers = select_modifiers(modifiers);
        self.update_grid(|selection, items, structure| {
            selection.navigate(items, current, next, modifiers, &structure.data());
        });
        // Iced's `sync_legacy_selection`: the album is the open one either
        // way (`src/gui/library.rs:961-976`).
        *self.inner.grid_anchor.borrow_mut() = Some(cards[open].clone());
        self.inner.scroll.send(match target {
            GridSpot::Song(row) => ScrollRequest::GridSong {
                tile: open,
                row,
                force_top: false,
            },
            GridSpot::Card(card) => ScrollRequest::GridTile(card),
        });
        outcome
    }

    /// Opens the album `id` in the grid (`Some`) or closes the open one
    /// (`None`), the grid arm of `ToggleAlbumExpansion`
    /// (`src/gui/app.rs:2891-2960`): the album toggled becomes the tile
    /// selection and the songs picked before are dropped. The chevron, the
    /// grid keys and the grid's own gestures on its band (a double click on a
    /// card, Space on the open card, a screen reader's Expand and Collapse)
    /// all come here. Nothing happens when `id` is already the open album.
    pub fn show_album(&self, id: Option<&str>) {
        let current = self.inner.expanded_album.get();
        if current.as_deref() == id {
            return;
        }
        let toggled = id.map(str::to_string).or(current);
        match id {
            Some(id) => self.open_detail(id),
            None => self.close_detail(),
        }
        if let Some(toggled) = toggled {
            self.select_tile(&toggled);
        }
        self.inner
            .selection
            .set_if_changed(LibrarySelection::default());
    }

    /// Opens the grid's band on `id`.
    fn open_detail(&self, id: &str) {
        let inner = &self.inner;
        inner.expanded_album.set(Some(id.to_string()));
        inner.expanded_songs.set(Rc::new(Vec::new()));
        self.load_expanded(id);
    }

    /// Closes the grid's band.
    fn close_detail(&self) {
        let inner = &self.inner;
        inner.detail_latest.invalidate();
        inner.expanded_album.set(None);
        inner.expanded_songs.set(Rc::new(Vec::new()));
    }

    /// Loads the songs of the expanded album `id` (D22 fixed): in memory
    /// first, as Iced does, else from the database by the parsed
    /// `(artist, hash)`, the artist being the album artist (quirk fix 54:
    /// Iced's `get_songs_by_album_and_artist` matches the track artist and
    /// finds none of a compilation's songs).
    fn load_expanded(&self, id: &str) {
        let inner = &self.inner;
        let snapshot = inner.services.library_songs.snapshot().get();
        let sort = inner.sort.get();
        if let Some(mut songs) = album_songs_in_memory(id, &snapshot.songs, &snapshot.albums)
            && !songs.is_empty()
        {
            sort_songs(&mut songs, sort.column, sort.ascending);
            inner.detail_latest.invalidate();
            inner.expanded_songs.set(Rc::new(songs));
            return;
        }
        let reply = inner
            .detail_latest
            .track(library::album_songs_by_album_artist(
                &inner.services.ctx,
                id,
            ));
        let weak = Rc::downgrade(inner);
        let wanted = id.to_string();
        finish_tracked(
            &inner.services.runtime,
            "the songs of the album",
            reply,
            move |songs| {
                with_vm(&weak, move |vm| {
                    let inner = &vm.inner;
                    if inner.expanded_album.get().as_deref() != Some(wanted.as_str()) {
                        return;
                    }
                    let mut songs = songs.unwrap_or_default();
                    let sort = inner.sort.get();
                    sort_songs(&mut songs, sort.column, sort.ascending);
                    inner.expanded_songs.set(Rc::new(songs));
                    vm.refresh_stats();
                });
            },
        );
    }

    /// A new snapshot: the expanded album keeps the songs the new results
    /// hold, and closes when they hold none (`refresh_expanded_album_songs`,
    /// `src/gui/library.rs:1224`).
    fn refresh_expanded_from_snapshot(&self) {
        let inner = &self.inner;
        if inner.view_mode.get() != LibraryViewMode::Grid {
            return;
        }
        let Some(id) = inner.expanded_album.get() else {
            return;
        };
        let snapshot = inner.services.library_songs.snapshot().get();
        let Some(mut songs) = album_songs_in_memory(&id, &snapshot.songs, &snapshot.albums) else {
            return;
        };
        if songs.is_empty() {
            self.close_detail();
            return;
        }
        let sort = inner.sort.get();
        sort_songs(&mut songs, sort.column, sort.ascending);
        inner.detail_latest.invalidate();
        inner.expanded_songs.set(Rc::new(songs));
    }

    // -----------------------------------------------------------------
    // Play, delete, add a folder
    // -----------------------------------------------------------------

    /// Plays the library selection: appends its songs to the active playlist
    /// (`PlayLibrarySelection` then `PlayAlbum`, `src/gui/app.rs:3486-3489`,
    /// `:1931`). Iced first plays the playlist's focused row; here the Play
    /// action and Enter act on the library selection only (quirk fix 2). The
    /// playlist reloads its rows on the `ContentChanged` event and leaves the
    /// playing track alone (quirk fix 12). Returns whether songs were sent.
    pub fn play_selection(&self) -> bool {
        let services = &self.inner.services;
        let ids: Vec<i64> = self.selected_songs().iter().map(|s| s.id).collect();
        let Some(playlist_id) = services.active_playlist.id().get() else {
            return false;
        };
        if ids.is_empty() {
            return false;
        }
        let reply = playlist::add_songs(&services.ctx, playlist_id, ids);
        finish_reply(
            &services.runtime,
            "the songs added to the playlist",
            reply,
            |_| {},
        );
        true
    }

    /// The songs a delete would remove now.
    pub fn delete_targets(&self) -> Vec<i64> {
        let snapshot = self.inner.services.library_songs.snapshot().get();
        delete_target_ids(
            &self.selected_items(),
            &self.inner.structure.get().data(),
            &snapshot.songs,
        )
    }

    /// Delete (quirk fix 1): asks for confirmation with a box naming how many
    /// songs go ([`delete_targets`](Self::delete_targets), quirk fix 49); only
    /// its "Eliminar" deletes, and Enter does nothing in it
    /// ([`ConfirmBox`](crate::teksilo_ui::widgets::dialog_frame::ConfirmBox)).
    /// Nothing happens when nothing is selected, as in Iced.
    pub fn request_delete(&self, ctx: &mut EventContext) {
        let ids = self.delete_targets();
        if ids.is_empty() {
            return;
        }
        let vm = self.clone();
        confirm_box(
            tr!(library_delete_title()),
            delete_text(ids.len()),
            tr!(library_delete_confirm()),
        )
        .on_result(move |result: ConfirmResult, _ctx| {
            if result.button == StandardButton::Ok && !result.was_dismissed() {
                vm.delete_songs(ids.clone());
            }
        })
        .present(ctx);
    }

    /// Deletes `ids` from the library as a long operation
    /// (`commands::library::delete_songs`). Its outcome arrives through
    /// [`on_long_op`](Self::on_long_op).
    pub fn delete_songs(&self, ids: Vec<i64>) -> OpId {
        let op = library::delete_songs(&self.inner.services.ctx, ids);
        self.inner.deletes.borrow_mut().insert(op);
        op
    }

    /// Whether a delete started here is still running.
    pub fn is_deleting(&self) -> bool {
        !self.inner.deletes.borrow().is_empty()
    }

    /// A long-operation event: the end of a delete started here. A success
    /// clears the selection (Iced's refresh does, `src/gui/app.rs:3944`); a
    /// failure that is not a cancellation shows an error toast (quirk fix 8)
    /// as well as the log line.
    pub fn on_long_op(&self, event: &Event, ctx: &mut EventContext) {
        let Event::LongOperation(event) = event else {
            return;
        };
        let (info, failed) = match event {
            LongOpEvent::Completed(info) => (info, false),
            LongOpEvent::Failed(info) => (info, true),
            LongOpEvent::Progress(_) => return,
        };
        if info.kind != LongOpKind::LibraryDelete
            || !self.inner.deletes.borrow_mut().remove(&info.op_id)
        {
            return;
        }
        let services = &self.inner.services;
        let _ = services.ctx.ops.take_result::<usize>(info.op_id);
        if !failed {
            self.deselect();
            return;
        }
        let reason = info.error.clone().unwrap_or_default();
        tracing::warn!("the songs could not be deleted from the library: {reason}");
        if !info.cancelled {
            ctx.show_toast(Toast::error(tr!(library_delete_failed())).body(lit!(reason)));
        }
    }

    /// The "more" action (`OpenFolderPicker`, `src/gui/app.rs:3498`): picks a
    /// folder and scans it. The picker runs off the UI thread; a scan that
    /// cannot start shows an error toast (quirk fix 8).
    pub fn add_folder(&self, ctx: &mut EventContext) {
        let vm = self.clone();
        let _ = ctx.pick_folder(FileDialogRequest::pick_folder(), move |result, ctx| {
            if let FileDialogResult::Folder(Some(path)) = result {
                vm.scan_folder(&path.to_string_lossy(), ctx);
            }
        });
    }

    /// Starts scanning `path`; a failure shows an error toast.
    pub fn scan_folder(&self, path: &str, ctx: &mut EventContext) {
        if let Err(error) = library::scan_folder(&self.inner.services.ctx, path) {
            tracing::warn!("the folder could not be scanned: {error}");
            ctx.show_toast(Toast::error(tr!(library_scan_failed())).body(lit!(error.to_string())));
        }
    }

    // -----------------------------------------------------------------
    // Reveal
    // -----------------------------------------------------------------

    /// "Show playing" (`LibraryShowPlaying`, `src/gui/app.rs:4197`): reveals
    /// the playing track.
    pub fn show_playing(&self) {
        let path = self.inner.services.now_playing.path().get();
        if !path.is_empty() {
            self.reveal_path(&path);
        }
    }

    /// Reveals the song at `path` (`handle_library_reveal`,
    /// `src/gui/app.rs:6073`): selects it and scrolls to it, opening what hides
    /// it, and asks for the keyboard ([`focus_request`](Self::focus_request)). When the search or a filter hides it, they are cleared, what was
    /// cleared goes out on [`reveal_cleared`](Self::reveal_cleared), and the
    /// reveal resumes when the new results arrive.
    pub fn reveal_path(&self, path: &str) {
        let inner = &self.inner;
        let snapshot = inner.services.library_songs.snapshot().get();
        if snapshot
            .songs
            .iter()
            .any(|s| s.full_file_path.as_ref() == path)
        {
            self.reveal_found(path);
            inner.focus_request.send(());
            return;
        }
        let params = inner.services.library_songs.params();
        if !has_any_filter(&params) && inner.search.get().is_empty() {
            return;
        }
        inner.focus_request.send(());
        *inner.pending_reveal.borrow_mut() = Some(path.to_string());
        let search = inner.search.get();
        inner.reveal_cleared.send(LibrarySearchParams {
            query: (!search.is_empty()).then_some(search),
            ..params
        });
        inner.search.set_if_changed(String::new());
        inner
            .services
            .library_songs
            .set_params(LibrarySearchParams::default());
    }

    /// Reveals a song the current results hold.
    fn reveal_found(&self, path: &str) {
        let inner = &self.inner;
        let snapshot = inner.services.library_songs.snapshot().get();
        let Some(song) = snapshot
            .songs
            .iter()
            .find(|s| s.full_file_path.as_ref() == path)
            .cloned()
        else {
            return;
        };
        if inner.view_mode.get() == LibraryViewMode::Grid {
            self.reveal_in_grid(&song);
            return;
        }
        let structure = inner.structure.get();
        if let Some(artist) = artist_to_toggle_fixed(
            None,
            Some(&*song),
            LibraryNavDir::Right,
            &structure.groups,
            &structure.data(),
        ) {
            inner.collapsed_artists.borrow_mut().remove(&artist);
        }
        // The song's album opens in every list mode (quirk fix 51); Iced
        // opens it in the Detailed mode only (`src/gui/app.rs:6266-6300`), so
        // in the Simple and Thumbnail modes a song in a collapsed album was
        // selected but not shown. The album is the run that holds the song,
        // the composite id its header toggles.
        if let Some(album) = structure.album_id_for_song(song.id) {
            inner.collapsed_albums.borrow_mut().remove(&album);
        }
        self.rebuild();
        let item = LibraryListItem::Song(song.id);
        let mut selection = inner.selection.get();
        selection.select_only(item.clone());
        inner.selection.set_if_changed(selection);
        if let Some((y, height)) = inner.structure.get().item_y_range(&item) {
            inner.scroll.send(ScrollRequest::Reveal {
                item,
                y,
                height,
                force_top: true,
            });
        }
    }

    /// The Grid part of the reveal: opens the song's album under its row of
    /// cards, selects the song and puts its row at the top. The album is found
    /// by its title and the artist the grid groups it under, the song's album
    /// artist (else its artist, `get_effective_artist`; quirk fix 50). Iced
    /// looks for the song's `artist` tag (`src/gui/app.rs:6125-6140`), which a
    /// compilation's card does not bear, so it opened nothing.
    fn reveal_in_grid(&self, song: &Arc<SongData>) {
        let inner = &self.inner;
        let album_name = song.album.as_deref().unwrap_or(UNKNOWN_ALBUM);
        let artist_name = get_effective_artist(song);
        let list = inner.services.albums.list();
        let found = (0..list.len()).find_map(|i| {
            list.with_item(i, |card| {
                (card.title == album_name && card.artist == artist_name)
                    .then(|| (i, card.id.clone()))
            })
            .flatten()
        });
        let Some((tile, id)) = found else {
            return;
        };
        if inner.expanded_album.get().as_deref() != Some(id.as_str()) {
            self.open_detail(&id);
        }
        self.write_grid_selection(|keyed| keyed.clear());
        *inner.grid_anchor.borrow_mut() = Some(id);
        let item = LibraryListItem::Song(song.id);
        let mut selection = inner.selection.get();
        selection.select_only(item);
        inner.selection.set_if_changed(selection);
        let visible = visible_album_songs(&inner.expanded_songs.get(), &inner.search.get());
        match visible.iter().position(|s| s.id == song.id) {
            Some(row) => inner.scroll.send(ScrollRequest::GridSong {
                tile,
                row,
                force_top: true,
            }),
            None => inner.scroll.send(ScrollRequest::GridTile(tile)),
        }
    }

    /// Subscribes to the long operations (the end of a delete) and to the
    /// library's changes ([`on_library_changed`](Self::on_library_changed))
    /// for as long as the widget building it lives; call it from the panel's
    /// `build`.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let vm = self.clone();
        ctx.subscribe_event_with_ctx(Origin::LongOperation, move |event: &Event, ctx| {
            vm.on_long_op(event, ctx);
        });
        let vm = self.clone();
        ctx.subscribe_event(Origin::Library, move |event: &Event| {
            if is_reload_event(event) {
                vm.on_library_changed();
            }
        });
    }
}

#[cfg(test)]
mod tests;
