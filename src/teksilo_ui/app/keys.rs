//! The app's one global shortcut: Ctrl+A (⌘A on macOS) selects everything in
//! the playlist or the library.
//!
//! Iced routes every unhandled key through `GlobalKeyDown`
//! (`src/gui/app.rs:3215-3316`) by its `AppFocus`, which the last click or key
//! in a panel sets and which starts on the library: Ctrl+A selects all in the
//! playlist when it has the focus, in the library otherwise. Here the arrows,
//! Enter, Delete and Space are the panels' own keys (quirk fix 2); Ctrl+A is a
//! shortcut, so that it works wherever the focus sits in a panel, and it acts
//! on the panel that has the keyboard focus ([`select_all_target`]):
//!
//! - the playlist or the library, when the focus is inside it;
//! - the library when nothing has the focus, as Iced's `AppFocus` starts there;
//! - nothing anywhere else (the filter column, the Audio Center, the title
//!   bar, a menu or a dialog): the shortcut is off and the key goes to the
//!   focused widget.
//!
//! The shortcut is also off while a text field has the focus, so the field
//! selects its own text (Iced only routes a key no widget took).

use teksilo::core::Key;
use teksilo::core::action::Action;
use teksilo::core::shortcut::{KeyStroke, Shortcut};
use teksilo::prelude::*;

use crate::teksilo_ui::library::LibraryViewModel;
use crate::teksilo_ui::playlist::PlaylistViewModel;

/// The shortcut's id, and the intent its action handles.
pub const SELECT_ALL: &str = "app.select_all";

/// The panel Ctrl+A selects in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectAllTarget {
    /// The playlist's rows.
    Playlist,
    /// The library's rows, or its albums in the grid.
    Library,
}

/// Where Ctrl+A acts: the panel the focus is inside, the library when
/// nothing has the focus, and nowhere when the focus is elsewhere.
pub fn select_all_target(
    in_playlist: bool,
    in_library: bool,
    anything_focused: bool,
) -> Option<SelectAllTarget> {
    if in_playlist {
        Some(SelectAllTarget::Playlist)
    } else if in_library || !anything_focused {
        Some(SelectAllTarget::Library)
    } else {
        None
    }
}

/// The two panels Ctrl+A can act on, and whether the keyboard focus is
/// inside each (bound with `focus_within` on the panels the app mounts).
#[derive(Clone)]
pub struct SelectAllPanels {
    /// The playlist's view-model.
    pub playlist: PlaylistViewModel,
    /// The library's view-model.
    pub library: LibraryViewModel,
    /// True while the focus is inside the playlist panel.
    pub in_playlist: Signal<bool>,
    /// True while the focus is inside the library panel.
    pub in_library: Signal<bool>,
}

impl std::fmt::Debug for SelectAllPanels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectAllPanels")
            .field("in_playlist", &self.in_playlist.get())
            .field("in_library", &self.in_library.get())
            .finish_non_exhaustive()
    }
}

/// Registers the shortcut and its action, owned by the widget being built
/// (the root).
pub fn register_select_all(ctx: &mut BuildContext, panels: SelectAllPanels) {
    let surfaces = ctx.text_surfaces();
    let focus = surfaces.focus_signal();
    let not_in_text = focus.map(move |_| !surfaces.focused_is_text_surface());
    let has_target = panels.in_playlist.zip3(&panels.in_library, &focus).map(
        |(in_playlist, in_library, focused)| {
            select_all_target(*in_playlist, *in_library, focused.is_some()).is_some()
        },
    );
    ctx.register_shortcut_global(
        Shortcut::new(SELECT_ALL)
            .primary(KeyStroke::command(Key::A))
            .enabled_when(has_target.and(&not_in_text))
            .build(),
    );
    ctx.register_action_global(Action::new(SELECT_ALL).on_invoke(move |_intent, _ctx| {
        match select_all_target(
            panels.in_playlist.get(),
            panels.in_library.get(),
            focus.get().is_some(),
        ) {
            Some(SelectAllTarget::Playlist) => panels.playlist.select_all(),
            Some(SelectAllTarget::Library) => panels.library.select_all(),
            None => {}
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_focused_panel_takes_ctrl_a_and_the_library_takes_it_from_nowhere() {
        assert_eq!(
            select_all_target(true, false, true),
            Some(SelectAllTarget::Playlist)
        );
        assert_eq!(
            select_all_target(false, true, true),
            Some(SelectAllTarget::Library)
        );
        assert_eq!(
            select_all_target(false, false, false),
            Some(SelectAllTarget::Library),
            "Iced's AppFocus starts on the library"
        );
        assert_eq!(select_all_target(false, false, true), None);
    }
}
