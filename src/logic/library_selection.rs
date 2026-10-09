//! Multi-selection in the library list views: clicks with modifiers, range
//! expansion, select-all, marquee, the expansion of a selection to songs, key
//! navigation over the visible items, and the Left/Right collapse helpers.
//!
//! Extracted from the Iced `LibraryManager` (`handle_click`, `select_range`,
//! `select_all`, `select_marquee`, `get_selected_songs`, `handle_key_nav`,
//! `get_artist_to_toggle`, `get_album_to_toggle`, `get_bulk_toggles`). The
//! library structure an operation needs (songs of an artist or album,
//! collapsed state, visible order) is passed in; nothing here owns library data.

use crate::db::database::SongData;
use crate::logic::library_geometry::ContentRect;
use crate::logic::library_groups::{
    AlbumEntry, ArtistGroup, album_composite_id_for_song, songs_for_album_id,
};
use crate::logic::selection_modifiers::SelectModifiers;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::sync::Arc;

/// An item of the library list views.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LibraryListItem {
    /// An artist header, by group name.
    Artist(String),
    /// An album, by composite id `"{artist}|{album_hash}"` (see
    /// [`album_composite_id`](crate::logic::library_groups::album_composite_id)).
    Album(String),
    /// A song, by database id.
    Song(i64),
}

/// A navigation key of the library and playlist views.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LibraryNavDir {
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left (collapse in the list views).
    Left,
    /// Arrow right (expand in the list views).
    Right,
    /// No movement.
    None,
}

/// Anything that names a [`LibraryListItem`]: the item itself, or an item
/// paired with extra data such as its rectangle.
pub trait AsListItem {
    /// The item named by `self`.
    fn list_item(&self) -> &LibraryListItem;
}

impl AsListItem for LibraryListItem {
    fn list_item(&self) -> &LibraryListItem {
        self
    }
}

impl<T> AsListItem for (LibraryListItem, T) {
    fn list_item(&self) -> &LibraryListItem {
        &self.0
    }
}

/// The library structure that selection operations consult.
pub trait LibraryStructure {
    /// Whether the artist header `artist` is collapsed.
    fn is_artist_collapsed(&self, artist: &str) -> bool;
    /// Whether the album `album_id` (composite id) is collapsed.
    fn is_album_collapsed(&self, album_id: &str) -> bool;
    /// Ids of every song of the artist group `artist`, or `None` if there is no such group.
    fn artist_song_ids(&self, artist: &str) -> Option<Vec<i64>>;
    /// Ids of every song of the album `album_id`, or `None` if the album is unknown.
    fn album_song_ids(&self, album_id: &str) -> Option<Vec<i64>>;
}

/// [`LibraryStructure`] over borrowed library data, as the managers hold it.
#[derive(Debug, Clone, Copy)]
pub struct LibraryData<'a> {
    /// Artist groups of the current (filtered) songs.
    pub artist_groups: &'a [ArtistGroup],
    /// The current songs: the filtered songs when a filter or search is active,
    /// all songs otherwise. `None` while nothing is loaded.
    pub songs: Option<&'a [Arc<SongData>]>,
    /// The album cache used to resolve composite album ids. `None` while nothing is loaded.
    pub cached_albums: Option<&'a [AlbumEntry]>,
    /// Names of the collapsed artist headers.
    pub collapsed_artists: &'a HashSet<String>,
    /// Composite ids of the collapsed albums.
    pub collapsed_albums: &'a HashSet<String>,
}

impl LibraryStructure for LibraryData<'_> {
    fn is_artist_collapsed(&self, artist: &str) -> bool {
        self.collapsed_artists.contains(artist)
    }

    fn is_album_collapsed(&self, album_id: &str) -> bool {
        self.collapsed_albums.contains(album_id)
    }

    fn artist_song_ids(&self, artist: &str) -> Option<Vec<i64>> {
        self.artist_groups
            .iter()
            .find(|g| g.name == artist)
            .map(|g| g.songs.iter().map(|s| s.id).collect())
    }

    fn album_song_ids(&self, album_id: &str) -> Option<Vec<i64>> {
        let songs = self.songs?;
        let albums = self.cached_albums?;
        songs_for_album_id(album_id, songs, albums).map(|v| v.iter().map(|s| s.id).collect())
    }
}

/// The selection state of a library list: selected items, keyboard focus and
/// the pivot that Shift ranges start from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySelection {
    /// The selected items.
    pub selected: HashSet<LibraryListItem>,
    /// The item with keyboard focus.
    pub focused: Option<LibraryListItem>,
    /// The anchor of Shift ranges.
    pub pivot: Option<LibraryListItem>,
}

