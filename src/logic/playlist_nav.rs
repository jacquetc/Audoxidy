//! Playlist navigation over folder groups: linear indexing, visible rows,
//! selection, collapsing, enabled flags, search filtering and playback order.
//!
//! # Linear index
//!
//! A playlist is a list of [`PlaylistFolderGroup`]s. Its items are numbered
//! with a *linear index*: each group contributes one separator followed by
//! its songs, so with groups of 2 and 3 songs the linear indices are
//!
//! ```text
//! 0 separator A   1 song A0   2 song A1   3 separator B   4 B0   5 B1   6 B2
//! ```
//!
//! Linear indices ignore collapsing: they only change when the groups (or
//! the search filter that selects them) change. Selections, the focus, the
//! range pivot and the playing song are all kept as linear indices.
//!
//! # Visible rows
//!
//! A collapsed group (keyed by its anchor, the `first_item_id`) shows only its
//! separator. *Visible rows* number what is drawn: every separator, plus the
//! songs of expanded groups. A song inside a collapsed group keeps its linear
//! index but has no row of its own; [`visible_row_of`] maps it to its
//! separator's row and [`item_y_bounds`] to its separator's pixel band. The
//! reverse mapping, [`linear_index_at_row`], always yields a visible item.
//!
//! All functions take the *active* groups (the search result when a filter
//! is applied, else the full list) unless they say otherwise.

use crate::db::database::{PlaylistFolderGroup, PlaylistSongRef, ShuffleSession};
use crate::logic::selection_modifiers::SelectModifiers;
use std::collections::{BTreeSet, HashSet};

/// Height (px) of a song row.
pub const SONG_ROW_HEIGHT: f32 = 46.0;

/// Height (px) of a folder separator row.
pub const FOLDER_SEPARATOR_HEIGHT: f32 = 46.0;

/// What a linear index points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistItemType {
    /// A group separator: `(group_idx, first_item_id)`.
    Separator(usize, i64), // group_idx, first_item_id
    /// A song: `(group_idx, song_local_idx, first_item_id)`.
    Song(usize, usize, i64), // group_idx, song_local_idx, first_item_id
}

impl PlaylistItemType {
    /// Anchor (`first_item_id`) of the item's group.
    pub fn anchor(&self) -> i64 {
        match self {
            PlaylistItemType::Separator(_, anchor) | PlaylistItemType::Song(_, _, anchor) => {
                *anchor
            }
        }
    }
}

// ============================================================
// Linear index
// ============================================================

/// All songs of all groups, in playlist order.
pub fn flatten_groups(groups: &[PlaylistFolderGroup]) -> Vec<&PlaylistSongRef> {
    groups.iter().flat_map(|g| g.songs.iter()).collect()
}

/// Number of linear items: one separator plus the songs of each group.
pub fn count_linear_items(groups: &[PlaylistFolderGroup]) -> usize {
    groups.iter().map(|g| 1 + g.songs.len()).sum()
}

/// The item at a linear index, or `None` past the end.
pub fn item_at(groups: &[PlaylistFolderGroup], linear_idx: usize) -> Option<PlaylistItemType> {
    let mut count = 0;
    for (group_idx, group) in groups.iter().enumerate() {
        if linear_idx == count {
            return Some(PlaylistItemType::Separator(group_idx, group.first_item_id));
        }
        let song_local_idx = linear_idx - count - 1;
        if song_local_idx < group.songs.len() {
            return Some(PlaylistItemType::Song(
                group_idx,
                song_local_idx,
                group.first_item_id,
            ));
        }
        count += 1 + group.songs.len();
    }
    None
}

/// The song at a linear index; `None` for a separator or past the end.
pub fn song_at(groups: &[PlaylistFolderGroup], linear_idx: usize) -> Option<&PlaylistSongRef> {
    let mut count = 0;
    for group in groups {
        if linear_idx == count {
            return None; // Separator
        }
        let song_local_idx = linear_idx - count - 1;
        if song_local_idx < group.songs.len() {
            return group.songs.get(song_local_idx);
        }
        count += 1 + group.songs.len();
    }
    None
}

