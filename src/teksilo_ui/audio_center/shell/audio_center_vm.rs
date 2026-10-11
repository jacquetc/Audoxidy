//! [`AudioCenterViewModel`]: the Audio Center's open state, tab and position,
//! and the drag of its panel.
//!
//! Port of the shell of `AudioCenterManager` (`open`, `selected_tab`,
//! `window_pos`, `src/gui/audio_center.rs:151`) and of the app's handling of
//! it (`src/gui/app.rs`):
//!
//! - the open state, the tab and the position are the shared
//!   [`AudioCenterLink`](crate::teksilo_ui::app::services::AudioCenterLink):
//!   the player menu toggles it with Iced's rule
//!   (`ToggleAudioCenter`, `app.rs:4388`), and this view-model selects tabs
//!   (`TabSelected`) and closes (`Close`);
//! - the panel is centred until it is dragged. A drag moves it by the
//!   pointer's movement since the last move, from the position it has (from
//!   the centred one when it was never dragged), and clamps the result to
//!   `[0, W - 100] × [0, H - 40]` in the window (`app.rs:4708-4729`);
//! - the window's size is what the panel's layer was laid out in
//!   ([`set_viewport`](AudioCenterViewModel::set_viewport)); until it is
//!   known it is Iced's initial 1360 × 880.
//!
//! Positions are in logical coordinates: `x` counts from the leading edge, so
//! a right-to-left layout mirrors the panel and the pointer deltas the panel
//! feeds in.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::prelude::Signal;

use crate::teksilo_ui::app::services::{AppServices, AudioCenterLink, audio_center_tab};

/// The panel's width (`audio_center.rs:1998`).
pub const WIDTH: f32 = 940.0;
/// The panel's height (`audio_center.rs:1999`).
pub const HEIGHT: f32 = 480.0;
/// The height Iced centres the panel's first drag with (`app.rs:4716`): 474,
/// where the panel is 480 high. The first drag of a centred panel therefore
/// starts 3 px lower than where it was drawn.
pub const CENTRE_HEIGHT: f32 = 474.0;
/// How much of the panel's width stays on screen at the far edge
/// (`app.rs:4723`).
pub const MIN_VISIBLE_WIDTH: f32 = 100.0;
/// How much of the panel's height stays on screen at the bottom edge
/// (`app.rs:4725`).
pub const MIN_VISIBLE_HEIGHT: f32 = 40.0;
/// The window size Iced starts with (`app.rs:1120`), used until the panel's
/// layer has been laid out.
pub const INITIAL_VIEWPORT: (f32, f32) = (1360.0, 880.0);

struct Inner {
    link: AudioCenterLink,
    viewport: Cell<(f32, f32)>,
    drag: Cell<Option<(f32, f32)>>,
}

/// The Audio Center shell's view-model. UI thread only; clones share one
/// state.
#[derive(Clone)]
pub struct AudioCenterViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for AudioCenterViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCenterViewModel")
            .field("open", &self.inner.link.open.get())
            .field("tab", &self.inner.link.tab.get())
            .field("position", &self.inner.link.position.get())
            .finish_non_exhaustive()
    }
}

/// `value` limited to `[0, max]` (a window smaller than the margin pins the
/// panel to 0, as Iced's `.max(0.0)` does).
fn clamp_axis(value: f32, max: f32) -> f32 {
    value.clamp(0.0, max.max(0.0))
}

impl AudioCenterViewModel {
    /// The view-model over the Audio Center link of `services`.
    pub fn new(services: &AppServices) -> Self {
        Self {
            inner: Rc::new(Inner {
                link: services.links.audio_center.clone(),
                viewport: Cell::new(INITIAL_VIEWPORT),
                drag: Cell::new(None),
            }),
        }
    }

    /// Whether the panel is shown.
    pub fn open(&self) -> Signal<bool> {
        self.inner.link.open.clone()
    }

    /// The selected tab (see [`audio_center_tab`]).
    pub fn tab(&self) -> Signal<usize> {
        self.inner.link.tab.clone()
    }

    /// The panel's position after a drag; `None` while it is centred.
    pub fn position(&self) -> Signal<Option<(f32, f32)>> {
        self.inner.link.position.clone()
    }

    /// Whether `tab` is on screen: the panel is open on it.
    pub fn shows(&self, tab: usize) -> Signal<bool> {
        self.inner
            .link
            .open
            .zip(&self.inner.link.tab)
            .map(move |(open, selected)| *open && *selected == tab)
    }

    /// Selects `tab` (Iced `TabSelected`). A tab that does not exist is
    /// ignored.
    pub fn select_tab(&self, tab: usize) {
        if tab <= audio_center_tab::VOLUME_MIX {
            self.inner.link.tab.set_if_changed(tab);
        }
    }

    /// Closes the panel and forgets its position (Iced `Close`). The
    /// equalizer and effects need no save: see the module documentation of
    /// [`shell`](super).
    pub fn close(&self) {
        self.end_drag();
        self.inner.link.close();
    }

    /// Records the size of the window the panel is placed in.
    pub fn set_viewport(&self, width: f32, height: f32) {
        self.inner.viewport.set((width, height));
    }

    /// The size of the window the panel is placed in.
    pub fn viewport(&self) -> (f32, f32) {
        self.inner.viewport.get()
    }

    /// Where the panel's top-leading corner is drawn: its position, or the
    /// centre of the window (never negative, so a window smaller than the
    /// panel shows its leading top corner).
    pub fn origin(&self) -> (f32, f32) {
        match self.inner.link.position.get() {
            Some(position) => position,
            None => {
                let (width, height) = self.viewport();
                (
                    ((width - WIDTH) / 2.0).max(0.0),
                    ((height - HEIGHT) / 2.0).max(0.0),
                )
            }
        }
    }

    /// Whether a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.inner.drag.get().is_some()
    }

    /// Starts a drag with the pointer at `pointer` (`DragStart`,
    /// `app.rs:4482`).
    pub fn begin_drag(&self, pointer: (f32, f32)) {
        self.inner.drag.set(Some(pointer));
    }

    /// Moves the panel by the pointer's movement since the last call
    /// (`app.rs:4708`). Without a drag in progress nothing happens.
    pub fn drag_to(&self, pointer: (f32, f32)) {
        let Some(last) = self.inner.drag.get() else {
            return;
        };
        let (width, height) = self.viewport();
        let current = self.inner.link.position.get().unwrap_or_else(|| {
            (
                ((width - WIDTH) / 2.0).max(0.0),
                ((height - CENTRE_HEIGHT) / 2.0).max(0.0),
            )
        });
        let x = clamp_axis(current.0 + (pointer.0 - last.0), width - MIN_VISIBLE_WIDTH);
        let y = clamp_axis(
            current.1 + (pointer.1 - last.1),
            height - MIN_VISIBLE_HEIGHT,
        );
        self.inner.link.position.set(Some((x, y)));
        self.inner.drag.set(Some(pointer));
    }

    /// Ends the drag (`app.rs:4823`).
    pub fn end_drag(&self) {
        self.inner.drag.set(None);
    }
}

#[cfg(test)]
mod tests;