impl LibrarySelection {
    /// Selects only `item`, focusing it and making it the pivot.
    pub fn select_only(&mut self, item: LibraryListItem) {
        self.selected.clear();
        self.selected.insert(item.clone());
        self.focused = Some(item.clone());
        self.pivot = Some(item);
    }

    /// Clears the selection, the focus and the pivot.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.focused = None;
        self.pivot = None;
    }

    /// A click on `item`.
    ///
    /// - Toggle (Ctrl/Cmd): flips `item` in the selection, focuses it and makes it the pivot.
    /// - Range (Shift): with a pivot, selects the range pivot..`item` (see
    ///   [`select_range`](Self::select_range)) and leaves focus and pivot unchanged;
    ///   without a pivot, behaves like a plain click.
    /// - Plain click: selects only `item`.
    ///
    /// `visible_items` yields the visible order used for ranges; it is only
    /// called for a range click.
    pub fn click<I: AsListItem>(
        &mut self,
        item: LibraryListItem,
        modifiers: SelectModifiers,
        visible_items: impl FnOnce() -> Vec<I>,
        structure: &impl LibraryStructure,
    ) {
        if modifiers.toggle {
            if !self.selected.remove(&item) {
                self.selected.insert(item.clone());
            }
            self.focused = Some(item.clone());
            self.pivot = Some(item);
        } else if modifiers.range {
            if let Some(pivot) = self.pivot.clone() {
                self.select_range(&visible_items(), &pivot, &item, structure);
            } else {
                self.select_only(item);
            }
        } else {
            self.select_only(item);
        }
    }

    /// Replaces the selection with the visible items between `start` and `end`
    /// (inclusive, in either order), expanding headers to their songs.
    ///
    /// When an item occurs several times in `items`, its last occurrence is
    /// used. An artist or album header adds all its songs when it is one of the
    /// two ends of the range, or when it is collapsed (its songs are not
    /// visible); an expanded header in the middle adds nothing extra because
    /// its visible songs are already in the range. Does nothing when `start` or
    /// `end` is not visible.
    pub fn select_range<I: AsListItem>(
        &mut self,
        items: &[I],
        start: &LibraryListItem,
        end: &LibraryListItem,
        structure: &impl LibraryStructure,
    ) {
        let mut start_v = None;
        let mut end_v = None;
        for (i, it) in items.iter().enumerate() {
            if it.list_item() == start {
                start_v = Some(i);
            }
            if it.list_item() == end {
                end_v = Some(i);
            }
        }
        let (Some(s), Some(e)) = (start_v, end_v) else {
            return;
        };
        let (min, max) = if s < e { (s, e) } else { (e, s) };

        self.selected.clear();
        for it in &items[min..=max] {
            let item = it.list_item();
            self.selected.insert(item.clone());
            let is_boundary = item == start || item == end;
            let songs = match item {
                LibraryListItem::Artist(name)
                    if is_boundary || structure.is_artist_collapsed(name) =>
                {
                    structure.artist_song_ids(name)
                }
                LibraryListItem::Album(id) if is_boundary || structure.is_album_collapsed(id) => {
                    structure.album_song_ids(id)
                }
                _ => None,
            };
            for id in songs.unwrap_or_default() {
                self.selected.insert(LibraryListItem::Song(id));
            }
        }
    }

    /// Selects every item of `items`; clears focus and pivot.
    pub fn select_all(&mut self, items: impl IntoIterator<Item = LibraryListItem>) {
        self.selected.clear();
        self.selected.extend(items);
        self.pivot = None;
        self.focused = None;
    }

    /// Applies a marquee drag over items laid out in content space.
    ///
    /// Without modifiers the selection is replaced; with Shift the hit items are
    /// added; with Ctrl/Cmd each hit item is toggled. Focus and pivot are left
    /// unchanged.
    pub fn select_marquee(
        &mut self,
        item_rects: &[(LibraryListItem, ContentRect)],
        marquee: ContentRect,
        modifiers: SelectModifiers,
    ) {
        if !modifiers.range && !modifiers.toggle {
            self.selected.clear();
        }
        for (item, bounds) in item_rects {
            if marquee.intersects(bounds) {
                if modifiers.toggle && self.selected.contains(item) {
                    self.selected.remove(item);
                } else {
                    self.selected.insert(item.clone());
                }
            }
        }
    }

    /// Moves the keyboard selection from `items[current]` to
    /// `items[target]` (clamped to the last item) and returns the new item.
    ///
    /// With Shift the range from the pivot (or from the current item when there
    /// is no pivot, which then becomes the pivot) to the new item is selected;
    /// otherwise only the new item is selected and becomes the pivot. The new
    /// item is focused. Returns `None` when `items` is empty.
    pub fn navigate<I: AsListItem>(
        &mut self,
        items: &[I],
        current: usize,
        target: usize,
        modifiers: SelectModifiers,
        structure: &impl LibraryStructure,
    ) -> Option<LibraryListItem> {
        let last = items.len().checked_sub(1)?;
        let new_item = items[target.min(last)].list_item().clone();
        if modifiers.range {
            let pivot = match self.pivot.clone() {
                Some(pivot) => pivot,
                None => items[current.min(last)].list_item().clone(),
            };
            self.pivot = Some(pivot.clone());
            self.select_range(items, &pivot, &new_item, structure);
        } else {
            self.selected.clear();
            self.selected.insert(new_item.clone());
            self.pivot = Some(new_item.clone());
        }
        self.focused = Some(new_item.clone());
        Some(new_item)
    }
}

