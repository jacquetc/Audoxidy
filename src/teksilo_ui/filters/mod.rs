//! The filter column: the panel under the title bar's logo cell, with the
//! filter-type selector, the letter grid, the tree and the search field
//! (`src/gui/library_filters.rs`, below its top bar).
//!
//! - [`FiltersViewModel`] holds the state (filter type, letter, search,
//!   expansion, selected node) and turns a selected node into the library's
//!   filter, through [`LibrarySongsModel`](crate::teksilo_ui::models::LibrarySongsModel);
//! - [`FilterTreeSource`] is the `TreeDataSource` the tree view reads;
//! - [`FiltersPanel`] is the 202 px column.

pub mod filters_vm;
pub mod panel;
pub mod tree_source;

pub use filters_vm::FiltersViewModel;
pub use panel::FiltersPanel;
pub use tree_source::FilterTreeSource;
