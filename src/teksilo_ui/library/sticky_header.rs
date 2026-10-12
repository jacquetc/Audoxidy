//! [`StickyHeader`]: the artist header pinned to the top of a list mode.
//!
//! Port of the sticky overlay of `universal_song_list` and
//! `detailed_song_list` (`src/gui/widgets.rs:2731-2778`, `:3065-3112`): over
//! the top of the list, inside its 10 px / 15 px padding, the header of the
//! last artist whose header top is at or above `scroll + header height − 1`
//! (`logic::library_geometry::sticky_artist_index`). It is the same header
//! as in the list: clicking it selects the artist, its chevron collapses or
//! expands it. At the very top it covers the first header exactly.
//!
//! It is an overlay driven by the list's `scroll_y_signal`: it is rebuilt
//! only when the pinned artist (or its state) changes, not on every scroll
//! step.

use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::widgets::{FixedSize, Spacer};

use super::library_vm::LibraryViewModel;
use super::list_rows::{artist_header, padded};
use crate::logic::library_geometry::{
    LibraryViewMode, ListMetrics, StickyArtist, sticky_artist_index,
};

/// The artist pinned at `scroll_y` in `vm`'s list, if any.
pub fn sticky_at(
    vm: &LibraryViewModel,
    mode: LibraryViewMode,
    scroll_y: f32,
) -> Option<(usize, StickyArtist)> {
    let structure = vm.structure().get();
    let index = sticky_artist_index(&structure.artists, scroll_y, mode)?;
    let va = &structure.artists[index];
    Some((
        index,
        StickyArtist {
            name: va.artist_name.clone(),
            is_collapsed: va.is_collapsed,
            albums_count: va.albums_count,
            songs_count: va.songs_count,
            duration_secs: va.duration_secs,
        },
    ))
}

/// The pinned artist header; see the module documentation.
pub struct StickyHeader {
    vm: LibraryViewModel,
    mode: LibraryViewMode,
    scroll_y: Signal<f32>,
    pinned: Signal<Option<(usize, StickyArtist)>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for StickyHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StickyHeader")
            .field("pinned", &self.pinned.get().map(|(_, artist)| artist.name))
            .finish_non_exhaustive()
    }
}

impl StickyHeader {
    /// The header pinned over `vm`'s `mode` list scrolled to `scroll_y`.
    pub fn new(vm: LibraryViewModel, mode: LibraryViewMode, scroll_y: Signal<f32>) -> Self {
        let pinned = Signal::new(sticky_at(&vm, mode, scroll_y.get()));
        Self {
            vm,
            mode,
            scroll_y,
            pinned,
            root: None,
        }
    }

    /// The artist it pins.
    pub fn pinned(&self) -> Signal<Option<(usize, StickyArtist)>> {
        self.pinned.clone()
    }
}

impl Widget for StickyHeader {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let id = ctx.self_id();
        self.pinned
            .bind_to(id, ctx.binding_registry(), BindingLevel::Rebuild);
        {
            let (vm, mode, pinned) = (self.vm.clone(), self.mode, self.pinned.clone());
            ctx.effect(&self.scroll_y, move |y| {
                pinned.set_if_changed(sticky_at(&vm, mode, *y));
            });
        }
        {
            let (vm, mode, pinned) = (self.vm.clone(), self.mode, self.pinned.clone());
            let scroll_y = self.scroll_y.clone();
            ctx.effect(&self.vm.structure(), move |_| {
                pinned.set_if_changed(sticky_at(&vm, mode, scroll_y.get()));
            });
        }
        let header_h = ListMetrics::for_mode(self.mode).artist_header_h;
        let structure = self.vm.structure().get();
        let header = self
            .pinned
            .get()
            .and_then(|(index, _)| artist_header(&self.vm, &structure, index));
        let root = match header {
            // As wide as the list's rows: a stack would give the header its
            // natural width only.
            Some(header) => ctx.add(FixedSize::new().height(header_h).child(padded(header))),
            None => ctx.add(FixedSize::new().width(0.0).height(0.0).child(Spacer::new())),
        };
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let header_h = if self.pinned.get().is_some() {
            ListMetrics::for_mode(self.mode).artist_header_h
        } else {
            0.0
        };
        let width = proposal.width.unwrap_or(0.0);
        self.root
            .and_then(|id| {
                ctx.child_size(
                    id,
                    SizeProposal {
                        width: Some(width),
                        height: Some(header_h),
                    },
                )
            })
            .map(|size| Size::new(width, size.height.min(header_h)))
            .unwrap_or_else(|| Size::new(width, header_h))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
