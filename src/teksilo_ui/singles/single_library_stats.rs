//! `SingleLibraryStats`: the totals of the whole library.
//!
//! The [`LibraryStats`] (songs, albums, artists, total duration and size) the
//! status bar shows when nothing narrows it, from `commands::library::stats`.
//! It reloads on [`LibraryEvent::Changed`](crate::frontend::events::LibraryEvent::Changed) and when a scan finishes
//! (coalesced: a scan that flags new data again and again costs one query per
//! frame), and a newer reload supersedes an older one.

use std::rc::Rc;

use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use crate::frontend::commands::library;
use crate::frontend::{AppContext, Latest, Origin};
use crate::logic::library_groups::LibraryStats;
use crate::teksilo_ui::models::coalesced_reload::{ReloadCoalescer, reload_on_events};
use crate::teksilo_ui::models::library_songs_model::is_reload_event;
use crate::teksilo_ui::models::load::{finish_tracked, prepare_runtime};

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    stats: Signal<LibraryStats>,
    loaded: Signal<bool>,
    latest: Latest,
}

/// The library totals. UI thread only; clones share one state.
#[derive(Clone)]
pub struct SingleLibraryStats {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for SingleLibraryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleLibraryStats")
            .field("stats", &self.inner.stats.get())
            .finish_non_exhaustive()
    }
}

impl SingleLibraryStats {
    /// A single with zero totals. Nothing is loaded until
    /// [`reload`](Self::reload).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                stats: Signal::new(LibraryStats::default()),
                loaded: Signal::new(false),
                latest: Latest::new(),
            }),
        }
    }

    /// Subscribes the single to the library changes; the reload runs once per
    /// frame however many arrive. Call it from a long-lived widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let single = self.clone();
        reload_on_events(ctx, [Origin::Library], is_reload_event, move || {
            single.reload();
        })
    }

    /// The totals of the whole library.
    pub fn stats(&self) -> Signal<LibraryStats> {
        self.inner.stats.clone()
    }

    /// Whether the totals were loaded at least once.
    pub fn loaded(&self) -> Signal<bool> {
        self.inner.loaded.clone()
    }

    /// Queries the totals again.
    pub fn reload(&self) {
        let inner = &self.inner;
        let reply = inner.latest.track(library::stats(&inner.ctx));
        let single = self.clone();
        finish_tracked(&inner.runtime, "the library totals", reply, move |stats| {
            if let Some(stats) = stats {
                single.inner.stats.set_if_changed(stats);
                single.inner.loaded.set_if_changed(true);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::{run_until, tree_with_async};

    #[test]
    fn the_totals_are_those_of_the_library() {
        let (ctx, seeded) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let single = SingleLibraryStats::new(ctx, runtime.clone());
        assert!(!single.loaded().get());
        assert_eq!(single.stats().get(), LibraryStats::default());
        single.reload();
        assert!(run_until(&runtime, TIMEOUT, || single.loaded().get()));
        let stats = single.stats().get();
        assert_eq!(stats.songs, seeded.songs.len() as u64);
        assert_eq!(stats.albums, 4);
        assert!(stats.duration_secs > 0.0);
        assert!(stats.size_bytes > 0.0);
    }

    #[test]
    fn a_reload_after_new_songs_updates_the_totals() {
        let (ctx, seeded) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let single = SingleLibraryStats::new(ctx.clone(), runtime.clone());
        single.reload();
        assert!(run_until(&runtime, TIMEOUT, || single.loaded().get()));
        LibraryFixture::new()
            .song(FixtureSong::new(
                "/fixtures/music/New/Fresh/01 - Fresh.flac",
                "Fresh",
                "New Band",
                "Fresh",
            ))
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        single.reload();
        let wanted = seeded.songs.len() as u64 + 1;
        assert!(run_until(&runtime, TIMEOUT, || single.stats().get().songs == wanted));
    }

    #[test]
    fn two_reloads_in_flight_apply_only_the_newer() {
        let (ctx, _) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let single = SingleLibraryStats::new(ctx, runtime.clone());
        let writes = Rc::new(std::cell::Cell::new(0));
        let counter = writes.clone();
        let _handle = single
            .stats()
            .observe(move |_| counter.set(counter.get() + 1));
        single.reload();
        single.reload();
        assert!(run_until(&runtime, TIMEOUT, || single.loaded().get()));
        runtime.tick();
        assert_eq!(writes.get(), 1);
    }

    #[test]
    fn a_burst_of_library_events_reloads_once_per_frame() {
        let (ctx, _) = test_env::demo();
        let (mut tree, runtime) = tree_with_async(&ctx);
        let single = SingleLibraryStats::new(ctx, runtime.clone());
        let slot = Rc::new(std::cell::RefCell::new(None));
        {
            let (single, slot) = (single.clone(), slot.clone());
            test_env::mount(&mut tree, move |build| {
                *slot.borrow_mut() = Some(single.wire(build));
            });
        }
        let coalescer = slot.borrow().clone().unwrap();
        let wake = test_env::wake();
        for _ in 0..25 {
            coalescer.mark(&wake);
        }
        let writes = Rc::new(std::cell::Cell::new(0));
        let counter = writes.clone();
        let _handle = single
            .stats()
            .observe(move |_| counter.set(counter.get() + 1));
        tree.frame_tick().set(16.0);
        tree.frame_tick().set(16.0);
        assert!(run_until(&runtime, TIMEOUT, || single.loaded().get()));
        runtime.tick();
        assert_eq!(writes.get(), 1, "twenty-five events, one query");
    }
}