/// Mutable access to the song at a linear index (see [`song_at`]).
pub fn song_at_mut(
    groups: &mut [PlaylistFolderGroup],
    linear_idx: usize,
) -> Option<&mut PlaylistSongRef> {
    let mut count = 0;
    for group in groups {
        if linear_idx == count {
            return None; // Separator
        }
        let song_local_idx = linear_idx - count - 1;
        if song_local_idx < group.songs.len() {
            return group.songs.get_mut(song_local_idx);
        }
        count += 1 + group.songs.len();
    }
    None
}

/// `(group_idx, is_separator)` of the item at a linear index.
pub fn group_info_at(groups: &[PlaylistFolderGroup], linear_idx: usize) -> Option<(usize, bool)> {
    let mut count = 0;
    for (gi, group) in groups.iter().enumerate() {
        if linear_idx == count {
            return Some((gi, true));
        }
        let song_local_idx = linear_idx - count - 1;
        if song_local_idx < group.songs.len() {
            return Some((gi, false));
        }
        count += 1 + group.songs.len();
    }
    None
}

/// Linear index of the separator of the first group with this anchor.
pub fn separator_index_for_anchor(groups: &[PlaylistFolderGroup], anchor: i64) -> Option<usize> {
    let mut count = 0;
    for group in groups {
        if group.first_item_id == anchor {
            return Some(count);
        }
        count += 1 + group.songs.len();
    }
    None
}

/// Linear index of the first occurrence of a song (by database song id).
pub fn linear_index_of_song_id(groups: &[PlaylistFolderGroup], song_id: i64) -> Option<usize> {
    let mut count = 0;
    for group in groups {
        count += 1; // Separator
        if let Some(idx) = group.songs.iter().position(|s| s.song_id == song_id) {
            return Some(count + idx);
        }
        count += group.songs.len();
    }
    None
}

/// Linear index of the first enabled song of the group whose separator is at
/// `separator_idx`; `None` if that is not a separator or no song is enabled.
pub fn first_enabled_song_in_group(
    groups: &[PlaylistFolderGroup],
    separator_idx: usize,
) -> Option<usize> {
    let mut current_idx = 0;
    for group in groups {
        if current_idx == separator_idx {
            return group
                .songs
                .iter()
                .position(|s| s.enabled)
                .map(|i| separator_idx + 1 + i);
        }
        current_idx += 1 + group.songs.len();
    }
    None
}

// ============================================================
// Visible rows
// ============================================================

/// The visible items in row order, as `(item, linear_idx)`: every separator,
/// and the songs of the groups that are not collapsed.
pub fn visible_items(
    groups: &[PlaylistFolderGroup],
    collapsed: &HashSet<i64>,
) -> Vec<(PlaylistItemType, usize)> {
    let mut visible = Vec::new();
    let mut global_idx = 0;

    for (group_idx, group) in groups.iter().enumerate() {
        visible.push((
            PlaylistItemType::Separator(group_idx, group.first_item_id),
            global_idx,
        ));
        let songs_len = group.songs.len();
        if !collapsed.contains(&group.first_item_id) {
            for song_i in 0..songs_len {
                visible.push((
                    PlaylistItemType::Song(group_idx, song_i, group.first_item_id),
                    global_idx + 1 + song_i,
                ));
            }
        }
        global_idx += 1 + songs_len;
    }
    visible
}

/// Visible row of a linear index. A song inside a collapsed group maps to the
/// row of its separator. `None` past the end.
pub fn visible_row_of(
    groups: &[PlaylistFolderGroup],
    collapsed: &HashSet<i64>,
    linear_idx: usize,
) -> Option<usize> {
    let mut count = 0;
    let mut row = 0;
    for group in groups {
        if linear_idx == count {
            return Some(row);
        }
        let len = group.songs.len();
        let in_group = linear_idx > count && linear_idx <= count + len;
        if collapsed.contains(&group.first_item_id) {
            if in_group {
                return Some(row);
            }
            row += 1;
        } else {
            if in_group {
                return Some(row + (linear_idx - count));
            }
            row += 1 + len;
        }
        count += 1 + len;
    }
    None
}

/// Linear index of the item drawn at a visible row; `None` past the end.
pub fn linear_index_at_row(
    groups: &[PlaylistFolderGroup],
    collapsed: &HashSet<i64>,
    row: usize,
) -> Option<usize> {
    let mut count = 0;
    let mut first_row = 0;
    for group in groups {
        let len = group.songs.len();
        let rows = if collapsed.contains(&group.first_item_id) {
            1
        } else {
            1 + len
        };
        if row < first_row + rows {
            return Some(count + (row - first_row));
        }
        first_row += rows;
        count += 1 + len;
    }
    None
}

