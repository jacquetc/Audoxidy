//! What the app does between panels and for the backend, beyond the links in
//! [`UiLinks`](super::UiLinks): the effects that tie one view-model's output
//! to another's state, and the UI's share of the memory purge.
//!
//! - **A reveal clears the filter column.** When the library has to clear its
//!   search and filters to show a song the playlist asked for
//!   ([`LibraryViewModel::reveal_cleared`]), the filter column drops its letter
//!   and its selected node ([`FiltersViewModel::clear_highlight`]), as Iced
//!   clears `selected_subfilter` and `selected_tree_node`
//!   (`src/gui/app.rs:6108-6110`).
//! - **A filter change resets the library.** When the filter column's type,
//!   letter, restore icon or node changes what the library shows
//!   ([`FiltersViewModel::library_reset`]), the library goes back to its top
//!   and its status bar gives the selection's figures to the filter's until
//!   the filter's songs arrive ([`LibraryViewModel::on_filter_changed`]), as
//!   Iced's `ChangeGeneralFilter`, `SelectSubfilter` and `SelectTreeNode` do
//!   (`src/gui/app.rs:4245-4354`; G1, G6).
//! - **The memory purge** (`GlobalMemoryPurge`, `src/gui/app.rs:1580-1631`):
//!   the backend part (the string interner, the preload, the stream buffers,
//!   memory back to the OS) runs on the engine worker, queued by the playback
//!   monitor; here the cover store drops its oldest covers
//!   ([`CoverStore::purge`](crate::teksilo_ui::shared::cover_store::CoverStore::purge)),
//!   as Iced's `purge_old_covers` does. Iced also cuts an unfocused library
//!   of 150 albums and 700 songs or more down to 100 and 500 until the user
//!   acts in it; the Teksilo UI never cuts the library (quirk fix 62). Its
//!   other purge steps have nothing to drop here: the playlist unload never
//!   runs in Iced (the active tab is always kept, `app.rs:1590`), and the
//!   view caches it invalidates are rebuilt on the next frame, which Teksilo's
//!   virtualized views do on their own.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::prelude::*;

use super::AppServices;
use crate::frontend::events::MaintenanceEvent;
use crate::frontend::{Event, Origin};
use crate::teksilo_ui::filters::FiltersViewModel;
use crate::teksilo_ui::library::LibraryViewModel;

/// Wires the effects between panels and the UI's purge, owned by the widget
/// being built (the root).
pub fn wire(
    ctx: &mut BuildContext,
    services: &AppServices,
    filters: &FiltersViewModel,
    library: &LibraryViewModel,
) {
    // Each request is acted on once, by its sequence number, even if the
    // effect runs again on a rebuild.
    let handled = Rc::new(Cell::new(
        library
            .reveal_cleared()
            .get()
            .map_or(0, |(sequence, _)| sequence),
    ));
    {
        let filters = filters.clone();
        ctx.effect(&library.reveal_cleared(), move |request| {
            if let Some((sequence, _)) = request
                && *sequence > handled.get()
            {
                handled.set(*sequence);
                filters.clear_highlight();
            }
        });
    }

    let handled = Rc::new(Cell::new(
        filters
            .library_reset()
            .get()
            .map_or(0, |(sequence, _)| sequence),
    ));
    let library = library.clone();
    ctx.effect(&filters.library_reset(), move |request| {
        if let Some((sequence, _)) = request
            && *sequence > handled.get()
        {
            handled.set(*sequence);
            library.on_filter_changed();
        }
    });

    let covers = services.covers.clone();
    ctx.subscribe_event(Origin::Maintenance, move |event: &Event| {
        if let Event::Maintenance(MaintenanceEvent::PurgeRequested(reason)) = event {
            covers.purge(*reason);
        }
    });
}
