//! [`ListStructure`]: what the library's list modes show, derived from the
//! library snapshot.
//!
//! The Iced `LibraryManager` keeps the same data spread over several caches
//! (`artist_groups`, `cached_detailed_view`, `cached_visible_elements`). Here
//! one immutable value holds it, rebuilt by the view-model whenever its inputs
//! change (a new snapshot, the sort, the view mode, a collapse or an
//! expansion), and shared with the views through a signal:
//!
//! - the songs, sorted by the column sort (`logic::library_sort`), or in the
//!   database order when no column is sorted;
//! - the artist groups (`logic::library_groups::group_by_artist`);
//! - the artists, their album runs and their heights, with the Teksilo fixes
//!   of defects D10 and D13 (`logic::library_geometry::build_view_structure_fixed`);
//! - the flat rows of the list (`logic::library_geometry::list_rows`), the
//!   rows a `ListView` virtualizes;
//! - the album cache the composite album ids resolve against, and the
//!   collapse state the structure was built with.

use std::collections::HashSet;
use std::sync::Arc;

use crate::db::database::SongData;
use crate::logic::library_geometry::{
    self, ContentRect, LibraryViewMode, LibraryVirtualArtist, ListRow, build_view_structure_fixed,
};
use crate::logic::library_groups::{AlbumEntry, ArtistGroup, group_by_artist};
use crate::logic::library_selection::{LibraryData, LibraryListItem};
use crate::logic::library_sort::{SortState, sort_songs};

/// The list modes' view of the library; see the module documentation.
#[derive(Debug, Clone)]
pub struct ListStructure {
    /// The view mode the heights are for.
    pub mode: LibraryViewMode,
    /// The songs, in the order the lists show them before grouping.
    pub songs: Vec<Arc<SongData>>,
    /// The artist groups of [`songs`](Self::songs).
    pub groups: Vec<ArtistGroup>,
    /// The artists with their album runs and their extent.
    pub artists: Vec<LibraryVirtualArtist>,
    /// The height of the whole list.
    pub total_height: f32,
    /// The flat rows, each with its `top_y`.
    pub rows: Vec<ListRow>,
    /// Every album of the library (the cache composite ids resolve against).
    pub albums: Vec<AlbumEntry>,
    /// The collapsed artist headers the structure was built with.
    pub collapsed_artists: HashSet<String>,
    /// The collapsed albums the structure was built with.
    pub collapsed_albums: HashSet<String>,
}

impl ListStructure {
    /// An empty structure (nothing loaded).
    pub fn empty(mode: LibraryViewMode) -> Self {
        Self {
            mode,
            songs: Vec::new(),
            groups: Vec::new(),
            artists: Vec::new(),
            total_height: 0.0,
            rows: Vec::new(),
            albums: Vec::new(),
            collapsed_artists: HashSet::new(),
            collapsed_albums: HashSet::new(),
        }
    }

    /// The structure of `songs` (the snapshot's, in database order) and
    /// `albums` under `sort`, in `mode`, with the given collapse state.
    pub fn build(
        songs: &[Arc<SongData>],
        albums: &[AlbumEntry],
        sort: SortState,
        mode: LibraryViewMode,
        collapsed_artists: &HashSet<String>,
        collapsed_albums: &HashSet<String>,
    ) -> Self {
        let mut songs = songs.to_vec();
        // With no sorted column the database order stands (Iced reloads it
        // when the sort is cleared, `src/gui/app.rs:2830`).
        if sort.column.is_some() {
            sort_songs(&mut songs, sort.column, sort.ascending);
        }
        let groups = group_by_artist(&songs);
        let (total_height, artists) = build_view_structure_fixed(
            &groups,
            Some(albums),
            collapsed_artists,
            collapsed_albums,
            mode,
        );
        let rows = library_geometry::list_rows(&artists, collapsed_albums, mode);
        Self {
            mode,
            songs,
            groups,
            artists,
            total_height,
            rows,
            albums: albums.to_vec(),
            collapsed_artists: collapsed_artists.clone(),
            collapsed_albums: collapsed_albums.clone(),
        }
    }

    /// The selection logic's view of this structure.
    pub fn data(&self) -> LibraryData<'_> {
        LibraryData {
            artist_groups: &self.groups,
            songs: Some(&self.songs),
            cached_albums: Some(&self.albums),
            collapsed_artists: &self.collapsed_artists,
            collapsed_albums: &self.collapsed_albums,
        }
    }

    /// The list items in display order (headers of collapsed albums and
    /// artists included, their hidden songs not).
    pub fn items(&self) -> Vec<LibraryListItem> {
        library_geometry::list_items(&self.artists, &self.collapsed_albums)
    }

    /// The list items with their rectangles in content space, `width` wide.
    pub fn item_rects(&self, width: f32) -> Vec<(LibraryListItem, ContentRect)> {
        library_geometry::list_item_rects(&self.artists, &self.collapsed_albums, self.mode, width)
    }

    /// The vertical span of `item` (D12 fixed: none for a hidden album).
    pub fn item_y_range(&self, item: &LibraryListItem) -> Option<(f32, f32)> {
        library_geometry::item_y_range_fixed(item, &self.artists, &self.collapsed_albums, self.mode)
    }

    /// The composite id of the album run that holds `song_id`.
    pub fn album_id_for_song(&self, song_id: i64) -> Option<String> {
        library_geometry::album_id_for_song(&self.artists, song_id)
    }

    /// The cover of the album with hash `hash` by `artist`, from the cache.
    pub fn cover_of(&self, artist: &str, hash: &str) -> Option<String> {
        self.albums
            .iter()
            .find(|a| a.id == hash && a.artist == artist)
            .and_then(|a| a.cover_path.clone())
    }

    /// Whether `artist_name`'s group holds the song at `path`: the playing
    /// marker of an artist header (`widgets.rs:2638-2647`).
    pub fn artist_holds(&self, artist_name: &str, path: &str) -> bool {
        !path.is_empty()
            && self
                .groups
                .iter()
                .find(|g| g.name == artist_name)
                .is_some_and(|g| g.songs.iter().any(|s| s.full_file_path.as_ref() == path))
    }
}
