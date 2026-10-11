//! The playlist panel: the left column under the player, 400 px wide.
//!
//! Port of the Iced playlist (`src/gui/playlist.rs`) and of its handlers in
//! `src/gui/app.rs`. From top to bottom:
//!
//! - the tab strip (40 px): one tab per playlist, scrolling sideways, with an
//!   overflow chevron when the tabs do not fit;
//! - the list: a folder separator per group and its songs, 46 px rows;
//! - the bottom bar (40 px): the search field and the lyrics, repeat,
//!   shuffle and equalizer icons.
//!
//! [`PlaylistViewModel`] holds the state and the behaviour; the views are thin.

pub mod dialogs;
pub mod list;
pub mod menus;
pub mod notice;
pub mod playlist_vm;
pub mod rows;
#[cfg(test)]
pub(crate) mod testing;

pub use playlist_vm::{Notice, PlaylistViewModel};
pub use rows::{PlaylistRowsSource, RowItem, ScrollTarget};