/// Pixel band `(top, bottom)` of a linear index in the list. A song inside a
/// collapsed group gets its separator's band; past the end yields `(0, 0)`.
pub fn item_y_bounds(
    groups: &[PlaylistFolderGroup],
    collapsed: &HashSet<i64>,
    target_idx: usize,
) -> (f32, f32) {
    let mut current_y = 0.0;
    let mut count = 0;

    for group in groups {
        let sep_top = current_y;
        let sep_bottom = current_y + FOLDER_SEPARATOR_HEIGHT;

        if count == target_idx {
            return (sep_top, sep_bottom);
        }

        let is_collapsed = collapsed.contains(&group.first_item_id);
        current_y += FOLDER_SEPARATOR_HEIGHT;
        count += 1;

        if !is_collapsed {
            if target_idx >= count && target_idx < count + group.songs.len() {
                let song_inner_idx = target_idx - count;
                let song_top = current_y + (song_inner_idx as f32 * SONG_ROW_HEIGHT);
                return (song_top, song_top + SONG_ROW_HEIGHT);
            }
            current_y += group.songs.len() as f32 * SONG_ROW_HEIGHT;
            count += group.songs.len();
        } else {
            if target_idx >= count && target_idx < count + group.songs.len() {
                return (sep_top, sep_bottom);
            }
            count += group.songs.len();
        }
    }
    (0.0, 0.0)
}

// ============================================================
// Selection
// ============================================================

/// Direction of a vertical key move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalDir {
    /// One visible row up.
    Up,
    /// One visible row down.
    Down,
}

/// Selection of a playlist, in linear indices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaylistSelection {
    /// Selected items.
    pub selected: HashSet<usize>,
    /// Keyboard focus.
    pub focused: Option<usize>,
    /// Anchor of Shift range selections.
    pub pivot: Option<usize>,
}

impl PlaylistSelection {
    /// Clears the selection, the focus and the pivot.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.focused = None;
        self.pivot = None;
    }

    /// Selects only `linear_idx`, focusing it and making it the pivot.
    pub fn select_only(&mut self, linear_idx: usize) {
        self.selected.clear();
        self.selected.insert(linear_idx);
        self.focused = Some(linear_idx);
        self.pivot = Some(linear_idx);
    }

    /// A click on `linear_idx`:
    /// - toggle (Ctrl/Cmd): flip the item, focus it, make it the pivot;
    /// - range (Shift): select from the pivot to the item over the visible
    ///   order, or select only the item when there is no pivot;
    /// - neither: select only the item.
    pub fn click(
        &mut self,
        linear_idx: usize,
        modifiers: SelectModifiers,
        visible: &[(PlaylistItemType, usize)],
    ) {
        if modifiers.toggle {
            if self.selected.contains(&linear_idx) {
                self.selected.remove(&linear_idx);
            } else {
                self.selected.insert(linear_idx);
            }
            self.focused = Some(linear_idx);
            self.pivot = Some(linear_idx);
        } else if modifiers.range {
            if let Some(pivot) = self.pivot {
                self.select_range(pivot, linear_idx, visible);
            } else {
                self.select_only(linear_idx);
            }
        } else {
            self.select_only(linear_idx);
        }
    }

    /// Replaces the selection with the visible items between `start_idx` and
    /// `end_idx` (inclusive, in either order) and focuses `end_idx`. Does
    /// nothing unless both are visible. Hidden songs of collapsed groups in
    /// between are not selected.
    pub fn select_range(
        &mut self,
        start_idx: usize,
        end_idx: usize,
        visible: &[(PlaylistItemType, usize)],
    ) {
        let mut start_v = None;
        let mut end_v = None;
        for (v_idx, (_, g_idx)) in visible.iter().enumerate() {
            if *g_idx == start_idx {
                start_v = Some(v_idx);
            }
            if *g_idx == end_idx {
                end_v = Some(v_idx);
            }
        }

        if let (Some(s), Some(e)) = (start_v, end_v) {
            let (min, max) = if s < e { (s, e) } else { (e, s) };
            self.selected.clear();
            self.selected
                .extend(visible[min..=max].iter().map(|(_, g_idx)| *g_idx));
            self.focused = Some(end_idx);
        }
    }

    /// Up/Down key: moves the focus one visible row (clamped; the first row
    /// when the focus is not visible). With `extend` (Shift) the selection
    /// becomes the range from the pivot (set to the current row if missing);
    /// otherwise only the new row is selected.
    pub fn navigate(
        &mut self,
        dir: VerticalDir,
        extend: bool,
        visible: &[(PlaylistItemType, usize)],
    ) {
        if visible.is_empty() {
            return;
        }

        let current = self
            .focused
            .and_then(|f| visible.iter().position(|(_, g_idx)| *g_idx == f));
        let current_visible_idx = current.unwrap_or(0);

        let next_v_idx = match (dir, current) {
            (VerticalDir::Up, Some(cur)) => cur.saturating_sub(1),
            (VerticalDir::Down, Some(cur)) => (cur + 1).min(visible.len() - 1),
            (_, None) => 0,
        };
        let next_linear_idx = visible[next_v_idx].1;

        if extend {
            let pivot = self.pivot.unwrap_or(visible[current_visible_idx].1);
            self.pivot = Some(pivot);
            self.select_range(pivot, next_linear_idx, visible);
        } else {
            self.select_only(next_linear_idx);
        }
    }

    /// Selects every visible item and drops the pivot (the focus stays).
    pub fn select_all(&mut self, visible: &[(PlaylistItemType, usize)]) {
        self.selected.clear();
        self.selected
            .extend(visible.iter().map(|(_, g_idx)| *g_idx));
        self.pivot = None;
    }
}

