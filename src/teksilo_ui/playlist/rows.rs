//! The playlist's rows: what the list draws, and the data source the
//! `ListView` reads them through.
//!
//! The rows are the *visible* items of the active groups
//! (`logic::playlist_nav::visible_items`): every folder separator, plus the
//! songs of the groups that are not collapsed. Each row keeps the linear index
//! of its item, which is what the selection, the focus, the playing marker
//! and every command speak (`logic::playlist_nav`, "Linear index"). A song
//! hidden in a collapsed group keeps its linear index but has no row.
//!
//! [`PlaylistRowsSource`] is the `ListDataSource` over a snapshot of those
//! rows, held in a `Signal`. The view-model replaces the snapshot whenever the
//! rows change (a content reload, a search, a collapse, a switch of playlist)
//! and the source reports a `DataChange::Reset`: the list re-reads the rows.
//! Both row kinds are 46 px high (`SONG_ROW_HEIGHT`, `FOLDER_SEPARATOR_HEIGHT`),
//! so the list uses the uniform row height.
//!
//! The pure scroll arithmetic of the Iced autoscroll
//! (`execute_playlist_autoscroll`, `src/gui/app.rs:1245`) is here too:
//! [`autoscroll_offset`].

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use teksilo::core::signal::ObserverHandle;
use teksilo::data::{DataChange, ListDataSource};
use teksilo::prelude::Signal;

use crate::db::database::{PlaylistFolderGroup, PlaylistSongRef};
use crate::logic::playlist_nav::{self as nav, PlaylistItemType};

/// The height of every row: a song row and a folder separator are both 46 px
/// (`SONG_ROW_HEIGHT`, `FOLDER_SEPARATOR_HEIGHT`, `src/gui/playlist.rs:723`).
pub const ROW_HEIGHT: f32 = nav::SONG_ROW_HEIGHT;

/// A folder separator row (`build_folder_separator`, `src/gui/playlist.rs:813`).
#[derive(Debug, Clone, PartialEq)]
pub struct SeparatorItem {
    /// The separator's linear index.
    pub linear_idx: usize,
    /// The group's anchor (`first_item_id`), the key of its collapsed state.
    pub anchor: i64,
    /// The folder's name, the separator's title.
    pub folder_name: Arc<str>,
    /// How many songs the group shows.
    pub song_count: usize,
    /// The group's total duration, in seconds.
    pub total_duration: f64,
    /// Whether any song of the group is enabled (the "•" shows).
    pub any_enabled: bool,
}

/// A song row (`build_song_row`, `src/gui/playlist.rs:900`).
#[derive(Debug, Clone)]
pub struct SongItem {
    /// The song's linear index.
    pub linear_idx: usize,
    /// The linear index of its group's separator (a selected separator lights
    /// its songs up, `src/gui/playlist.rs:772`).
    pub separator_idx: usize,
    /// The song.
    pub song: PlaylistSongRef,
}

/// One row of the list.
#[derive(Debug, Clone)]
pub enum RowItem {
    /// A folder separator.
    Separator(SeparatorItem),
    /// A song.
    Song(SongItem),
}

impl RowItem {
    /// The row's linear index.
    pub fn linear_idx(&self) -> usize {
        match self {
            Self::Separator(item) => item.linear_idx,
            Self::Song(item) => item.linear_idx,
        }
    }

    /// The song, for a song row.
    pub fn song(&self) -> Option<&PlaylistSongRef> {
        match self {
            Self::Separator(_) => None,
            Self::Song(item) => Some(&item.song),
        }
    }
}

/// The rows the list draws for `groups` with the groups of `collapsed`
/// folded, in order (`logic::playlist_nav::visible_items`).
pub fn visible_rows(groups: &[PlaylistFolderGroup], collapsed: &HashSet<i64>) -> Vec<RowItem> {
    let mut separator_idx = 0;
    nav::visible_items(groups, collapsed)
        .into_iter()
        .filter_map(|(item, linear_idx)| match item {
            PlaylistItemType::Separator(group_idx, anchor) => {
                separator_idx = linear_idx;
                let group = groups.get(group_idx)?;
                Some(RowItem::Separator(SeparatorItem {
                    linear_idx,
                    anchor,
                    folder_name: group.folder_name.clone(),
                    song_count: group.songs.len(),
                    total_duration: group.total_duration,
                    any_enabled: group.songs.iter().any(|song| song.enabled),
                }))
            }
            PlaylistItemType::Song(group_idx, song_idx, _) => {
                let song = groups.get(group_idx)?.songs.get(song_idx)?.clone();
                Some(RowItem::Song(SongItem {
                    linear_idx,
                    separator_idx,
                    song,
                }))
            }
        })
        .collect()
}