/// Index of the current item for keyboard navigation: the focused item if it
/// is visible, else the first item accepted by `fallback`, else 0.
pub fn current_index<I: AsListItem>(
    items: &[I],
    focused: Option<&LibraryListItem>,
    fallback: impl Fn(&LibraryListItem) -> bool,
) -> usize {
    focused
        .and_then(|f| items.iter().position(|it| it.list_item() == f))
        .or_else(|| items.iter().position(|it| fallback(it.list_item())))
        .unwrap_or(0)
}

/// Target index of a navigation key in the list views: Up and Down move by
/// one item, clamped to the list; any other key stays put.
pub fn list_nav_target(current: usize, len: usize, dir: LibraryNavDir) -> usize {
    match dir {
        LibraryNavDir::Up => current.saturating_sub(1),
        LibraryNavDir::Down => (current + 1).min(len.saturating_sub(1)),
        _ => current,
    }
}

/// Ids of the songs a selection stands for.
///
/// Explicitly selected songs are always included. A selected album or artist
/// adds all its songs only when none of them is explicitly selected (so
/// selecting a header and then picking some of its songs means "those songs").
pub fn selected_song_ids(
    selected: &HashSet<LibraryListItem>,
    structure: &impl LibraryStructure,
) -> HashSet<i64> {
    let explicit: HashSet<i64> = selected
        .iter()
        .filter_map(|item| match item {
            LibraryListItem::Song(id) => Some(*id),
            _ => None,
        })
        .collect();
    let mut result = explicit.clone();

    let mut expand = |songs: Option<Vec<i64>>| {
        if let Some(songs) = songs
            && !songs.iter().any(|id| explicit.contains(id))
        {
            result.extend(songs);
        }
    };
    for item in selected {
        if let LibraryListItem::Album(id) = item {
            expand(structure.album_song_ids(id));
        }
    }
    for item in selected {
        if let LibraryListItem::Artist(name) = item {
            expand(structure.artist_song_ids(name));
        }
    }
    result
}

/// The songs a selection stands for (see [`selected_song_ids`]), in the order
/// of `songs` (the current filtered songs).
pub fn selected_songs<S: Clone + Borrow<SongData>>(
    selected: &HashSet<LibraryListItem>,
    structure: &impl LibraryStructure,
    songs: &[S],
) -> Vec<S> {
    if selected.is_empty() {
        return Vec::new();
    }
    let ids = selected_song_ids(selected, structure);
    songs
        .iter()
        .filter(|s| ids.contains(&(*s).borrow().id))
        .cloned()
        .collect()
}

/// Whether a Left/Right key should toggle a header: Left collapses an
/// expanded one, Right expands a collapsed one.
pub fn toggle_wanted(dir: LibraryNavDir, is_collapsed: bool) -> bool {
    (dir == LibraryNavDir::Left && !is_collapsed) || (dir == LibraryNavDir::Right && is_collapsed)
}

/// The artist header a Left/Right key toggles, from the legacy single selection.
///
/// The artist is the selected header, else the `artist` tag (or, failing that,
/// the `album_artist` tag) of the selected song. Returns it only when
/// [`toggle_wanted`].
pub fn artist_to_toggle(
    selected_header: Option<&str>,
    selected_song: Option<&SongData>,
    dir: LibraryNavDir,
    structure: &impl LibraryStructure,
) -> Option<String> {
    let artist = match selected_header {
        Some(header) => Some(header.to_string()),
        None => selected_song.and_then(|s| {
            s.artist
                .as_ref()
                .map(|a| a.to_string())
                .or_else(|| s.album_artist.as_ref().map(|a| a.to_string()))
        }),
    }?;
    toggle_wanted(dir, structure.is_artist_collapsed(&artist)).then_some(artist)
}