/// Song ids of a selection, in playlist order and without duplicates. A
/// selected separator stands for every song of its group.
pub fn selected_song_ids(groups: &[PlaylistFolderGroup], selected: &HashSet<usize>) -> Vec<i64> {
    let mut song_linear_idxs = BTreeSet::new();

    for &idx in selected {
        match item_at(groups, idx) {
            Some(PlaylistItemType::Separator(_, anchor)) => {
                if let Some(pos) = separator_index_for_anchor(groups, anchor)
                    && let Some((gi, _)) = group_info_at(groups, pos)
                    && let Some(group) = groups.get(gi)
                {
                    song_linear_idxs.extend((0..group.songs.len()).map(|o| pos + 1 + o));
                }
            }
            Some(PlaylistItemType::Song(..)) => {
                song_linear_idxs.insert(idx);
            }
            None => {}
        }
    }

    song_linear_idxs
        .into_iter()
        .filter_map(|idx| song_at(groups, idx).map(|s| s.song_id))
        .collect()
}

// ============================================================
// Collapsing
// ============================================================

/// Collapses the group with this anchor, or expands it if it is collapsed.
pub fn toggle_group_collapsed(collapsed: &mut HashSet<i64>, anchor: i64) {
    if !collapsed.remove(&anchor) {
        collapsed.insert(anchor);
    }
}

/// Collapses every group if any is expanded, otherwise expands them all.
/// `groups` is the full list (not the search result).
pub fn toggle_all_groups(groups: &[PlaylistFolderGroup], collapsed: &mut HashSet<i64>) {
    let any_expanded = groups.iter().any(|g| !collapsed.contains(&g.first_item_id));
    if any_expanded {
        collapsed.extend(groups.iter().map(|g| g.first_item_id));
    } else {
        collapsed.clear();
    }
}

fn selected_anchors(groups: &[PlaylistFolderGroup], selected: &HashSet<usize>) -> HashSet<i64> {
    selected
        .iter()
        .filter_map(|&sel| item_at(groups, sel))
        .map(|info| info.anchor())
        .collect()
}

/// Left key: collapses every group touched by the selection. If the focus is
/// on a song, the focus and the selection move to its group's separator.
/// Returns whether anything was collapsed (the visible rows changed).
pub fn collapse_selected_groups(
    groups: &[PlaylistFolderGroup],
    collapsed: &mut HashSet<i64>,
    selection: &mut PlaylistSelection,
) -> bool {
    let anchors = selected_anchors(groups, &selection.selected);
    if anchors.is_empty() {
        return false;
    }
    collapsed.extend(anchors);

    if let Some(focused) = selection.focused
        && let Some(PlaylistItemType::Song(_, _, anchor)) = item_at(groups, focused)
        && let Some(separator) = separator_index_for_anchor(groups, anchor)
    {
        selection.select_only(separator);
    }
    true
}

