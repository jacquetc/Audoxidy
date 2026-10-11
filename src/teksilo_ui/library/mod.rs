//! The library panel: the album grid, the three list modes and the bottom
//! bar, right of the filter column (`app.rs`'s `library` region).
//!
//! Port of `src/gui/library.rs` and of the library rows of
//! `src/gui/widgets.rs`.

pub mod bottom_bar;
pub mod detailed_list;
pub mod grid;
pub mod grid_detail;
pub mod header_rows;
pub mod hshift;
pub mod library_vm;
pub mod list_rows;
pub mod marquee;
pub mod panel;
pub mod simple_list;
pub mod song_row;
pub mod sticky_header;
pub mod structure;
#[cfg(test)]
pub mod test_util;
pub mod thumbnail_list;

pub use library_vm::LibraryViewModel;
pub use panel::LibraryPanel;
pub use structure::ListStructure;
