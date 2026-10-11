//! One reload per burst of facade events, not one per event.
//!
//! A command that touches many rows publishes one event per change: adding
//! 300 songs to a playlist, or a scan that flags new data again and again,
//! must not make a model re-query its whole source 300 times in one burst. An
//! event marks the model stale and asks for a frame; the next frame reloads it
//! once, however many events arrived. A single event still reloads on the very
//! next frame, which is the frame that would have repainted anyway.
//!
//! This is deliberately not a time-based throttle: a throttle either makes a
//! change take its interval to appear, or fires at once and coalesces nothing.
//! A frame is the natural quantum, because a frame is how often the result
//! could be seen.
//!
//! ## Why `wake_at` and not just `frame_tick`
//!
//! Teksilo pumps frames only when something asks for one. An effect on
//! `frame_tick` alone would run when a frame happened to be pumped for some
//! other reason, which for a background scan into a window nobody touches
//! could be never. Marking stale therefore also arms a wake for now, the
//! one-shot handle `BuildContext::wake_at_handle` returns, so the loop pumps
//! exactly one frame and goes back to sleep.
//!
//! Ported from Skribisto's `models/coalesced_reload.rs`, over this app's
//! [`Origin`] and [`Event`].

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use teksilo::prelude::BuildContext;

use crate::frontend::{Event, Origin};

/// A shared "something changed; reload on the next frame" flag.
///
/// Cloneable and cheap: the event closures and the frame-tick effect hold the
/// same cell.
#[derive(Clone, Default)]
pub struct ReloadCoalescer {
    stale: Rc<Cell<bool>>,
}

impl ReloadCoalescer {
    /// A coalescer with nothing owed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Notes that a reload is owed, and makes sure a frame arrives to do it
    /// on. The wake is for now, not "soon": this coalesces, it does not
    /// debounce.
    pub fn mark(&self, wake: &Rc<Cell<Option<Instant>>>) {
        self.stale.set(true);
        wake.set(Some(Instant::now()));
    }

    /// Takes the pending reload, if there is one.
    pub fn take(&self) -> bool {
        self.stale.replace(false)
    }

    /// Whether a reload is owed.
    pub fn is_stale(&self) -> bool {
        self.stale.get()
    }
}

/// Reloads with `on_reload` when an event of `origins` that `wanted` accepts
/// arrives, once per frame however many of them do.
///
/// Call it from a model's `wire` in place of a `subscribe_event` loop that
/// reloads directly. The reload runs on a frame, so a caller that must see the
/// effect at once (a test, or a command whose result is read back in the same
/// call) calls its own `reload()`; this governs the event-driven path only.
///
/// Returns the coalescer so a test can drive the frame half for real: a
/// headless tree drops the hub's deliveries (`test_support`'s poster), so
/// marking by hand and pumping `frame_tick` is the way to exercise the effect
/// that was registered rather than a re-implementation of it.
pub fn reload_on_events(
    ctx: &mut BuildContext,
    origins: impl IntoIterator<Item = Origin>,
    wanted: impl Fn(&Event) -> bool + 'static,
    on_reload: impl Fn() + 'static,
) -> ReloadCoalescer {
    let coalescer = ReloadCoalescer::new();
    let wake = ctx.wake_at_handle();
    let wanted = Rc::new(wanted);

    for origin in origins {
        let coalescer = coalescer.clone();
        let wake = wake.clone();
        let wanted = wanted.clone();
        ctx.subscribe_event(origin, move |event: &Event| {
            if wanted(event) {
                coalescer.mark(&wake);
            }
        });
    }

    let tick = ctx.frame_tick();
    {
        let coalescer = coalescer.clone();
        ctx.effect(&tick, move |_| {
            if coalescer.take() {
                on_reload();
            }
        });
    }
    coalescer
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use teksilo::core::widget_tree::WidgetTree;
    use teksilo::prelude::*;

    use super::*;
    use crate::frontend::AppContext;
    use crate::frontend::events::LibraryEvent;
    use crate::teksilo_ui::test_support::tree_with_events;

    fn wake() -> Rc<Cell<Option<Instant>>> {
        Rc::new(Cell::new(None))
    }

    #[test]
    fn a_burst_of_events_owes_exactly_one_reload() {
        let coalescer = ReloadCoalescer::new();
        let wake = wake();
        for _ in 0..200 {
            coalescer.mark(&wake);
        }
        assert!(coalescer.take(), "the frame owes a reload");
        assert!(!coalescer.take(), "and only one");
    }

    #[test]
    fn a_frame_with_nothing_pending_does_not_reload() {
        assert!(!ReloadCoalescer::new().take());
    }

    #[test]
    fn marking_asks_for_a_frame_to_reload_on() {
        let coalescer = ReloadCoalescer::new();
        let wake = wake();
        assert!(wake.get().is_none());
        coalescer.mark(&wake);
        assert!(wake.get().is_some(), "a stale model must ask for a frame");
        assert!(coalescer.is_stale());
    }

    #[test]
    fn an_event_after_a_reload_owes_another() {
        let coalescer = ReloadCoalescer::new();
        let wake = wake();
        coalescer.mark(&wake);
        assert!(coalescer.take());
        coalescer.mark(&wake);
        assert!(coalescer.take(), "the second burst is not swallowed");
    }

    /// A widget that wires one coalescer and counts its reloads.
    struct Probe {
        reloads: Rc<Cell<usize>>,
        slot: Rc<RefCell<Option<ReloadCoalescer>>>,
    }

    impl std::fmt::Debug for Probe {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Probe").finish()
        }
    }

    impl Widget for Probe {
        fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
            let reloads = self.reloads.clone();
            let coalescer = reload_on_events(
                ctx,
                [Origin::Library],
                |event| matches!(event, Event::Library(LibraryEvent::Changed)),
                move || reloads.set(reloads.get() + 1),
            );
            *self.slot.borrow_mut() = Some(coalescer);
            Vec::new()
        }

        fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
            proposal.resolve(0.0, 0.0).into()
        }
    }

    fn probe() -> (AppContext, WidgetTree, Rc<Cell<usize>>, ReloadCoalescer) {
        let app = AppContext::in_memory().unwrap();
        let mut tree = tree_with_events(&app);
        let reloads = Rc::new(Cell::new(0));
        let slot = Rc::new(RefCell::new(None));
        tree.add_boxed(Box::new(Probe {
            reloads: reloads.clone(),
            slot: slot.clone(),
        }));
        let coalescer = slot.borrow().clone().unwrap();
        (app, tree, reloads, coalescer)
    }

    #[test]
    fn two_hundred_events_reload_the_model_once() {
        let (_app, tree, reloads, coalescer) = probe();
        let wake = wake();
        for _ in 0..200 {
            coalescer.mark(&wake);
        }
        assert_eq!(reloads.get(), 0, "nothing reloads until a frame arrives");
        tree.frame_tick().set(16.0);
        assert_eq!(reloads.get(), 1, "the whole burst cost one reload");
        tree.frame_tick().set(16.0);
        assert_eq!(reloads.get(), 1, "a later quiet frame reloads nothing");
    }

    #[test]
    fn one_event_still_reloads_on_the_very_next_frame() {
        let (_app, tree, reloads, coalescer) = probe();
        let wake = wake();
        coalescer.mark(&wake);
        tree.frame_tick().set(16.0);
        assert_eq!(reloads.get(), 1);
        coalescer.mark(&wake);
        tree.frame_tick().set(16.0);
        assert_eq!(reloads.get(), 2, "the next edit reloads too");
    }
}