/// Right key: expands every group touched by the selection. Returns whether
/// the selection touched any group.
pub fn expand_selected_groups(
    groups: &[PlaylistFolderGroup],
    collapsed: &mut HashSet<i64>,
    selection: &PlaylistSelection,
) -> bool {
    let anchors = selected_anchors(groups, &selection.selected);
    if anchors.is_empty() {
        return false;
    }
    for anchor in anchors {
        collapsed.remove(&anchor);
    }
    true
}

// ============================================================
// Enabled flags
// ============================================================
//
// `groups` is the full list and `filtered` the search result, when a filter
// is applied. A change is made in the active list; with a filter it is then
// mirrored into the full list: songs by song id (the first match of each
// group), groups by anchor (every song of the full group).

fn sync_song_enabled(groups: &mut [PlaylistFolderGroup], song_id: i64, enabled: bool) {
    for group in groups {
        if let Some(song) = group.songs.iter_mut().find(|s| s.song_id == song_id) {
            song.enabled = enabled;
        }
    }
}

fn sync_group_enabled(groups: &mut [PlaylistFolderGroup], anchor: i64, enabled: bool) {
    if let Some(group) = groups.iter_mut().find(|g| g.first_item_id == anchor) {
        for song in &mut group.songs {
            song.enabled = enabled;
        }
    }
}

/// Flips the enabled flag of the song at `linear_idx` (in the active list).
/// Returns the new state, or `None` if the index is not a song.
pub fn toggle_song_enabled(
    groups: &mut [PlaylistFolderGroup],
    filtered: Option<&mut [PlaylistFolderGroup]>,
    linear_idx: usize,
) -> Option<bool> {
    match filtered {
        Some(active) => {
            let song = song_at_mut(active, linear_idx)?;
            song.enabled = !song.enabled;
            let (song_id, enabled) = (song.song_id, song.enabled);
            sync_song_enabled(groups, song_id, enabled);
            Some(enabled)
        }
        None => {
            let song = song_at_mut(groups, linear_idx)?;
            song.enabled = !song.enabled;
            Some(song.enabled)
        }
    }
}

/// Sets the enabled flag of the song at `linear_idx` (in the active list).
pub fn set_song_enabled(
    groups: &mut [PlaylistFolderGroup],
    filtered: Option<&mut [PlaylistFolderGroup]>,
    linear_idx: usize,
    enabled: bool,
) {
    match filtered {
        Some(active) => {
            if let Some(song) = song_at_mut(active, linear_idx) {
                song.enabled = enabled;
                let song_id = song.song_id;
                sync_song_enabled(groups, song_id, enabled);
            }
        }
        None => {
            if let Some(song) = song_at_mut(groups, linear_idx) {
                song.enabled = enabled;
            }
        }
    }
}

/// Applies `state(group)` to every song of the group whose separator is at
/// `linear_idx`, then mirrors it into the full list by anchor.
fn change_group_enabled(
    groups: &mut [PlaylistFolderGroup],
    filtered: Option<&mut [PlaylistFolderGroup]>,
    linear_idx: usize,
    state: impl Fn(&PlaylistFolderGroup) -> bool,
) {
    let has_filter = filtered.is_some();
    let active: &mut [PlaylistFolderGroup] = match filtered {
        Some(f) => f,
        None => &mut *groups,
    };
    let Some((gi, true)) = group_info_at(active, linear_idx) else {
        return;
    };
    let Some(group) = active.get_mut(gi) else {
        return;
    };
    let enabled = state(group);
    for song in &mut group.songs {
        song.enabled = enabled;
    }
    let anchor = group.first_item_id;
    if has_filter {
        sync_group_enabled(groups, anchor, enabled);
    }
}

/// Toggles a whole group from its separator at `linear_idx`: the group becomes
/// enabled if every song is disabled, otherwise disabled. A song index does
/// nothing.
pub fn toggle_group_enabled(
    groups: &mut [PlaylistFolderGroup],
    filtered: Option<&mut [PlaylistFolderGroup]>,
    linear_idx: usize,
) {
    change_group_enabled(groups, filtered, linear_idx, |g| {
        !g.songs.iter().any(|s| s.enabled)
    });
}