/// The `ListDataSource` of the playlist's rows; see the module documentation.
/// Clones share the rows.
#[derive(Clone)]
pub struct PlaylistRowsSource {
    rows: Signal<Rc<Vec<RowItem>>>,
}

impl std::fmt::Debug for PlaylistRowsSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistRowsSource")
            .field("rows", &self.rows.get_ref().len())
            .finish()
    }
}

impl PlaylistRowsSource {
    /// A source over `rows`: each new snapshot written to it is a reset of the
    /// list.
    pub fn new(rows: Signal<Rc<Vec<RowItem>>>) -> Self {
        Self { rows }
    }

    /// The row index (in the list) of the item at `linear_idx`, if it has a
    /// row.
    pub fn row_of(&self, linear_idx: usize) -> Option<usize> {
        self.rows
            .get_ref()
            .iter()
            .position(|row| row.linear_idx() == linear_idx)
    }
}

impl ListDataSource for PlaylistRowsSource {
    type Item = RowItem;
    type Key = usize;

    fn len(&self) -> usize {
        self.rows.get_ref().len()
    }

    fn with_item<R>(&self, index: usize, f: impl FnOnce(&Self::Item) -> R) -> Option<R> {
        self.rows.get_ref().get(index).map(f)
    }

    fn key_at(&self, index: usize) -> Option<usize> {
        self.rows.get_ref().get(index).map(RowItem::linear_idx)
    }

    fn index_of(&self, key: &usize) -> Option<usize> {
        self.row_of(*key)
    }

    fn observe_changes(&self, f: impl Fn(&DataChange) + 'static) -> ObserverHandle {
        self.rows.observe(move |_| f(&DataChange::Reset))
    }
}

/// Where the list scrolls to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    /// The top of the list (a new search, a playlist without a playing song).
    Top,
    /// Just enough to show the item at this linear index (a song hidden in a
    /// collapsed group shows its separator).
    Item(usize),
}

/// The scroll offset that brings the band `top..bottom` into a viewport
/// `viewport_height` tall scrolled to `scroll_y`, or `None` when it is already
/// in view: above the viewport, its top goes to the top; below it, its bottom
/// goes to the bottom (`execute_playlist_autoscroll`, `src/gui/app.rs:1245`).
pub fn autoscroll_offset(
    top: f32,
    bottom: f32,
    scroll_y: f32,
    viewport_height: f32,
) -> Option<f32> {
    if top < scroll_y {
        Some(top)
    } else if bottom > scroll_y + viewport_height {
        Some(bottom - viewport_height)
    } else {
        None
    }
}

