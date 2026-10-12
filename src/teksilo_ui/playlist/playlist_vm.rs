//! `PlaylistViewModel`: the state and the behaviour of the playlist panel.
//!
//! The Iced UI keeps the playlist in `PlaylistManager` (`src/gui/playlist.rs`)
//! and handles it in `AudoxidyApp::update` (`src/gui/app.rs`). Here the
//! playlist's rows, the playing song, repeat, shuffle and the shuffle session
//! live in the shared [`PlaybackCursor`] (`services.session`), which the facade
//! commands update; the view-model adds what only the panel needs:
//!
//! - the **rows** on screen ([`rows`](PlaylistViewModel::rows), see
//!   [`rows`](super::rows)), rebuilt whenever the cursor's rows change: a
//!   content reload (`services.playlist_content`), a search, a collapse, a
//!   switch of playlist;
//! - the **selection**, in linear indices, with its focus and pivot
//!   (`logic::playlist_nav::PlaylistSelection`), and the **collapsed** groups;
//! - the **playing marker**: the linear index of the playing song, found from
//!   its song id whenever the cursor changes, so a search never moves it to
//!   the wrong row (quirk fix 11);
//! - the **double click**: a second click on the same row within 400 ms plays
//!   the song or folds the group (`PlaySongIndex`, `TogglePlaylistFolder`,
//!   `src/gui/app.rs:1956`, `:2115`);
//! - the **autoscroll** requests ([`scroll_request`](PlaylistViewModel::scroll_request)):
//!   to the focused row after the arrow keys, to the playing row after a track
//!   change unless the pointer is over the list (`is_mouse_over_playlist`),
//!   to the playing row or the top after a switch, to the top after a search;
//! - the long operations it started (export, M3U import, external files) and
//!   what the user is told when they end ([`Notice`]).
//!
//! The panel's event subscriptions call [`on_event`](PlaylistViewModel::on_event):
//! the end of a track advances once (quirk fix 28, in the facade), the
//! position preloads the next song once per track and prefetches its cover,
//! `Session::AutosaveDue` persists the position (quirk fix 13: a facade-thread
//! event, so it also runs while the window is minimised), and the system media
//! controls drive the transport. No playback command runs before the startup
//! restore has settled (quirk fix 6).
//!
//! Persistence happens where Iced persists: every facade command the
//! view-model calls (`play_song`, `end_of_track`, `toggle_repeat`,
//! `toggle_shuffle`, `switch_playlist`, …) persists the playlist state itself.
//!
//! Teksilo-only fixes made here (`docs/teksilo-port-quirk-fixes.md`): the
//! selection follows its rows through a search (quirk fix 56), renaming the
//! active playlist leaves the playing track alone (quirk fix 55), and "Enviar
//! a otra lista" leaves the active playlist out (quirk fix 58).
//!
//! Changing a playlist's content (adding, importing, sending songs, removing)
//! reloads only its rows (quirk fix 12): the content model applies the new
//! rows, and the view-model rebuilds its own. When the songs of the playlist
//! changed, the selection goes back to the playing row, as it does in Iced,
//! where every such change switches to the playlist again.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use teksilo::core::signal::ObserverHandle;
use teksilo::core::window::TeksiloWindowId;
use teksilo::data::ListModel;
use teksilo::prelude::*;

use super::rows::{KeyedSelection, RowItem, ScrollTarget, visible_rows};
use crate::db::database::PlaylistSongRef;
use crate::frontend::commands::playback::{self, EndOfTrackOutcome, PlaybackCursor, TrackChange};
use crate::frontend::commands::playlist::{self, ExportReport, PlaylistSwitch, RenameOutcome};
use crate::frontend::events::{Event, LongOpEvent, LongOpInfo, OpId, PlaybackEvent, SessionEvent};
use crate::frontend::reply::reply_channel;
use crate::frontend::{FrontendError, Reply};
use crate::integrations::control::ExternalControlEvent;
use crate::logic::playlist_export::{ExportFormat, ExportMode};
use crate::logic::playlist_nav::{
    self as nav, PlaylistItemType, PlaylistSelection, RepeatMode, VerticalDir,
};
use crate::logic::selection_modifiers::SelectModifiers;
use crate::teksilo_ui::app::{AppServices, Request, audio_center_tab};
use crate::teksilo_ui::models::PlaylistTab;

/// Two clicks on the same row closer than this are a double click
/// (`src/gui/app.rs:1960`).
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// The width the tab strip has before its overflow chevron appears
/// (`bar_width`, `src/gui/playlist.rs:567`).
pub const TAB_STRIP_WIDTH: f32 = 380.0;

/// The space between two tabs (`src/gui/playlist.rs:423`).
pub const TAB_SPACING: f32 = 5.0;

/// The width Iced estimates for the tab of `name`: 7.5 px a character plus
/// 10 (`estimate_tab_width`, `src/gui/playlist.rs:624`).
pub fn estimate_tab_width(name: &str) -> f32 {
    name.chars().count() as f32 * 7.5 + 10.0
}

/// Whether tabs named `names` overflow the strip, so the chevron shows: their
/// estimated widths, 5 px apart, exceed 380 px (`src/gui/playlist.rs:566`).
pub fn tabs_overflow<'a>(names: impl IntoIterator<Item = &'a str>) -> bool {
    let total: f32 = names
        .into_iter()
        .map(|name| estimate_tab_width(name) + TAB_SPACING)
        .sum();
    total > TAB_STRIP_WIDTH
}

/// The horizontal offset that brings the tab at `position` of `names` into a
/// strip scrolled to `scroll_x` and `viewport_width` wide, or `None` when it
/// is already in view (2 px of tolerance on each side). The tab goes 15 px
/// from the leading edge (`InternalPlaylistLoaded`, `src/gui/app.rs:2629-2655`;
/// the widths are Iced's estimates).
pub fn tab_scroll_offset(
    names: &[&str],
    position: usize,
    scroll_x: f32,
    viewport_width: f32,
) -> Option<f32> {
    let used: f32 = names
        .iter()
        .take(position)
        .map(|name| estimate_tab_width(name) + TAB_SPACING)
        .sum();
    let width = estimate_tab_width(names.get(position)?);
    let leading_visible = used >= scroll_x - 2.0;
    let trailing_visible = used + width <= scroll_x + viewport_width + 2.0;
    if leading_visible && trailing_visible {
        None
    } else {
        Some((used - 15.0).max(0.0))
    }
}