/// Sets the enabled flag of a whole group from its separator at `linear_idx`.
/// A song index does nothing.
pub fn set_group_enabled(
    groups: &mut [PlaylistFolderGroup],
    filtered: Option<&mut [PlaylistFolderGroup]>,
    linear_idx: usize,
    enabled: bool,
) {
    change_group_enabled(groups, filtered, linear_idx, |_| enabled);
}

/// Target state of a bulk enable toggle over `targets`: `true` (enable all)
/// if any targeted song, or any song of a targeted group, is disabled;
/// otherwise `false` (disable all). Songs are looked up in `active` by linear
/// index; separators are resolved by anchor in `all_groups`, the full list.
pub fn bulk_enable_target(
    all_groups: &[PlaylistFolderGroup],
    active: &[PlaylistFolderGroup],
    targets: impl IntoIterator<Item = usize>,
) -> bool {
    targets.into_iter().any(|idx| match item_at(active, idx) {
        Some(PlaylistItemType::Song(..)) => song_at(active, idx).is_some_and(|s| !s.enabled),
        Some(PlaylistItemType::Separator(_, anchor)) => all_groups
            .iter()
            .find(|g| g.first_item_id == anchor)
            .is_some_and(|g| g.songs.iter().any(|s| !s.enabled)),
        None => false,
    })
}

// ============================================================
// Search filter
// ============================================================

/// Groups that match a search, each keeping only its matching songs (with its
/// total duration recomputed); groups left empty are dropped. A song matches
/// when its title, artist, album artist or album title contains the query,
/// case-insensitively. `None` for an empty query and when nothing matches
/// (the full list is then shown).
pub fn filter_groups(
    groups: &[PlaylistFolderGroup],
    query: &str,
) -> Option<Vec<PlaylistFolderGroup>> {
    if query.is_empty() {
        return None;
    }
    let query = query.to_lowercase();

    let filtered: Vec<PlaylistFolderGroup> = groups
        .iter()
        .filter_map(|group| {
            let matching_songs: Vec<PlaylistSongRef> = group
                .songs
                .iter()
                .filter(|s| {
                    s.title.to_lowercase().contains(&query)
                        || s.artist_name.to_lowercase().contains(&query)
                        || s.album_artist_name.to_lowercase().contains(&query)
                        || s.album_title.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();

            if matching_songs.is_empty() {
                None
            } else {
                let mut g = group.clone();
                g.songs = matching_songs;
                g.total_duration = g.songs.iter().map(|s| s.duration).sum();
                Some(g)
            }
        })
        .collect();

    if filtered.is_empty() {
        None
    } else {
        Some(filtered)
    }
}

// ============================================================
// Playback order
// ============================================================

/// Repeat mode of a playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    /// Stop at the end.
    #[default]
    Off,
    /// Wrap around (sequential) or re-shuffle (shuffle) at the end.
    All,
    /// Replay the current song.
    One,
}

impl RepeatMode {
    /// From the persisted number: 1 = All, 2 = One, anything else Off.
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => RepeatMode::All,
            2 => RepeatMode::One,
            _ => RepeatMode::Off,
        }
    }

    /// The persisted number: Off 0, All 1, One 2.
    pub fn as_u8(self) -> u8 {
        match self {
            RepeatMode::Off => 0,
            RepeatMode::All => 1,
            RepeatMode::One => 2,
        }
    }

    /// Next mode of the repeat button: Off → All → One → Off.
    pub fn cycled(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
}

/// What playing the next song does, decided without changing any state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextStep {
    /// Repeat One: play the item at this linear index again.
    Replay(usize),
    /// Shuffle: play this song; the session then records it in its history
    /// and continues from `next_position`.
    Shuffle {
        /// Linear index of the song.
        linear_idx: usize,
        /// Its song id.
        song_id: i64,
        /// Session position after it.
        next_position: usize,
    },
    /// Sequential: play the item at this linear index.
    Sequential(usize),
    /// The shuffle order is exhausted under Repeat All: draw a new session,
    /// then decide again.
    Reshuffle,
    /// Nothing to play.
    Stop,
}

impl NextStep {
    /// Linear index this step plays, if it is already known.
    pub fn target(&self) -> Option<usize> {
        match *self {
            NextStep::Replay(idx) | NextStep::Sequential(idx) => Some(idx),
            NextStep::Shuffle { linear_idx, .. } => Some(linear_idx),
            NextStep::Reshuffle | NextStep::Stop => None,
        }
    }
}