/// Where `target` puts a list scrolled to `scroll_y` in a viewport
/// `viewport_height` tall, over `groups` with `collapsed` folded; `None`
/// leaves the scroll where it is.
pub fn scroll_offset_for(
    target: ScrollTarget,
    groups: &[PlaylistFolderGroup],
    collapsed: &HashSet<i64>,
    scroll_y: f32,
    viewport_height: f32,
) -> Option<f32> {
    match target {
        ScrollTarget::Top => Some(0.0),
        ScrollTarget::Item(linear_idx) => {
            let (top, bottom) = nav::item_y_bounds(groups, collapsed, linear_idx);
            autoscroll_offset(top, bottom, scroll_y, viewport_height)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn song(song_id: i64, enabled: bool) -> PlaylistSongRef {
        PlaylistSongRef {
            item_id: song_id,
            song_id,
            sequence_order: song_id as f64,
            enabled,
            title: format!("Song {song_id}").into(),
            artist_name: "Artist".into(),
            album_title: "Album".into(),
            album_artist_name: "Artist".into(),
            year: None,
            genre: None,
            track_number: None,
            duration: 60.0,
            file_path: format!("/music/{song_id}.flac").into(),
            folder_path: "/music".into(),
            folder_name: "Music".into(),
            cover_path: None,
            cover_override: None,
        }
    }

    fn group(anchor: i64, name: &str, songs: Vec<PlaylistSongRef>) -> PlaylistFolderGroup {
        PlaylistFolderGroup {
            folder_path: format!("/music/{name}").into(),
            folder_name: name.into(),
            total_duration: songs.iter().map(|s| s.duration).sum(),
            songs,
            first_item_id: anchor,
        }
    }

    fn groups() -> Vec<PlaylistFolderGroup> {
        vec![
            group(1, "A", vec![song(1, true), song(2, false)]),
            group(3, "B", vec![song(3, false), song(4, false), song(5, false)]),
        ]
    }

    #[test]
    fn the_rows_are_the_separators_and_the_songs_of_the_open_groups() {
        let rows = visible_rows(&groups(), &HashSet::new());
        let linear: Vec<usize> = rows.iter().map(RowItem::linear_idx).collect();
        assert_eq!(linear, vec![0, 1, 2, 3, 4, 5, 6]);
        let RowItem::Separator(b) = &rows[3] else {
            panic!("row 3 is the second separator")
        };
        assert_eq!(&*b.folder_name, "B");
        assert_eq!(b.song_count, 3);
        assert_eq!(b.total_duration, 180.0);
        assert!(!b.any_enabled, "every song of B is disabled");
        let RowItem::Song(song) = &rows[5] else {
            panic!("row 5 is a song")
        };
        assert_eq!(song.separator_idx, 3);
        assert_eq!(song.song.song_id, 4);
    }

    #[test]
    fn a_collapsed_group_shows_only_its_separator_and_keeps_the_linear_indices() {
        let rows = visible_rows(&groups(), &HashSet::from([1]));
        let linear: Vec<usize> = rows.iter().map(RowItem::linear_idx).collect();
        assert_eq!(linear, vec![0, 3, 4, 5, 6]);
    }

    #[test]
    fn the_source_reads_the_snapshot_and_resets_on_every_new_one() {
        let rows = Signal::new(Rc::new(visible_rows(&groups(), &HashSet::new())));
        let source = PlaylistRowsSource::new(rows.clone());
        assert_eq!(source.len(), 7);
        assert_eq!(source.key_at(4), Some(4));
        assert_eq!(
            source.with_item(1, |row| row.song().map(|s| s.song_id)),
            Some(Some(1))
        );
        let changes = Rc::new(RefCell::new(Vec::new()));
        let sink = changes.clone();
        let _handle = source.observe_changes(move |change| {
            sink.borrow_mut().push(matches!(change, DataChange::Reset));
        });
        rows.set(Rc::new(visible_rows(&groups(), &HashSet::from([3]))));
        assert_eq!(*changes.borrow(), vec![true]);
        assert_eq!(source.len(), 4);
        assert_eq!(source.index_of(&3), Some(3));
        assert_eq!(source.row_of(5), None, "a folded song has no row");
    }

    #[test]
    fn the_autoscroll_moves_only_as_far_as_needed() {
        // In view: nothing.
        assert_eq!(autoscroll_offset(92.0, 138.0, 0.0, 400.0), None);
        // Above: its top to the top.
        assert_eq!(autoscroll_offset(92.0, 138.0, 200.0, 400.0), Some(92.0));
        // Below: its bottom to the bottom.
        assert_eq!(autoscroll_offset(460.0, 506.0, 0.0, 400.0), Some(106.0));
    }

    #[test]
    fn a_folded_song_scrolls_to_its_separator_and_top_goes_home() {
        let groups = groups();
        let folded = HashSet::from([3]);
        // Song 5 (linear 6) is folded into the separator at row 3: 138..184.
        assert_eq!(
            scroll_offset_for(ScrollTarget::Item(6), &groups, &folded, 300.0, 100.0),
            Some(138.0)
        );
        assert_eq!(
            scroll_offset_for(ScrollTarget::Top, &groups, &folded, 300.0, 100.0),
            Some(0.0)
        );
        assert_eq!(
            scroll_offset_for(ScrollTarget::Item(1), &groups, &folded, 0.0, 400.0),
            None
        );
    }
}