/// The playlists in the order of the overflow menu and of the "Enviar a otra
/// lista" submenu: the system playlists first, "Audoxidy" then "Oxidy Drift"
/// (`src/gui/app.rs:2697-2712`, `:4024-4038`), then the user playlists, in
/// tab order or sorted case-insensitively (`src/gui/app.rs:4041`).
pub fn menu_order(tabs: &[PlaylistTab], sort_user_lists: bool) -> Vec<PlaylistTab> {
    let (mut system, mut user): (Vec<PlaylistTab>, Vec<PlaylistTab>) =
        tabs.iter().cloned().partition(|tab| tab.is_system);
    system.sort_by_key(|tab| match tab.name.as_str() {
        "Audoxidy" => 0,
        "Oxidy Drift" => 1,
        _ => 2,
    });
    if sort_user_lists {
        user.sort_by(|a, b| crate::utils::compare_strings_ignore_case(&a.name, &b.name));
    }
    system.into_iter().chain(user).collect()
}

/// What the user is told when something they started has ended (a toast):
/// a success, or a failure that Iced only logs (quirk fix 8).
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// The export wrote its list (and copied `copied` songs); `failed` copies
    /// could not be made.
    Exported {
        /// The playlist file written.
        path: PathBuf,
        /// Songs copied (portable export).
        copied: usize,
        /// Copies that failed (portable export).
        failed: usize,
    },
    /// The export failed.
    ExportFailed(String),
    /// The M3U import failed.
    ImportFailed(String),
    /// Adding files or a folder failed.
    AddFilesFailed(String),
    /// Creating a playlist failed.
    CreateFailed(String),
    /// Renaming a playlist failed.
    RenameFailed(String),
    /// Deleting a playlist failed.
    DeleteFailed(String),
    /// The file manager could not be opened on a song.
    FileLocationFailed(String),
}

/// A long operation the view-model started, by what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOp {
    /// A playlist export.
    Export,
    /// An M3U import.
    Import,
    /// Adding files or a folder to a playlist.
    AddFiles,
}

/// Where the toast of an asynchronous result goes: the window of the event
/// that started it (`ctx.window().map(|w| w.id())`). `None` (a headless tree)
/// only logs the failure.
pub type ToastWindow = Option<TeksiloWindowId>;

struct Inner {
    services: AppServices,
    rows: Signal<Rc<Vec<RowItem>>>,
    selection: Signal<PlaylistSelection>,
    collapsed: RefCell<HashSet<i64>>,
    search: Signal<String>,
    playing: Signal<Option<usize>>,
    hovered: Signal<bool>,
    scroll: Request<ScrollTarget>,
    tab_scroll: Request<i64>,
    last_click: Cell<Option<(usize, Instant)>>,
    content_key: Cell<Option<u64>>,
    playing_song: Cell<Option<i64>>,
    switch_seq: Cell<u64>,
    ops: RefCell<HashMap<OpId, PendingOp>>,
    handles: RefCell<Vec<ObserverHandle>>,
}

/// The playlist panel's view-model; see the module documentation. Clones
/// share one state. UI thread only.
#[derive(Clone)]
pub struct PlaylistViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for PlaylistViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistViewModel")
            .field("rows", &self.inner.rows.get_ref().len())
            .field("selection", &self.inner.selection.get().selected.len())
            .field("playing", &self.inner.playing.get())
            .finish_non_exhaustive()
    }
}

impl PlaylistViewModel {
    /// The view-model over `services`. It follows the cursor from the start:
    /// its rows fill when the restore settles, and follow every content
    /// reload, search and switch.
    pub fn new(services: &AppServices) -> Self {
        let vm = Self {
            inner: Rc::new(Inner {
                services: services.clone(),
                rows: Signal::new(Rc::new(Vec::new())),
                selection: Signal::new(PlaylistSelection::default()),
                collapsed: RefCell::new(HashSet::new()),
                search: Signal::new(String::new()),
                playing: Signal::new(None),
                hovered: Signal::new(false),
                scroll: Request::default(),
                tab_scroll: Request::default(),
                last_click: Cell::new(None),
                content_key: Cell::new(None),
                playing_song: Cell::new(None),
                switch_seq: Cell::new(0),
                ops: RefCell::new(HashMap::new()),
                handles: RefCell::new(Vec::new()),
            }),
        };
        vm.observe();
        vm.refresh_rows();
        vm.sync_playing();
        vm
    }