/// First enabled song after `after` (from the start when `None`). Past the
/// end the search wraps once, only with `repeat_all`; the result can then be
/// `after` itself.
pub fn find_next_enabled(
    groups: &[PlaylistFolderGroup],
    after: Option<usize>,
    repeat_all: bool,
) -> Option<usize> {
    let total = count_linear_items(groups);
    if total == 0 {
        return None;
    }

    let mut idx = after.map_or(0, |current| current.wrapping_add(1));
    let mut wrapped = false;

    loop {
        if idx >= total {
            if wrapped || !repeat_all {
                return None;
            }
            idx = 0;
            wrapped = true;
        }
        if song_at(groups, idx).is_some_and(|s| s.enabled) {
            return Some(idx);
        }
        idx += 1;
    }
}

/// Last enabled song of the playlist.
pub fn find_last_enabled(groups: &[PlaylistFolderGroup]) -> Option<usize> {
    (0..count_linear_items(groups))
        .rev()
        .find(|&idx| song_at(groups, idx).is_some_and(|s| s.enabled))
}

/// The next step from the playing song, without changing anything:
/// - Repeat One replays the playing item (`Stop` when nothing plays);
/// - shuffle takes the first song of the session order, from its position,
///   that is enabled (looked up by song id); at the end it is `Reshuffle`
///   under Repeat All and `Stop` otherwise (also without a session);
/// - otherwise the next enabled song in playlist order (from the start when
///   nothing plays), wrapping only under Repeat All.
pub fn peek_next(
    groups: &[PlaylistFolderGroup],
    playing: Option<usize>,
    repeat: RepeatMode,
    shuffle_active: bool,
    session: Option<&ShuffleSession>,
) -> NextStep {
    if repeat == RepeatMode::One {
        return playing.map_or(NextStep::Stop, NextStep::Replay);
    }

    if shuffle_active {
        if let Some(session) = session {
            let order = session
                .shuffle_order
                .get(session.current_position..)
                .unwrap_or(&[]);
            for (offset, &song_id) in order.iter().enumerate() {
                if let Some(linear_idx) = linear_index_of_song_id(groups, song_id)
                    && song_at(groups, linear_idx).is_some_and(|s| s.enabled)
                {
                    return NextStep::Shuffle {
                        linear_idx,
                        song_id,
                        next_position: session.current_position + offset + 1,
                    };
                }
            }
        }
        return if repeat == RepeatMode::All {
            NextStep::Reshuffle
        } else {
            NextStep::Stop
        };
    }

    find_next_enabled(groups, playing, repeat == RepeatMode::All)
        .map_or(NextStep::Stop, NextStep::Sequential)
}

/// The song the next step would play, for preloading: `None` when nothing
/// plays, at a re-shuffle (the new order is not known yet) and at the end.
pub fn next_song_ref<'g>(
    groups: &'g [PlaylistFolderGroup],
    playing: Option<usize>,
    repeat: RepeatMode,
    shuffle_active: bool,
    session: Option<&ShuffleSession>,
) -> Option<&'g PlaylistSongRef> {
    let current = playing?;
    peek_next(groups, Some(current), repeat, shuffle_active, session)
        .target()
        .and_then(|idx| song_at(groups, idx))
}

/// Records a shuffle step in the session: the song joins the history and the
/// position moves past it.
pub fn commit_shuffle_step(session: &mut ShuffleSession, song_id: i64, next_position: usize) {
    session.history.push(song_id);
    session.current_position = next_position;
}

/// Plays the next song: applies [`peek_next`] and updates the state (the
/// playing index, the shuffle history and position; a re-shuffle draws a new
/// session with `rng` and decides again). Returns the linear index of the
/// song to load and play, or `None` when nothing plays.
pub fn advance_next(
    groups: &[PlaylistFolderGroup],
    playing: &mut Option<usize>,
    repeat: RepeatMode,
    shuffle_active: &mut bool,
    session: &mut Option<ShuffleSession>,
    rng: &mut fastrand::Rng,
) -> Option<usize> {
    match peek_next(groups, *playing, repeat, *shuffle_active, session.as_ref()) {
        NextStep::Replay(idx) => song_at(groups, idx).map(|_| idx),
        NextStep::Shuffle {
            linear_idx,
            song_id,
            next_position,
        } => {
            if let Some(session) = session.as_mut() {
                commit_shuffle_step(session, song_id, next_position);
            }
            *playing = Some(linear_idx);
            Some(linear_idx)
        }
        NextStep::Sequential(idx) => {
            song_at(groups, idx)?;
            *playing = Some(idx);
            Some(idx)
        }
        NextStep::Reshuffle => {
            activate_shuffle(groups, *playing, shuffle_active, session, rng);
            let has_order = session
                .as_ref()
                .is_some_and(|s| !s.shuffle_order.is_empty());
            if has_order {
                advance_next(groups, playing, repeat, shuffle_active, session, rng)
            } else {
                None
            }
        }
        NextStep::Stop => None,
    }
}

