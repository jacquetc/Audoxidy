//! The library panel: the album grid, the three list modes and the bottom
//! bar, right of the filter column (`app.rs`'s `library` region).
//!
//! Port of `src/gui/library.rs` and of the library rows of
//! `src/gui/widgets.rs`.

pub mod library_vm;
pub mod structure;

pub use library_vm::LibraryViewModel;
pub use structure::ListStructure;