/// The album a Left/Right key toggles, from the legacy single selection.
///
/// The album is the selected album, else the album of the selected song (see
/// [`album_composite_id_for_song`]). Returns it only when [`toggle_wanted`].
pub fn album_to_toggle(
    selected_album: Option<&str>,
    selected_song: Option<&SongData>,
    cached_albums: Option<&[AlbumEntry]>,
    dir: LibraryNavDir,
    structure: &impl LibraryStructure,
) -> Option<String> {
    let album_id = match selected_album {
        Some(album) => Some(album.to_string()),
        None => selected_song.map(|s| album_composite_id_for_song(s, cached_albums)),
    }?;
    toggle_wanted(dir, structure.is_album_collapsed(&album_id)).then_some(album_id)
}

/// Albums and artists a Left/Right key collapses or expands in bulk.
///
/// Processes the selected items, or the focused item when nothing is
/// selected. A selected song stands for its album (`album_id_for_song`). Each
/// header is returned once, only when [`toggle_wanted`]; the order of each
/// list is unspecified.
pub fn bulk_toggles(
    selected: &HashSet<LibraryListItem>,
    focused: Option<&LibraryListItem>,
    dir: LibraryNavDir,
    structure: &impl LibraryStructure,
    album_id_for_song: impl Fn(i64) -> Option<String>,
) -> (Vec<String>, Vec<String>) {
    let mut albums_to_toggle = HashSet::new();
    let mut artists_to_toggle = HashSet::new();

    let items: Vec<&LibraryListItem> = if !selected.is_empty() {
        selected.iter().collect()
    } else {
        focused.into_iter().collect()
    };

    for item in items {
        match item {
            LibraryListItem::Artist(name) => {
                if toggle_wanted(dir, structure.is_artist_collapsed(name)) {
                    artists_to_toggle.insert(name.clone());
                }
            }
            LibraryListItem::Album(id) => {
                if toggle_wanted(dir, structure.is_album_collapsed(id)) {
                    albums_to_toggle.insert(id.clone());
                }
            }
            LibraryListItem::Song(id) => {
                if let Some(album_id) = album_id_for_song(*id)
                    && toggle_wanted(dir, structure.is_album_collapsed(&album_id))
                {
                    albums_to_toggle.insert(album_id);
                }
            }
        }
    }

    (
        albums_to_toggle.into_iter().collect(),
        artists_to_toggle.into_iter().collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A hand-built structure: artists and albums with their song ids.
    #[derive(Default)]
    struct FakeStructure {
        artists: HashMap<String, Vec<i64>>,
        albums: HashMap<String, Vec<i64>>,
        collapsed_artists: HashSet<String>,
        collapsed_albums: HashSet<String>,
    }

    impl LibraryStructure for FakeStructure {
        fn is_artist_collapsed(&self, artist: &str) -> bool {
            self.collapsed_artists.contains(artist)
        }
        fn is_album_collapsed(&self, album_id: &str) -> bool {
            self.collapsed_albums.contains(album_id)
        }
        fn artist_song_ids(&self, artist: &str) -> Option<Vec<i64>> {
            self.artists.get(artist).cloned()
        }
        fn album_song_ids(&self, album_id: &str) -> Option<Vec<i64>> {
            self.albums.get(album_id).cloned()
        }
    }

    fn artist(name: &str) -> LibraryListItem {
        LibraryListItem::Artist(name.to_string())
    }

    fn album(id: &str) -> LibraryListItem {
        LibraryListItem::Album(id.to_string())
    }

    fn song(id: i64) -> LibraryListItem {
        LibraryListItem::Song(id)
    }

    fn set(items: &[LibraryListItem]) -> HashSet<LibraryListItem> {
        items.iter().cloned().collect()
    }

    const PLAIN: SelectModifiers = SelectModifiers {
        toggle: false,
        range: false,
    };
    const TOGGLE: SelectModifiers = SelectModifiers {
        toggle: true,
        range: false,
    };
    const RANGE: SelectModifiers = SelectModifiers {
        toggle: false,
        range: true,
    };

    /// Two artists: A (album A|1 with songs 1, 2; album A|2 with song 3) and
    /// B (album B|1 with songs 4, 5).
    fn structure() -> FakeStructure {
        let mut s = FakeStructure::default();
        s.artists.insert("A".into(), vec![1, 2, 3]);
        s.artists.insert("B".into(), vec![4, 5]);
        s.albums.insert("A|1".into(), vec![1, 2]);
        s.albums.insert("A|2".into(), vec![3]);
        s.albums.insert("B|1".into(), vec![4, 5]);
        s
    }

    /// The visible order with everything expanded.
    fn expanded_items() -> Vec<LibraryListItem> {
        vec![
            artist("A"),
            album("A|1"),
            song(1),
            song(2),
            album("A|2"),
            song(3),
            artist("B"),
            album("B|1"),
            song(4),
            song(5),
        ]
    }

    #[test]
    fn a_plain_click_selects_focuses_and_pivots_on_the_item() {
        let mut sel = LibrarySelection::default();
        sel.selected.insert(song(9));
        sel.click(song(1), PLAIN, expanded_items, &structure());
        assert_eq!(sel.selected, set(&[song(1)]));
        assert_eq!(sel.focused, Some(song(1)));
        assert_eq!(sel.pivot, Some(song(1)));
    }

    #[test]
    fn a_toggle_click_flips_the_item_and_moves_focus_and_pivot() {
        let items = expanded_items();
        let mut sel = LibrarySelection::default();
        sel.click(song(1), PLAIN, || items.clone(), &structure());
        sel.click(song(4), TOGGLE, || items.clone(), &structure());
        assert_eq!(sel.selected, set(&[song(1), song(4)]));
        assert_eq!(sel.pivot, Some(song(4)));

        sel.click(song(1), TOGGLE, || items.clone(), &structure());
        assert_eq!(sel.selected, set(&[song(4)]));
        assert_eq!(sel.focused, Some(song(1)));
        assert_eq!(sel.pivot, Some(song(1)));
    }

    #[test]
    fn toggle_wins_over_range_when_both_modifiers_are_held() {
        let both = SelectModifiers {
            toggle: true,
            range: true,
        };
        let mut sel = LibrarySelection::default();
        sel.click(song(1), PLAIN, expanded_items, &structure());
        sel.click(song(5), both, expanded_items, &structure());
        assert_eq!(sel.selected, set(&[song(1), song(5)]));
    }

    #[test]
    fn a_range_click_selects_from_the_pivot_and_keeps_focus_and_pivot() {
        let items = expanded_items();
        let mut sel = LibrarySelection::default();
        sel.click(song(2), PLAIN, || items.clone(), &structure());
        sel.click(song(4), RANGE, || items.clone(), &structure());
        assert_eq!(
            sel.selected,
            set(&[
                song(2),
                album("A|2"),
                song(3),
                artist("B"),
                album("B|1"),
                song(4)
            ])
        );
        assert_eq!(sel.focused, Some(song(2)));
        assert_eq!(sel.pivot, Some(song(2)));
    }

    #[test]
    fn only_a_range_click_with_a_pivot_computes_the_visible_items() {
        let calls = std::cell::Cell::new(0);
        let counted = || {
            calls.set(calls.get() + 1);
            expanded_items()
        };
        let mut sel = LibrarySelection::default();
        sel.click(song(1), RANGE, counted, &structure());
        sel.click(song(2), PLAIN, counted, &structure());
        sel.click(song(3), TOGGLE, counted, &structure());
        assert_eq!(calls.get(), 0);
        sel.click(song(4), RANGE, counted, &structure());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_range_click_without_a_pivot_acts_as_a_plain_click() {
        let mut sel = LibrarySelection::default();
        sel.click(song(4), RANGE, expanded_items, &structure());
        assert_eq!(sel.selected, set(&[song(4)]));
        assert_eq!(sel.pivot, Some(song(4)));
    }

    #[test]
    fn range_boundaries_that_are_headers_add_all_their_songs() {
        let items = vec![artist("A"), album("A|1"), album("A|2"), artist("B")];
        let mut sel = LibrarySelection::default();
        sel.select_range(&items, &album("A|2"), &artist("B"), &structure());
        assert_eq!(
            sel.selected,
            set(&[album("A|2"), artist("B"), song(3), song(4), song(5)])
        );
    }

    #[test]
    fn collapsed_headers_inside_a_range_add_their_songs_and_expanded_ones_do_not() {
        let mut s = structure();
        s.collapsed_albums.insert("A|2".into());
        let items = vec![
            song(1),
            song(2),
            album("A|2"),
            artist("B"),
            album("B|1"),
            song(4),
        ];
        let mut sel = LibrarySelection::default();
        sel.select_range(&items, &song(1), &song(4), &s);
        assert!(
            sel.selected.contains(&song(3)),
            "collapsed album adds its song"
        );
        assert!(
            !sel.selected.contains(&song(5)),
            "expanded artist and album in the middle add nothing"
        );
    }

    #[test]
    fn a_range_with_an_invisible_end_changes_nothing() {
        let mut sel = LibrarySelection::default();
        sel.selected.insert(song(1));
        sel.select_range(&expanded_items(), &song(1), &song(99), &structure());
        assert_eq!(sel.selected, set(&[song(1)]));
    }

    #[test]
    fn a_range_uses_the_last_occurrence_of_a_repeated_item() {
        let items = vec![album("A|1"), song(1), album("A|1"), song(2)];
        let mut sel = LibrarySelection::default();
        sel.select_range(&items, &album("A|1"), &song(2), &FakeStructure::default());
        assert!(!sel.selected.contains(&song(1)));
        assert!(sel.selected.contains(&song(2)));
    }

    #[test]
    fn select_all_selects_every_item_and_clears_focus_and_pivot() {
        let mut sel = LibrarySelection::default();
        sel.click(song(1), PLAIN, expanded_items, &structure());
        sel.select_all(expanded_items());
        assert_eq!(sel.selected.len(), 10);
        assert_eq!(sel.focused, None);
        assert_eq!(sel.pivot, None);
    }

    fn rects() -> Vec<(LibraryListItem, ContentRect)> {
        (0..4)
            .map(|i| (song(i), ContentRect::new(0.0, i as f32 * 32.0, 300.0, 32.0)))
            .collect()
    }

    #[test]
    fn a_plain_marquee_replaces_the_selection_with_the_hit_items() {
        let mut sel = LibrarySelection::default();
        sel.select_only(song(3));
        sel.select_marquee(&rects(), ContentRect::new(10.0, 40.0, 5.0, 30.0), PLAIN);
        assert_eq!(sel.selected, set(&[song(1), song(2)]));
        assert_eq!(sel.focused, Some(song(3)), "focus is left unchanged");
    }

    #[test]
    fn a_shift_marquee_adds_and_a_ctrl_marquee_toggles() {
        let mut sel = LibrarySelection::default();
        sel.selected.insert(song(1));
        sel.select_marquee(&rects(), ContentRect::new(0.0, 70.0, 5.0, 5.0), RANGE);
        assert_eq!(sel.selected, set(&[song(1), song(2)]));

        // Hits songs 2 (selected: toggled off) and 3 (not selected: toggled on).
        sel.select_marquee(&rects(), ContentRect::new(0.0, 70.0, 5.0, 40.0), TOGGLE);
        assert_eq!(sel.selected, set(&[song(1), song(3)]));
    }

    #[test]
    fn a_marquee_that_only_touches_an_edge_hits_nothing() {
        let mut sel = LibrarySelection::default();
        sel.select_marquee(&rects(), ContentRect::new(0.0, 64.0, 5.0, 0.0), PLAIN);
        assert!(sel.selected.is_empty());
        sel.select_marquee(&rects(), ContentRect::new(300.0, 0.0, 5.0, 40.0), PLAIN);
        assert!(sel.selected.is_empty());
    }

    #[test]
    fn selected_songs_expand_headers_only_without_explicit_songs() {
        let s = structure();
        let ids = selected_song_ids(&set(&[album("A|1")]), &s);
        assert_eq!(ids, [1, 2].into_iter().collect());

        let ids = selected_song_ids(&set(&[album("A|1"), song(2)]), &s);
        assert_eq!(
            ids,
            [2].into_iter().collect(),
            "explicit song narrows the album"
        );

        let ids = selected_song_ids(&set(&[artist("A"), album("B|1")]), &s);
        assert_eq!(ids, [1, 2, 3, 4, 5].into_iter().collect());

        let ids = selected_song_ids(&set(&[artist("A"), song(4)]), &s);
        assert_eq!(ids, [1, 2, 3, 4].into_iter().collect());

        let ids = selected_song_ids(&set(&[artist("Nobody"), album("X|1")]), &s);
        assert!(ids.is_empty());
    }

    #[test]
    fn selected_songs_follow_the_order_of_the_song_list() {
        let songs: Vec<Arc<SongData>> = [5, 3, 1, 4]
            .into_iter()
            .map(|id| {
                Arc::new(SongData {
                    id,
                    ..SongData::default()
                })
            })
            .collect();
        let picked = selected_songs(&set(&[artist("A"), song(5)]), &structure(), &songs);
        let ids: Vec<i64> = picked.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![5, 3, 1]);
        assert!(selected_songs(&HashSet::new(), &structure(), &songs).is_empty());
    }

    #[test]
    fn the_current_index_prefers_the_focused_item_then_the_fallback() {
        let items = expanded_items();
        assert_eq!(current_index(&items, Some(&song(3)), |_| true), 5);
        assert_eq!(
            current_index(&items, Some(&song(99)), |it| *it == album("B|1")),
            7
        );
        assert_eq!(current_index(&items, None, |_| false), 0);
    }

    #[test]
    fn list_navigation_moves_by_one_and_clamps_at_both_ends() {
        assert_eq!(list_nav_target(0, 3, LibraryNavDir::Up), 0);
        assert_eq!(list_nav_target(2, 3, LibraryNavDir::Up), 1);
        assert_eq!(list_nav_target(1, 3, LibraryNavDir::Down), 2);
        assert_eq!(list_nav_target(2, 3, LibraryNavDir::Down), 2);
        assert_eq!(list_nav_target(1, 3, LibraryNavDir::Left), 1);
        assert_eq!(list_nav_target(1, 3, LibraryNavDir::None), 1);
    }

    #[test]
    fn navigating_without_shift_selects_only_the_new_item() {
        let items = expanded_items();
        let mut sel = LibrarySelection::default();
        sel.select_only(song(1));
        let new = sel.navigate(&items, 2, 3, PLAIN, &structure());
        assert_eq!(new, Some(song(2)));
        assert_eq!(sel.selected, set(&[song(2)]));
        assert_eq!(sel.focused, Some(song(2)));
        assert_eq!(sel.pivot, Some(song(2)));
    }

    #[test]
    fn navigating_with_shift_extends_a_range_from_the_pivot() {
        let items = expanded_items();
        let mut sel = LibrarySelection::default();
        sel.select_only(song(1));
        sel.navigate(&items, 2, 3, RANGE, &structure());
        let new = sel.navigate(&items, 3, 4, RANGE, &structure());
        assert_eq!(new, Some(album("A|2")));
        assert_eq!(
            sel.selected,
            set(&[song(1), song(2), album("A|2"), song(3)])
        );
        assert_eq!(sel.pivot, Some(song(1)));
        assert_eq!(sel.focused, Some(album("A|2")));
    }

    #[test]
    fn navigating_with_shift_and_no_pivot_starts_from_the_current_item() {
        let items = expanded_items();
        let mut sel = LibrarySelection::default();
        sel.navigate(&items, 8, 9, RANGE, &structure());
        assert_eq!(sel.pivot, Some(song(4)));
        assert_eq!(sel.selected, set(&[song(4), song(5)]));
    }

    #[test]
    fn navigating_an_empty_list_does_nothing_and_targets_are_clamped() {
        let mut sel = LibrarySelection::default();
        let empty: Vec<LibraryListItem> = Vec::new();
        assert_eq!(sel.navigate(&empty, 0, 1, PLAIN, &structure()), None);
        assert_eq!(sel, LibrarySelection::default());

        let new = sel.navigate(&expanded_items(), 0, 99, PLAIN, &structure());
        assert_eq!(new, Some(song(5)));
    }

    #[test]
    fn left_collapses_expanded_headers_and_right_expands_collapsed_ones() {
        assert!(toggle_wanted(LibraryNavDir::Left, false));
        assert!(!toggle_wanted(LibraryNavDir::Left, true));
        assert!(toggle_wanted(LibraryNavDir::Right, true));
        assert!(!toggle_wanted(LibraryNavDir::Right, false));
        assert!(!toggle_wanted(LibraryNavDir::Up, true));
    }

    #[test]
    fn bulk_toggles_cover_selected_headers_and_the_albums_of_selected_songs() {
        let mut s = structure();
        s.collapsed_albums.insert("B|1".into());
        let selected = set(&[artist("A"), album("A|2"), song(1), song(4), artist("B")]);
        let album_of = |id: i64| match id {
            1 | 2 => Some("A|1".to_string()),
            4 | 5 => Some("B|1".to_string()),
            _ => None,
        };
        let (albums, artists) = bulk_toggles(&selected, None, LibraryNavDir::Left, &s, album_of);
        let albums: HashSet<String> = albums.into_iter().collect();
        let artists: HashSet<String> = artists.into_iter().collect();
        assert_eq!(
            albums,
            ["A|2", "A|1"].into_iter().map(String::from).collect()
        );
        assert_eq!(artists, ["A", "B"].into_iter().map(String::from).collect());

        let (albums, artists) = bulk_toggles(&selected, None, LibraryNavDir::Right, &s, album_of);
        assert_eq!(albums, vec!["B|1".to_string()]);
        assert!(artists.is_empty());
    }

    #[test]
    fn bulk_toggles_use_the_focused_item_when_nothing_is_selected() {
        let (albums, artists) = bulk_toggles(
            &HashSet::new(),
            Some(&artist("A")),
            LibraryNavDir::Left,
            &structure(),
            |_| None,
        );
        assert!(albums.is_empty());
        assert_eq!(artists, vec!["A".to_string()]);

        let (albums, artists) = bulk_toggles(
            &HashSet::new(),
            None,
            LibraryNavDir::Left,
            &structure(),
            |_| None,
        );
        assert!(albums.is_empty() && artists.is_empty());
    }

    fn tagged_song(
        artist: Option<&str>,
        album_artist: Option<&str>,
        album: Option<&str>,
    ) -> SongData {
        SongData {
            artist: artist.map(Arc::from),
            album_artist: album_artist.map(Arc::from),
            album: album.map(Arc::from),
            ..SongData::default()
        }
    }

    #[test]
    fn the_artist_to_toggle_comes_from_the_header_or_the_song_artist_tag() {
        let mut s = structure();
        s.collapsed_artists.insert("B".into());
        assert_eq!(
            artist_to_toggle(Some("A"), None, LibraryNavDir::Left, &s),
            Some("A".to_string())
        );
        assert_eq!(
            artist_to_toggle(Some("A"), None, LibraryNavDir::Right, &s),
            None
        );

        let by_tag = tagged_song(Some("B"), Some("A"), None);
        assert_eq!(
            artist_to_toggle(None, Some(&by_tag), LibraryNavDir::Right, &s),
            Some("B".to_string()),
            "the artist tag wins over the album artist"
        );
        let only_album_artist = tagged_song(None, Some("A"), None);
        assert_eq!(
            artist_to_toggle(None, Some(&only_album_artist), LibraryNavDir::Left, &s),
            Some("A".to_string())
        );
        assert_eq!(artist_to_toggle(None, None, LibraryNavDir::Left, &s), None);
    }

    #[test]
    fn the_album_to_toggle_comes_from_the_album_or_the_song_tags() {
        let s = structure();
        assert_eq!(
            album_to_toggle(Some("A|1"), None, None, LibraryNavDir::Left, &s),
            Some("A|1".to_string())
        );
        let cached = vec![AlbumEntry {
            id: "h9".into(),
            title: "Hits".into(),
            artist: "A".into(),
            genre: String::new(),
            year: String::new(),
            cover_path: None,
        }];
        let hits = tagged_song(Some("A"), None, Some("Hits"));
        assert_eq!(
            album_to_toggle(None, Some(&hits), Some(&cached), LibraryNavDir::Left, &s),
            Some("A|h9".to_string())
        );
        assert_eq!(
            album_to_toggle(None, Some(&hits), None, LibraryNavDir::Left, &s),
            Some("A|Hits".to_string())
        );
        assert_eq!(
            album_to_toggle(None, None, None, LibraryNavDir::Left, &s),
            None
        );
        assert_eq!(
            album_to_toggle(Some("A|1"), None, None, LibraryNavDir::Right, &s),
            None
        );
    }

    #[test]
    fn library_data_resolves_artists_and_albums_from_the_managers_data() {
        use crate::logic::library_groups::group_by_artist;
        let songs: Vec<Arc<SongData>> = vec![
            Arc::new(SongData {
                id: 1,
                ..tagged_song(Some("A"), None, Some("Hits"))
            }),
            Arc::new(SongData {
                id: 2,
                ..tagged_song(Some("A"), None, Some("Other"))
            }),
        ];
        let groups = group_by_artist(&songs);
        let cached = vec![AlbumEntry {
            id: "h9".into(),
            title: "Hits".into(),
            artist: "A".into(),
            genre: String::new(),
            year: String::new(),
            cover_path: None,
        }];
        let collapsed_artists: HashSet<String> = ["A".to_string()].into_iter().collect();
        let collapsed_albums = HashSet::new();
        let data = LibraryData {
            artist_groups: &groups,
            songs: Some(&songs),
            cached_albums: Some(&cached),
            collapsed_artists: &collapsed_artists,
            collapsed_albums: &collapsed_albums,
        };
        assert!(data.is_artist_collapsed("A"));
        assert!(!data.is_album_collapsed("A|h9"));
        assert_eq!(data.artist_song_ids("A"), Some(vec![1, 2]));
        assert_eq!(data.artist_song_ids("B"), None);
        assert_eq!(data.album_song_ids("A|h9"), Some(vec![1]));

        let unloaded = LibraryData {
            songs: None,
            ..data
        };
        assert_eq!(unloaded.album_song_ids("A|h9"), None);
    }
}