    /// Follows the content model, the restore, the cursor and the search.
    fn observe(&self) {
        let services = &self.inner.services;
        let mut handles = Vec::new();
        let weak = self.weak();
        handles.push(services.playlist_content.version().observe(move |_| {
            if let Some(vm) = upgrade(&weak) {
                vm.refresh_rows();
            }
        }));
        let weak = self.weak();
        handles.push(services.session.status().observe(move |status| {
            if status.is_settled()
                && let Some(vm) = upgrade(&weak)
            {
                vm.refresh_rows();
                vm.sync_playing();
                // A search typed before the restore had no cursor to filter.
                vm.apply_search(&vm.inner.search.get());
                // An end of track published before the restore settled was
                // ignored (quirk fix 6); its flag is still set, so it is
                // handled now (a no-op when none is pending).
                vm.end_of_track();
            }
        }));
        let weak = self.weak();
        handles.push(services.session.cursor_version().observe(move |_| {
            if let Some(vm) = upgrade(&weak) {
                vm.sync_playing();
                vm.drop_stale_preload();
            }
        }));
        let weak = self.weak();
        handles.push(self.inner.search.observe(move |query| {
            if let Some(vm) = upgrade(&weak) {
                vm.apply_search(query);
            }
        }));
        self.inner.handles.borrow_mut().extend(handles);
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.inner)
    }

    /// The services the view-model was built from.
    pub fn services(&self) -> &AppServices {
        &self.inner.services
    }

    // ------------------------------------------------------------------
    // State the views bind to
    // ------------------------------------------------------------------

    /// The rows on screen.
    pub fn rows(&self) -> Signal<Rc<Vec<RowItem>>> {
        self.inner.rows.clone()
    }

    /// The selection, in linear indices.
    pub fn selection(&self) -> Signal<PlaylistSelection> {
        self.inner.selection.clone()
    }

    /// The search query (the bottom bar's field writes it).
    pub fn search(&self) -> Signal<String> {
        self.inner.search.clone()
    }

    /// The linear index of the playing song in the rows on screen; `None`
    /// when nothing plays or the search hides it (quirk fix 11).
    pub fn playing_index(&self) -> Signal<Option<usize>> {
        self.inner.playing.clone()
    }

    /// Whether the pointer is over the list (the list writes it). A track
    /// change does not scroll the list under the pointer.
    pub fn hovered(&self) -> Signal<bool> {
        self.inner.hovered.clone()
    }

    /// The list's scroll requests.
    pub fn scroll_request(&self) -> Signal<Option<(u64, ScrollTarget)>> {
        self.inner.scroll.latest()
    }

    /// The tab strip's scroll requests: the playlist whose tab to bring into
    /// view after a switch.
    pub fn tab_scroll_request(&self) -> Signal<Option<(u64, i64)>> {
        self.inner.tab_scroll.latest()
    }

    /// The playlist tabs, in tab order.
    pub fn tabs(&self) -> ListModel<PlaylistTab> {
        self.inner.services.playlist_tabs.list()
    }

    /// A counter that moves whenever the tabs were reloaded.
    pub fn tabs_version(&self) -> Signal<u64> {
        self.inner.services.playlist_tabs.version()
    }

    /// The tabs, in tab order.
    pub fn tab_list(&self) -> Vec<PlaylistTab> {
        let list = self.tabs();
        (0..list.len())
            .filter_map(|index| list.with_item(index, Clone::clone))
            .collect()
    }

    /// The active playlist's id.
    pub fn active_playlist(&self) -> Signal<Option<i64>> {
        self.inner.services.active_playlist.id()
    }

    /// Whether shuffle is on.
    pub fn shuffle(&self) -> Signal<bool> {
        self.inner.services.active_playlist.shuffle()
    }

    /// The repeat mode.
    pub fn repeat(&self) -> Signal<RepeatMode> {
        self.inner.services.active_playlist.repeat()
    }

    /// The groups currently collapsed, by anchor.
    pub fn collapsed(&self) -> HashSet<i64> {
        self.inner.collapsed.borrow().clone()
    }

    /// Whether the startup restore has settled: until then no playback command
    /// runs (quirk fix 6).
    pub fn is_ready(&self) -> bool {
        self.inner.services.session.status().get().is_settled()
    }

    /// Runs `f` on the cursor without changing it.
    pub fn read_cursor<R>(&self, f: impl FnOnce(&PlaybackCursor) -> R) -> Option<R> {
        self.inner.services.session.read_cursor(f)
    }

    /// The song at `linear_idx` in the rows on screen.
    pub fn song_at(&self, linear_idx: usize) -> Option<PlaylistSongRef> {
        self.read_cursor(|cursor| cursor.song_at(linear_idx).cloned())
            .flatten()
    }

    /// What the item at `linear_idx` is.
    pub fn item_at(&self, linear_idx: usize) -> Option<PlaylistItemType> {
        self.read_cursor(|cursor| nav::item_at(cursor.active_groups(), linear_idx))
            .flatten()
    }

    // ------------------------------------------------------------------
    // Rows
    // ------------------------------------------------------------------

    /// Rebuilds the rows from the cursor. When the playlist or its songs
    /// changed, the selection goes back to the playing row.
    fn refresh_rows(&self) {
        let inner = &self.inner;
        let snapshot = self.read_cursor(|cursor| {
            let collapsed = inner.collapsed.borrow();
            (
                visible_rows(cursor.active_groups(), cursor.groups(), &collapsed),
                content_key(cursor),
                cursor.playing_index(),
            )
        });
        let Some((rows, key, playing)) = snapshot else {
            if !inner.rows.get_ref().is_empty() {
                inner.rows.set(Rc::new(Vec::new()));
            }
            return;
        };
        inner.playing.set_if_changed(playing);
        let changed = inner.content_key.replace(Some(key)) != Some(key);
        inner.rows.set(Rc::new(rows));
        if changed {
            self.select_playing_row();
        }
    }

    /// Selects only the playing row, or nothing (what Iced's
    /// `InternalPlaylistLoaded` leaves, `src/gui/app.rs:2591-2620`).
    fn select_playing_row(&self) {
        let mut selection = PlaylistSelection::default();
        if let Some(playing) = self.inner.playing.get() {
            selection.select_only(playing);
        }
        self.inner.selection.set_if_changed(selection);
    }

    /// Follows the playing song, shuffle and repeat. A new playing song
    /// scrolls into view unless the pointer is over the list (`Tick`,
    /// `NextTrack`, `PreviousTrack`, `src/gui/app.rs:1450`, `:1774`, `:1805`).
    fn sync_playing(&self) {
        let inner = &self.inner;
        let Some((playing, song, shuffle, repeat)) = self.read_cursor(|cursor| {
            (
                cursor.playing_index(),
                cursor.playing_song_id(),
                cursor.shuffle_active(),
                cursor.repeat(),
            )
        }) else {
            return;
        };
        inner.playing.set_if_changed(playing);
        let active = &inner.services.active_playlist;
        active.shuffle().set_if_changed(shuffle);
        active.repeat().set_if_changed(repeat);
        if inner.playing_song.replace(song) != song
            && !inner.hovered.get()
            && let Some(playing) = playing
        {
            inner.scroll.send(ScrollTarget::Item(playing));
        }
    }

    /// Drops a pending preload that is no longer the song Next would play,
    /// whatever changed the cursor (a removal, a search, a disabled song, a
    /// switch): Iced defect D28, `playback::drop_stale_preload`.
    fn drop_stale_preload(&self) {
        let ctx = &self.inner.services.ctx;
        self.read_cursor(|cursor| playback::drop_stale_preload(ctx, cursor));
    }

    /// Applies the search and scrolls to the top (`PlaylistSearchChanged`,
    /// `src/gui/app.rs:2746`). The selection follows its rows: the selected
    /// songs the search still shows stay selected under their new linear
    /// indices, the others drop out, and the focus and the pivot follow
    /// likewise (quirk fix 56; Iced keeps the old linear indices, which then
    /// point at other rows).
    fn apply_search(&self, query: &str) {
        let selection = self.inner.selection.get();
        let carried = self
            .inner
            .services
            .session
            .with_cursor(|cursor| {
                if cursor.search() == query {
                    return None;
                }
                let keyed = KeyedSelection::of(&selection, cursor.active_groups());
                cursor.set_search(query);
                Some(keyed.in_groups(cursor.active_groups()))
            })
            .flatten();
        if let Some(carried) = carried {
            self.inner.selection.set_if_changed(carried);
            self.refresh_rows();
            self.inner.scroll.send(ScrollTarget::Top);
        }
    }

    /// The visible items (rows) as `logic::playlist_nav` reads them.
    fn visible_items(&self) -> Vec<(PlaylistItemType, usize)> {
        let collapsed = self.inner.collapsed.borrow();
        self.read_cursor(|cursor| nav::visible_items(cursor.active_groups(), &collapsed))
            .unwrap_or_default()
    }

    /// Changes the selection with `f`.
    fn update_selection<R>(&self, f: impl FnOnce(&mut PlaylistSelection) -> R) -> R {
        let mut selection = self.inner.selection.get();
        let result = f(&mut selection);
        self.inner.selection.set_if_changed(selection);
        result
    }

    // ------------------------------------------------------------------
    // Selection and keys
    // ------------------------------------------------------------------

    /// A click on the row `linear_idx` at `now` (`PlaySongIndex`,
    /// `TogglePlaylistFolder`): Ctrl toggles it in the selection, Shift
    /// selects the range from the pivot, otherwise it alone is selected. A
    /// second click on the same row within 400 ms plays a song, or folds or
    /// unfolds a group.
    pub fn click(&self, linear_idx: usize, modifiers: SelectModifiers, now: Instant) {
        let previous = self.inner.last_click.replace(Some((linear_idx, now)));
        let double = previous.is_some_and(|(last, at)| {
            last == linear_idx && now.saturating_duration_since(at) < DOUBLE_CLICK
        });
        let visible = self.visible_items();
        self.update_selection(|selection| selection.click(linear_idx, modifiers, &visible));
        if double {
            match self.item_at(linear_idx) {
                Some(PlaylistItemType::Song(..)) => self.play_row(linear_idx),
                Some(PlaylistItemType::Separator(_, anchor)) => self.toggle_group(anchor),
                None => {}
            }
        }
    }

    /// Clears the selection (`PlaylistDeselect`, a press on the panel's
    /// background).
    pub fn deselect(&self) {
        self.update_selection(PlaylistSelection::clear);
    }

    /// Up or Down: moves the focus one row (Shift extends the selection from
    /// the pivot) and scrolls the focused row into view
    /// (`GlobalKeyDown`, `src/gui/app.rs:3221-3246`).
    pub fn key_move(&self, down: bool, extend: bool) {
        let visible = self.visible_items();
        let dir = if down {
            VerticalDir::Down
        } else {
            VerticalDir::Up
        };
        let focused = self.update_selection(|selection| {
            selection.navigate(dir, extend, &visible);
            selection.focused
        });
        if let Some(focused) = focused {
            self.inner.scroll.send(ScrollTarget::Item(focused));
        }
    }

    /// Left: folds every group the selection touches; a focused song hands
    /// the selection to its separator.
    pub fn collapse_selected(&self) {
        let mut selection = self.inner.selection.get();
        let changed = self
            .read_cursor(|cursor| {
                nav::collapse_selected_groups(
                    cursor.active_groups(),
                    &mut self.inner.collapsed.borrow_mut(),
                    &mut selection,
                )
            })
            .unwrap_or(false);
        self.inner.selection.set_if_changed(selection);
        if changed {
            self.refresh_rows();
        }
    }

    /// Right: unfolds every group the selection touches.
    pub fn expand_selected(&self) {
        let selection = self.inner.selection.get();
        let changed = self
            .read_cursor(|cursor| {
                nav::expand_selected_groups(
                    cursor.active_groups(),
                    &mut self.inner.collapsed.borrow_mut(),
                    &selection,
                )
            })
            .unwrap_or(false);
        if changed {
            self.refresh_rows();
        }
    }

    /// Ctrl+A: selects every row (the app routes it to the focused panel).
    pub fn select_all(&self) {
        let visible = self.visible_items();
        self.update_selection(|selection| selection.select_all(&visible));
    }

    /// Enter: plays the focused row if it is a song (or restarts it if it
    /// plays).
    pub fn activate_focused(&self) {
        if let Some(focused) = self.inner.selection.get().focused
            && self.song_at(focused).is_some()
        {
            self.play_row(focused);
        }
    }

    /// Delete: removes what a click on the focused row targets.
    pub fn remove_focused(&self) {
        if let Some(focused) = self.inner.selection.get().focused {
            self.remove(focused);
        }
    }

    /// Space: the enable toggle on the focused row.
    pub fn toggle_enabled_focused(&self) {
        if let Some(focused) = self.inner.selection.get().focused {
            self.toggle_enabled(focused);
        }
    }

    /// A context menu opens on the row `linear_idx`: a row outside the
    /// selection becomes the selection (`RequestContextMenu`,
    /// `src/gui/app.rs:4858-4878`).
    pub fn menu_opened_on(&self, linear_idx: usize) {
        self.update_selection(|selection| {
            if !selection.selected.contains(&linear_idx) {
                selection.select_only(linear_idx);
            }
        });
    }

    // ------------------------------------------------------------------
    // Folding
    // ------------------------------------------------------------------

    /// Folds or unfolds the group `anchor`.
    pub fn toggle_group(&self, anchor: i64) {
        nav::toggle_group_collapsed(&mut self.inner.collapsed.borrow_mut(), anchor);
        self.refresh_rows();
    }

    /// "Plegar | Desplegar": folds every group if any is open, else unfolds
    /// them all (`PlaylistToggleAllFolders`, `src/gui/app.rs:4104`).
    pub fn toggle_all_groups(&self) {
        let toggled = self
            .read_cursor(|cursor| {
                nav::toggle_all_groups(cursor.groups(), &mut self.inner.collapsed.borrow_mut());
            })
            .is_some();
        if toggled {
            self.refresh_rows();
        }
    }

    // ------------------------------------------------------------------
    // Playing
    // ------------------------------------------------------------------

    /// Runs a playback command on the cursor, once the restore has settled
    /// (quirk fix 6).
    fn with_playback<R>(&self, f: impl FnOnce(&mut PlaybackCursor) -> R) -> Option<R> {
        if !self.is_ready() {
            tracing::debug!("the session is not restored yet; the playback command is ignored");
            return None;
        }
        self.inner.services.session.with_cursor(f)
    }

    /// Plays the song row `linear_idx`, or restarts it if it is playing
    /// (`play_song_from_playlist`, `src/gui/app.rs:1209`).
    pub fn play_row(&self, linear_idx: usize) {
        let ctx = self.inner.services.ctx.clone();
        let outcome = self.with_playback(|cursor| {
            let song_id = cursor.song_at(linear_idx)?.song_id;
            Some(playback::play_song(&ctx, cursor, song_id))
        });
        if let Some(Some(Err(error))) = outcome {
            tracing::error!("the song could not be played: {error}");
        }
    }

    /// "Reproducir" in the row menu: plays the song, or the first enabled song
    /// of a group, and selects it (`PlaylistContextMenuPlay`,
    /// `src/gui/app.rs:3526`).
    pub fn play_item(&self, linear_idx: usize) {
        let ctx = self.inner.services.ctx.clone();
        let outcome = self.with_playback(|cursor| playback::play_item(&ctx, cursor, linear_idx));
        match outcome {
            Some(Ok(TrackChange::Loaded(_) | TrackChange::Restarted(_))) => {
                if let Some(playing) = self.inner.playing.get() {
                    self.update_selection(|selection| selection.select_only(playing));
                }
            }
            Some(Ok(TrackChange::Unchanged)) | None => {}
            Some(Err(error)) => tracing::error!("the song could not be played: {error}"),
        }
    }

    /// Turns shuffle on or off (`ToggleShuffle`).
    pub fn toggle_shuffle(&self) {
        let ctx = self.inner.services.ctx.clone();
        self.inner
            .services
            .session
            .with_cursor(|cursor| playback::toggle_shuffle(&ctx, cursor));
    }

    /// Cycles the repeat mode Off → All → One (`ToggleRepeat`).
    pub fn toggle_repeat(&self) {
        let ctx = self.inner.services.ctx.clone();
        self.inner
            .services
            .session
            .with_cursor(|cursor| playback::toggle_repeat(&ctx, cursor));
    }

    /// The equalizer icon: the Audio Center on its equalizer tab, with
    /// Iced's toggle rule (`ToggleAudioCenter(Some(1))`).
    pub fn toggle_equalizer(&self) {
        self.inner
            .services
            .links
            .audio_center
            .toggle(Some(audio_center_tab::EQUALIZER));
    }

    // ------------------------------------------------------------------
    // Enabled flags
    // ------------------------------------------------------------------

    /// The "•" of a song row: flips that song (`ToggleSongEnabled`).
    pub fn toggle_song_enabled(&self, linear_idx: usize) {
        let ctx = self.inner.services.ctx.clone();
        let toggled = self
            .inner
            .services
            .session
            .with_cursor(|cursor| playlist::toggle_song_enabled(&ctx, cursor, linear_idx));
        if toggled.flatten().is_some() {
            self.refresh_rows();
        }
    }

    /// The "•" of a separator: enables the group when every song is disabled,
    /// otherwise disables it (`ToggleGroupEnabled`).
    pub fn toggle_group_enabled(&self, linear_idx: usize) {
        let ctx = self.inner.services.ctx.clone();
        let toggled = self
            .inner
            .services
            .session
            .with_cursor(|cursor| playlist::toggle_group_enabled(&ctx, cursor, linear_idx));
        if toggled.flatten().is_some() {
            self.refresh_rows();
        }
    }

    /// "Activar | Desactivar" and Space: the selection when the row is in it,
    /// else the row, all enabled if any is disabled, else all disabled
    /// (`PlaylistToggleEnabled`, `src/gui/app.rs:4128`).
    pub fn toggle_enabled(&self, linear_idx: usize) {
        let ctx = self.inner.services.ctx.clone();
        let selected = self.inner.selection.get().selected;
        let toggled =
            self.inner.services.session.with_cursor(|cursor| {
                playlist::toggle_enabled(&ctx, cursor, &selected, linear_idx)
            });
        if toggled.is_some() {
            self.refresh_rows();
        }
    }

    // ------------------------------------------------------------------
    // Songs
    // ------------------------------------------------------------------

    /// "Eliminar de la lista" and Delete: removes the selection when the row
    /// is in it, else the row, resolved over the rows on screen (Iced defect
    /// D2). The rows reload on the content change. Returns the facade's reply
    /// (the number of songs removed), `None` before the restore.
    pub fn remove(&self, linear_idx: usize) -> Option<Reply<usize>> {
        let selected = self.inner.selection.get().selected;
        let ctx = &self.inner.services.ctx;
        let reply =
            self.read_cursor(|cursor| playlist::remove_songs(ctx, cursor, &selected, linear_idx))?;
        Some(self.logged(reply, "the songs could not be removed from the playlist"))
    }

    /// The song ids a send-to from the row `linear_idx` carries.
    pub fn send_targets(&self, linear_idx: usize) -> Vec<i64> {
        let selected = self.inner.selection.get().selected;
        self.read_cursor(|cursor| cursor.action_targets(&selected, linear_idx))
            .unwrap_or_default()
    }

    /// "Enviar a otra lista" → a playlist: adds the targeted songs to
    /// `target_playlist_id` (`PlaylistSendToList`). Returns the facade's
    /// reply, `None` before the restore.
    pub fn send_to(&self, linear_idx: usize, target_playlist_id: i64) -> Option<Reply<usize>> {
        let selected = self.inner.selection.get().selected;
        let ctx = &self.inner.services.ctx;
        let reply = self.read_cursor(|cursor| {
            playlist::send_to(ctx, cursor, &selected, linear_idx, target_playlist_id)
        })?;
        Some(self.logged(reply, "the songs could not be sent to the playlist"))
    }

    /// The playlists of the "Enviar a otra lista" submenu, in its order: the
    /// active playlist is left out (quirk fix 58; Iced lists it, and sending
    /// its own songs to it adds nothing, `src/gui/app.rs:4024-4041`).
    pub fn send_to_lists(&self) -> Vec<PlaylistTab> {
        let active = self.active_playlist().get();
        menu_order(&self.tab_list(), true)
            .into_iter()
            .filter(|tab| Some(tab.id) != active)
            .collect()
    }

    /// The playlists of the tab overflow menu, in its order.
    pub fn overflow_lists(&self) -> Vec<PlaylistTab> {
        menu_order(&self.tab_list(), false)
    }

    /// "Mostrar en la biblioteca": asks the library to reveal the song, or a
    /// group's first song (`PlaylistShowInLibrary`, `src/gui/app.rs:3674`).
    pub fn show_in_library(&self, linear_idx: usize) {
        let path = self
            .read_cursor(|cursor| {
                let groups = cursor.active_groups();
                match nav::item_at(groups, linear_idx)? {
                    PlaylistItemType::Song(..) => cursor.song_at(linear_idx),
                    PlaylistItemType::Separator(group_idx, _) => {
                        groups.get(group_idx)?.songs.first()
                    }
                }
                .map(|song| song.file_path.to_string())
            })
            .flatten();
        if let Some(path) = path {
            self.inner.services.links.reveal_in_library.send(path);
        }
    }

    /// "Ubicación del archivo": opens the file manager on the song, off the UI
    /// thread (`PlaylistShowFileLocation`, `src/gui/app.rs:3639`). A failure is
    /// toasted in `window`.
    pub fn show_file_location(&self, linear_idx: usize, window: ToastWindow) {
        let Some(song) = self.song_at(linear_idx) else {
            return;
        };
        let path = song.file_path.to_string();
        let future = async move {
            match spawn_blocking(move || {
                crate::frontend::commands::files::show_file_location(&path)
            })
            .await
            {
                Ok(result) => result.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        self.finish(window, future, |result| match result {
            Ok(()) => None,
            Err(error) => {
                tracing::warn!("the file manager could not be opened: {error}");
                Some(Notice::FileLocationFailed(error))
            }
        });
    }

    // ------------------------------------------------------------------
    // Playlists
    // ------------------------------------------------------------------

    /// Switches to playlist `id` (`SwitchPlaylist`): the current state is
    /// persisted, the playlist loaded, then put in the cursor with its saved
    /// song (or, with `auto_play`, its first enabled song, playing). The rows,
    /// the selection (the playing row), the active playlist and the scroll
    /// follow. A switch overtaken by another is dropped. Nothing happens
    /// before the restore (quirk fix 6).
    pub fn switch_playlist(&self, id: i64, auto_play: bool) {
        if !self.is_ready() {
            return;
        }
        let ctx = &self.inner.services.ctx;
        let Some(reply) =
            self.read_cursor(|cursor| playlist::switch_playlist(ctx, cursor, id, auto_play))
        else {
            return;
        };
        let seq = self.inner.switch_seq.get().wrapping_add(1);
        self.inner.switch_seq.set(seq);
        self.inner.services.playlist_content.switch_started(id);
        let vm = self.clone();
        self.inner
            .services
            .runtime
            .spawn_local(async move {
                match reply.recv().await {
                    Ok(switch) if vm.inner.switch_seq.get() == seq => vm.finish_switch(switch),
                    Ok(_) => {}
                    Err(error) => tracing::warn!("the playlist could not be opened: {error}"),
                }
            })
            .detach();
    }

    /// Applies a loaded switch (`InternalPlaylistLoaded`, `src/gui/app.rs:2548`).
    fn finish_switch(&self, switch: PlaylistSwitch) {
        let services = &self.inner.services;
        let id = switch.playlist_id();
        let ctx = services.ctx.clone();
        if let Some(Err(error)) = services
            .session
            .with_cursor(|cursor| switch.apply(&ctx, cursor))
        {
            tracing::warn!("the playlist's song could not be loaded: {error}");
        }
        let (shuffle, repeat) = self
            .read_cursor(|cursor| (cursor.shuffle_active(), cursor.repeat()))
            .unwrap_or((false, RepeatMode::Off));
        let tab = services.playlist_tabs.tab(id);
        let (name, is_system) = tab.map_or((String::new(), false), |tab| (tab.name, tab.is_system));
        services
            .active_playlist
            .set(id, &name, is_system, shuffle, repeat);
        services.active_playlist.refresh();
        services.playlist_content.notify_changed();
        if services.playlist_content.switch_finished(id) {
            // The playlist's content changed while it loaded: its rows may
            // predate the change.
            services.playlist_content.reload();
        }
        self.select_playing_row();
        let target = self
            .inner
            .playing
            .get()
            .map_or(ScrollTarget::Top, ScrollTarget::Item);
        self.inner.scroll.send(target);
        self.inner.tab_scroll.send(id);
    }

    /// The name a new playlist gets when its name is left blank: "Nueva lista
    /// n", n being the number of playlists plus one (`src/gui/app.rs:2213`).
    pub fn default_playlist_name(&self) -> String {
        let number = self.tabs().len() + 1;
        tr!(dialogs_create_default_name(number = number)).resolve_now()
    }

    /// Whether playlist `id` can be deleted: not a system playlist (quirk fix
    /// 16).
    pub fn can_delete(&self, id: i64) -> bool {
        self.inner
            .services
            .playlist_tabs
            .tab(id)
            .is_some_and(|tab| tab.can_delete())
    }

    /// Creates a playlist named `name` (the default name when blank) holding
    /// `songs`, then switches to it (`ConfirmDialogAction`,
    /// `src/gui/app.rs:2208`). A failure is toasted in `window`.
    pub fn create_playlist(&self, name: &str, songs: Vec<i64>, window: ToastWindow) {
        let name = if name.trim().is_empty() {
            self.default_playlist_name()
        } else {
            name.to_string()
        };
        let reply = playlist::create_playlist(&self.inner.services.ctx, &name, songs);
        let vm = self.clone();
        self.finish(window, reply.recv(), move |result| match result {
            Ok(id) => {
                vm.switch_playlist(id, false);
                None
            }
            Err(error) => {
                tracing::warn!("the playlist could not be created: {error}");
                Some(Notice::CreateFailed(error.to_string()))
            }
        });
    }

    /// Renames playlist `id` to `name`, or, for a system playlist, makes a
    /// renamed copy; then switches to it (`src/gui/app.rs:2248`). The active
    /// playlist, renamed, only takes its new name: Iced switches to it again,
    /// which reloads and seeks the playing track (`src/gui/app.rs:2404-2420`,
    /// quirk fix 55). A blank name does nothing, as in Iced. A failure is
    /// toasted in `window`.
    pub fn rename_playlist(&self, id: i64, name: &str, window: ToastWindow) {
        if name.trim().is_empty() {
            return;
        }
        let reply = playlist::rename_playlist(&self.inner.services.ctx, id, name);
        let vm = self.clone();
        self.finish(window, reply.recv(), move |result| match result {
            Ok(RenameOutcome::Renamed(id))
                if vm.inner.services.active_playlist.id().get() == Some(id) =>
            {
                // Quirk fix 55: the active playlist only takes its new name;
                // the playing track is not reloaded, seeked or replayed.
                vm.inner.services.active_playlist.refresh();
                None
            }
            Ok(RenameOutcome::Renamed(id) | RenameOutcome::Copied(id)) => {
                vm.switch_playlist(id, false);
                None
            }
            Err(error) => {
                tracing::warn!("the playlist could not be renamed: {error}");
                Some(Notice::RenameFailed(error.to_string()))
            }
        });
    }

    /// Deletes playlist `id`; when it was the active one, switches to the
    /// first playlist (`InternalPlaylistsRefreshed`, `src/gui/app.rs:2404`). A
    /// failure is toasted in `window`.
    pub fn delete_playlist(&self, id: i64, window: ToastWindow) {
        let ctx = self.inner.services.ctx.clone();
        let deleted = playlist::delete_playlist(&ctx, id);
        let vm = self.clone();
        let future = async move {
            deleted.recv().await?;
            playlist::list_playlists(&ctx).recv().await
        };
        self.finish(window, future, move |result| match result {
            Ok(playlists) => {
                let active = vm.inner.services.active_playlist.id().get();
                if !playlists.iter().any(|p| Some(p.id) == active)
                    && let Some(first) = playlists.first()
                {
                    vm.switch_playlist(first.id, false);
                }
                None
            }
            Err(error) => {
                tracing::warn!("the playlist could not be deleted: {error}");
                Some(Notice::DeleteFailed(error.to_string()))
            }
        });
    }

    // ------------------------------------------------------------------
    // Long operations
    // ------------------------------------------------------------------

    /// Exports playlist `id` to `path` as a long operation (`ExportPlaylist`);
    /// its end is a [`Notice`] ([`on_long_op`](Self::on_long_op)).
    pub fn export(&self, id: i64, path: PathBuf, format: ExportFormat, mode: ExportMode) -> OpId {
        let op = playlist::export(&self.inner.services.ctx, id, path, format, mode);
        self.inner.ops.borrow_mut().insert(op, PendingOp::Export);
        op
    }

    /// Imports the M3U/M3U8 file `path` as a new playlist (`ImportPlaylistFile`);
    /// once it is imported, the view-model switches to it and plays it.
    pub fn import(&self, path: PathBuf) -> OpId {
        let op = playlist::import_m3u(&self.inner.services.ctx, path);
        self.inner.ops.borrow_mut().insert(op, PendingOp::Import);
        op
    }

    /// Adds files or folders to the active playlist
    /// (`PlaylistProcessExternalFiles`); the rows reload on the content change.
    /// `None` when no playlist is active yet.
    pub fn add_external(&self, paths: Vec<PathBuf>) -> Option<OpId> {
        let playlist_id = self.read_cursor(PlaybackCursor::playlist_id)?;
        let op = playlist::add_external(&self.inner.services.ctx, paths, playlist_id);
        self.inner.ops.borrow_mut().insert(op, PendingOp::AddFiles);
        Some(op)
    }

    /// The long operations started here that have not ended.
    pub fn pending_ops(&self) -> Vec<(OpId, PendingOp)> {
        let mut ops: Vec<_> = self
            .inner
            .ops
            .borrow()
            .iter()
            .map(|(op, kind)| (*op, *kind))
            .collect();
        ops.sort_by_key(|(op, _)| *op);
        ops
    }

    /// A long-operation event. For an operation started here that ended, takes
    /// its result and returns what to tell the user: an import switches to the
    /// new playlist and plays it; a failure (not a cancellation) is a notice
    /// (quirk fix 8).
    pub fn on_long_op(&self, event: &LongOpEvent) -> Option<Notice> {
        let (info, completed) = match event {
            LongOpEvent::Completed(info) => (info, true),
            LongOpEvent::Failed(info) => (info, false),
            LongOpEvent::Progress(_) => return None,
        };
        let kind = self.inner.ops.borrow_mut().remove(&info.op_id)?;
        let ops = &self.inner.services.ctx.ops;
        if info.cancelled {
            // A cancellation is not a failure the user is told about.
            match kind {
                PendingOp::Export => drop(ops.take_result::<ExportReport>(info.op_id)),
                PendingOp::Import => drop(ops.take_result::<i64>(info.op_id)),
                PendingOp::AddFiles => drop(ops.take_result::<()>(info.op_id)),
            }
            return None;
        }
        match kind {
            PendingOp::Export => match ops.take_result::<ExportReport>(info.op_id) {
                Some(Ok(report)) => Some(Notice::Exported {
                    path: report.playlist_path,
                    copied: report.copied,
                    failed: report.failed_copies.len(),
                }),
                result => failure(info, completed, result.map(|r| r.map(|_| ())))
                    .map(Notice::ExportFailed),
            },
            PendingOp::Import => match ops.take_result::<i64>(info.op_id) {
                Some(Ok(id)) => {
                    self.switch_playlist(id, true);
                    None
                }
                result => failure(info, completed, result.map(|r| r.map(|_| ())))
                    .map(Notice::ImportFailed),
            },
            PendingOp::AddFiles => match ops.take_result::<()>(info.op_id) {
                Some(Ok(())) => None,
                result => failure(info, completed, result).map(Notice::AddFilesFailed),
            },
        }
    }

    // ------------------------------------------------------------------
    // Facade events
    // ------------------------------------------------------------------

    /// A facade event the panel subscribed to (playback, session, external
    /// control). Long operations go through [`on_long_op`](Self::on_long_op),
    /// which needs a context to show its notice.
    pub fn on_event(&self, event: &Event) {
        match event {
            Event::Playback(PlaybackEvent::EndOfTrack(_)) => {
                self.end_of_track();
            }
            Event::Playback(PlaybackEvent::PositionChanged {
                position_secs,
                duration_secs,
            }) => {
                self.position_changed(*position_secs, *duration_secs);
            }
            Event::Session(SessionEvent::AutosaveDue) => {
                self.autosave();
            }
            Event::ExternalControl(control) => self.external_control(*control),
            _ => {}
        }
    }

    /// The end of a track (`Tick`, `src/gui/app.rs:1412`): the playlist
    /// advances once, however many times the event repeats (quirk fix 28); the
    /// session is saved and the state persisted by the facade, and the new
    /// row scrolls into view unless the pointer is over the list.
    pub fn end_of_track(&self) -> Option<EndOfTrackOutcome> {
        let ctx = self.inner.services.ctx.clone();
        match self.with_playback(|cursor| playback::end_of_track(&ctx, cursor))? {
            Ok(outcome) => Some(outcome),
            Err(error) => {
                tracing::error!("the next song could not be played: {error}");
                None
            }
        }
    }

    /// The position moved (`Tick`, `src/gui/app.rs:1369-1405`): with 15 s or
    /// less left, the next song is preloaded once per track, with its gains,
    /// and its cover is fetched. Returns that cover's path.
    pub fn position_changed(&self, position_secs: f64, duration_secs: f64) -> Option<String> {
        let ctx = self.inner.services.ctx.clone();
        let cover = self
            .with_playback(|cursor| {
                playback::maybe_preload(&ctx, cursor, position_secs, duration_secs)
            })
            .flatten()?;
        let _ = self.inner.services.covers.request(&cover);
        Some(cover)
    }

    /// `Session::AutosaveDue`: saves the position (quirk fix 13). Returns the
    /// save's reply, `None` before the restore.
    pub fn autosave(&self) -> Option<Reply<()>> {
        let services = &self.inner.services;
        let reply = services.session.persist_now(&services.ctx)?;
        Some(self.logged(reply, "the playlist state could not be saved"))
    }

    /// A request from the system media controls (`ExternalControl`,
    /// `src/gui/app.rs:1855`), mapped to the transport by
    /// `logic::transport` (in the facade's `external_control`).
    pub fn external_control(&self, event: ExternalControlEvent) {
        let ctx = self.inner.services.ctx.clone();
        if let Some(Err(error)) =
            self.with_playback(|cursor| playback::external_control(&ctx, cursor, event))
        {
            tracing::error!("the media control could not be carried out: {error}");
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Logs a failure of `reply` once it resolves, and hands the reply back
    /// to the caller through a second one-shot (a test awaits it).
    fn logged<T: 'static>(&self, reply: Reply<T>, what: &'static str) -> Reply<T> {
        let (tx, rx) = reply_channel("playlist view-model");
        self.inner
            .services
            .runtime
            .spawn_local(async move {
                let result = reply.recv().await;
                if let Err(error) = &result {
                    tracing::warn!("{what}: {error}");
                }
                tx.send(result);
            })
            .detach();
        rx
    }

    /// Awaits `future` on the executor and runs `done` with its output. When
    /// `done` returns a notice and `window` names the window that started
    /// the operation, the notice is shown there as a toast.
    fn finish<T: 'static>(
        &self,
        window: ToastWindow,
        future: impl Future<Output = T> + 'static,
        done: impl FnOnce(T) -> Option<Notice> + 'static,
    ) {
        let runtime = &self.inner.services.runtime;
        match window {
            Some(window) => runtime
                .spawn_local_with(window, future, move |output, ctx| {
                    if let Some(notice) = done(output) {
                        super::notice::present(&notice, ctx);
                    }
                })
                .detach(),
            None => runtime
                .spawn_local(async move {
                    let _ = done(future.await);
                })
                .detach(),
        }
    }
}

/// The view-model behind `weak`, if it still lives.
fn upgrade(weak: &Weak<Inner>) -> Option<PlaylistViewModel> {
    weak.upgrade().map(|inner| PlaylistViewModel { inner })
}

/// A key of the cursor's playlist and songs (not their flags): it changes when
/// another playlist is shown or songs were added or removed.
fn content_key(cursor: &PlaybackCursor) -> u64 {
    let mut hasher = DefaultHasher::new();
    cursor.playlist_id().hash(&mut hasher);
    for group in cursor.groups() {
        group.first_item_id.hash(&mut hasher);
        for song in &group.songs {
            song.song_id.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The error text of a long operation that ended without its result, unless
/// it was cancelled.
fn failure(
    info: &LongOpInfo,
    completed: bool,
    result: Option<Result<(), FrontendError>>,
) -> Option<String> {
    if info.cancelled {
        return None;
    }
    let error = match result {
        Some(Err(error)) => error.to_string(),
        _ if completed => return None,
        _ => info.error.clone().unwrap_or_default(),
    };
    tracing::warn!("the operation {:?} failed: {error}", info.kind);
    Some(error)
}

#[cfg(test)]
mod tests;
