//! The Simple list mode: artist headers, album headers and 32 px song rows.
//!
//! Port of the `SimpleList` arm of the Iced library view
//! (`src/gui/library.rs:1748-1766`): `universal_song_list` with
//! `library_song_row_widget` rows and the Simple columns ("#", title, artist,
//! album artist, album, genre, year, then the technical columns). Every row
//! is 32 px (`logic::library_geometry::ListMetrics`). The rows, the sticky
//! header and the rest are [`LibraryList`]'s.

use super::library_vm::LibraryViewModel;
use super::list_rows::LibraryList;
use crate::logic::library_geometry::LibraryViewMode;

/// The Simple list of `vm`.
pub fn simple_list(vm: LibraryViewModel) -> LibraryList {
    LibraryList::new(vm, LibraryViewMode::SimpleList)
}
