//! The Thumbnail list mode: 42 px rows, each song with its cover.
//!
//! Port of the `ThumbnailList` arm of the Iced library view
//! (`src/gui/library.rs:1786-1804`): `universal_song_list` with
//! `thumbnail_song_row_widget` rows. Artist headers, album headers and song
//! rows are all 42 px (`logic::library_geometry::ListMetrics`); a song row
//! starts with a 42 px slot holding its 32 px cover (radius 4, the small
//! placeholder), under the header's empty thumbnail column. The rows, the
//! sticky header and the rest are [`LibraryList`]'s.

use super::library_vm::LibraryViewModel;
use super::list_rows::LibraryList;
use crate::logic::library_geometry::LibraryViewMode;

/// The Thumbnail list of `vm`.
pub fn thumbnail_list(vm: LibraryViewModel) -> LibraryList {
    LibraryList::new(vm, LibraryViewMode::ThumbnailList)
}