/// Goes back one song and returns the linear index to load and play.
///
/// In shuffle, with more than one song in the history, the current song is
/// popped (and the position moved back one) and the previous one is played.
/// Otherwise (or if that song is no longer in the list) the previous enabled
/// song in playlist order is used; from linear index 0, or when nothing plays,
/// it wraps to the last enabled song only under Repeat All. The caller sets
/// the playing index to the result.
pub fn step_prev(
    groups: &[PlaylistFolderGroup],
    playing: Option<usize>,
    repeat: RepeatMode,
    shuffle_active: bool,
    session: Option<&mut ShuffleSession>,
) -> Option<usize> {
    if shuffle_active
        && let Some(session) = session
        && session.history.len() > 1
    {
        session.history.pop(); // Drop the current song
        if session.current_position > 0 {
            session.current_position -= 1;
        }
        if let Some(&prev_id) = session.history.last()
            && let Some(idx) = linear_index_of_song_id(groups, prev_id)
            && song_at(groups, idx).is_some()
        {
            return Some(idx);
        }
    }

    let current = playing.unwrap_or(0);
    if current == 0 {
        return if repeat == RepeatMode::All {
            find_last_enabled(groups)
        } else {
            None
        };
    }
    (0..current)
        .rev()
        .find(|&idx| song_at(groups, idx).is_some_and(|s| s.enabled))
}

/// A new shuffle session over the enabled songs: a Fisher–Yates shuffle of
/// their ids drawn from `rng`, with the playing song removed from the order
/// and seeded as the history. The id is `sess-<random u64>`, drawn after the
/// shuffle. `None` when no song is enabled.
pub fn new_shuffle_session(
    groups: &[PlaylistFolderGroup],
    playing: Option<usize>,
    rng: &mut fastrand::Rng,
) -> Option<ShuffleSession> {
    let mut songs: Vec<i64> = groups
        .iter()
        .flat_map(|g| g.songs.iter())
        .filter(|s| s.enabled)
        .map(|s| s.song_id)
        .collect();
    if songs.is_empty() {
        return None;
    }

    // Fisher–Yates
    for i in (1..songs.len()).rev() {
        let j = rng.usize(0..=i);
        songs.swap(i, j);
    }

    let current_song_id = playing
        .and_then(|idx| song_at(groups, idx))
        .map(|s| s.song_id);
    if let Some(sid) = current_song_id {
        songs.retain(|&id| id != sid);
    }

    Some(ShuffleSession {
        session_id: format!("sess-{}", rng.u64(..)),
        shuffle_order: songs,
        current_position: 0,
        history: current_song_id.into_iter().collect(),
    })
}

/// Turns shuffle on with a new session. With no enabled song, shuffle is
/// turned off instead and the previous session is left as it was.
pub fn activate_shuffle(
    groups: &[PlaylistFolderGroup],
    playing: Option<usize>,
    shuffle_active: &mut bool,
    session: &mut Option<ShuffleSession>,
    rng: &mut fastrand::Rng,
) {
    match new_shuffle_session(groups, playing, rng) {
        Some(new_session) => {
            *session = Some(new_session);
            *shuffle_active = true;
        }
        None => *shuffle_active = false,
    }
}

/// Records a song started by hand (double click, Enter) in a shuffle session:
/// it joins the history and is removed from the rest of the order, so that it
/// does not come up again soon.
pub fn record_manual_play(session: &mut ShuffleSession, song_id: i64) {
    session.history.push(song_id);
    if session.current_position < session.shuffle_order.len()
        && let Some(idx) = session.shuffle_order[session.current_position..]
            .iter()
            .position(|&x| x == song_id)
    {
        session.shuffle_order.remove(session.current_position + idx);
    }
}

#[cfg(test)]
mod tests;
