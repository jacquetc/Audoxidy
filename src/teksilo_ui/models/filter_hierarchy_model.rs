//! `FilterHierarchyModel`: the tree behind the Folder, Artist, Album, Genre
//! and Year filters.
//!
//! It holds a [`FilterIndex`] (`logic::filter_index`: the three-level tree and
//! the alphabetical subfilters of every filter type) and the filter type the
//! panel shows. The index is built, as in Iced, for all five types at once,
//! by `commands::filters::build_filter_index`: a long operation on its own
//! thread that reports its progress through [`LongOpEvent`]s. The model
//! starts it on [`reload`](FilterHierarchyModel::reload) (and on
//! [`LibraryEvent::Changed`](crate::frontend::events::LibraryEvent::Changed), coalesced), follows its events for the
//! `progress` signal, and takes the finished index when the operation
//! completes. A reload that starts while a build runs cancels it: the events
//! of an operation that is no longer the current one are ignored, and the
//! result of a cancelled build that finished anyway is discarded.
//!
//! Until the first build completes, selecting a filter type loads just that
//! type with `commands::filters::load_hierarchy`, so the panel does not wait
//! for all five.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use super::coalesced_reload::{ReloadCoalescer, reload_on_events};
use super::library_songs_model::is_reload_event;
use super::load::{finish_tracked, prepare_runtime};
use crate::frontend::commands::filters;
use crate::frontend::events::{LongOpEvent, LongOpKind, OpId};
use crate::frontend::{AppContext, Event, Latest, Origin};
use crate::logic::filter_index::{FilterIndex, FilterType, TreeNode};

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    filter_type: Signal<FilterType>,
    index: RefCell<FilterIndex>,
    version: Signal<u64>,
    progress: Signal<Option<f32>>,
    build_op: Cell<Option<OpId>>,
    /// Builds a reload cancelled, whose stored results are still to discard.
    superseded: RefCell<Vec<OpId>>,
    latest: Latest,
}

/// The filter tree. UI thread only; clones share one state.
#[derive(Clone)]
pub struct FilterHierarchyModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for FilterHierarchyModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterHierarchyModel")
            .field("filter_type", &self.inner.filter_type.get())
            .field("building", &self.inner.build_op.get())
            .finish_non_exhaustive()
    }
}

impl FilterHierarchyModel {
    /// An empty model showing `filter_type`. Nothing is loaded until
    /// [`reload`](Self::reload).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle, filter_type: FilterType) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                filter_type: Signal::new(filter_type),
                index: RefCell::new(FilterIndex::new()),
                version: Signal::new(0),
                progress: Signal::new(None),
                build_op: Cell::new(None),
                superseded: RefCell::new(Vec::new()),
                latest: Latest::new(),
            }),
        }
    }

    /// Subscribes the model to the library changes (the rebuild runs once per
    /// frame however many arrive) and to the long operations' events. Call it
    /// from a long-lived widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let model = self.clone();
        ctx.subscribe_event(Origin::LongOperation, move |event: &Event| {
            model.on_long_op(event);
        });
        let model = self.clone();
        reload_on_events(ctx, [Origin::Library], is_reload_event, move || {
            model.reload();
        })
    }

    /// The filter type the panel shows.
    pub fn filter_type(&self) -> Signal<FilterType> {
        self.inner.filter_type.clone()
    }

    /// A counter that bumps whenever the index changed.
    pub fn version(&self) -> Signal<u64> {
        self.inner.version.clone()
    }

    /// The index build's progress, from 0.0 to 1.0 (`None` while it is
    /// indeterminate or when no build runs).
    pub fn progress(&self) -> Signal<Option<f32>> {
        self.inner.progress.clone()
    }

    /// Whether an index build is running.
    pub fn is_building(&self) -> bool {
        self.inner.build_op.get().is_some()
    }

    /// The full sorted tree of the shown filter type (empty until it loaded).
    pub fn tree(&self) -> Vec<TreeNode> {
        self.tree_of(self.inner.filter_type.get())
    }

    /// The full sorted tree of `filter_type` (empty until it loaded).
    pub fn tree_of(&self, filter_type: FilterType) -> Vec<TreeNode> {
        self.inner
            .index
            .borrow()
            .tree(filter_type)
            .map(<[TreeNode]>::to_vec)
            .unwrap_or_default()
    }

    /// The initial-character groups of the shown filter type (empty until it
    /// loaded).
    pub fn subfilters(&self) -> Vec<String> {
        self.inner
            .index
            .borrow()
            .subfilters(self.inner.filter_type.get())
            .map(<[String]>::to_vec)
            .unwrap_or_default()
    }

    /// Shows `filter_type`. If its tree has not been built yet, loads it.
    pub fn set_filter_type(&self, filter_type: FilterType) {
        self.inner.filter_type.set(filter_type);
        let loaded = self.inner.index.borrow().tree(filter_type).is_some();
        if !loaded {
            self.load_type(filter_type);
        }
    }

    /// Rebuilds the index of every filter type as a long operation, cancelling
    /// a build that is still running.
    pub fn reload(&self) {
        let inner = &self.inner;
        if let Some(running) = inner.build_op.take() {
            inner.ctx.ops.cancel(running);
            inner.superseded.borrow_mut().push(running);
        }
        inner
            .build_op
            .set(Some(filters::build_filter_index(&inner.ctx)));
        inner.progress.set(None);
    }

    /// Loads the tree of one filter type, without the other four.
    fn load_type(&self, filter_type: FilterType) {
        let inner = &self.inner;
        let reply = inner
            .latest
            .track(filters::load_hierarchy(&inner.ctx, filter_type));
        let model = self.clone();
        finish_tracked(&inner.runtime, "the filter hierarchy", reply, move |rows| {
            let Some(rows) = rows else { return };
            model.inner.index.borrow_mut().insert(filter_type, rows);
            model.bump();
        });
    }

    /// Follows the index build's [`LongOpEvent`]s. Events of other operations
    /// are ignored.
    pub fn on_long_op(&self, event: &Event) {
        let Event::LongOperation(event) = event else {
            return;
        };
        let (LongOpEvent::Progress(info)
        | LongOpEvent::Completed(info)
        | LongOpEvent::Failed(info)) = event;
        let inner = &self.inner;
        if info.kind != LongOpKind::FilterIndexBuild {
            return;
        }
        if !matches!(event, LongOpEvent::Progress(_)) {
            let mut superseded = inner.superseded.borrow_mut();
            if let Some(position) = superseded.iter().position(|id| *id == info.op_id) {
                // A build this model cancelled ended: its result is of no use.
                superseded.remove(position);
                drop(inner.ctx.ops.take_result::<FilterIndex>(info.op_id));
                return;
            }
        }
        if inner.build_op.get() != Some(info.op_id) {
            return;
        }
        match event {
            LongOpEvent::Progress(info) => inner.progress.set(info.progress),
            LongOpEvent::Completed(info) => {
                inner.build_op.set(None);
                inner.progress.set(None);
                match inner.ctx.ops.take_result::<FilterIndex>(info.op_id) {
                    Some(Ok(index)) => {
                        *inner.index.borrow_mut() = index;
                        self.bump();
                    }
                    Some(Err(error)) => {
                        tracing::warn!("the filter index could not be built: {error}");
                    }
                    None => tracing::warn!("the filter index build left no result"),
                }
            }
            LongOpEvent::Failed(info) => {
                inner.build_op.set(None);
                inner.progress.set(None);
                if !info.cancelled {
                    tracing::warn!(
                        "the filter index build failed: {}",
                        info.error.as_deref().unwrap_or("unknown error")
                    );
                }
            }
        }
    }

    fn bump(&self) {
        let version = &self.inner.version;
        version.set(version.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, mpsc};

    use super::*;
    use crate::frontend::{EventHub, SubscriptionToken};
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::{run_until, tree_with_async};

    /// Collects the long operations' events, as a subscription would deliver
    /// them (a headless tree drops the deliveries).
    fn collect_ops(hub: &EventHub) -> (SubscriptionToken, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let token = hub.subscribe(
            Origin::LongOperation,
            Arc::new(move |event| {
                if let Ok(tx) = tx.lock() {
                    let _ = tx.send(event);
                }
            }),
        );
        (token, rx)
    }

    /// Feeds `model` the events of its build until it completes.
    fn finish_build(model: &FilterHierarchyModel, events: &mpsc::Receiver<Event>) {
        let deadline = std::time::Instant::now() + TIMEOUT;
        while model.is_building() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let event = events.recv_timeout(left).expect("the build did not finish");
            model.on_long_op(&event);
        }
    }

    fn labels(nodes: &[TreeNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.label.as_str()).collect()
    }

    #[test]
    fn a_build_fills_the_tree_of_every_filter_type() {
        let (ctx, _) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let model = FilterHierarchyModel::new(ctx.clone(), runtime, FilterType::Genre);
        let (_token, events) = collect_ops(&ctx.hub);
        assert!(model.tree().is_empty());

        model.reload();
        assert!(model.is_building());
        finish_build(&model, &events);

        assert_eq!(model.version().get(), 1);
        assert!(model.progress().get().is_none());
        assert_eq!(labels(&model.tree()), vec!["Pop", "Rock", "Synthpop"]);
        for filter_type in FilterType::ALL {
            assert!(!model.tree_of(filter_type).is_empty(), "{filter_type:?}");
        }
        assert!(!model.subfilters().is_empty());
    }

    #[test]
    fn the_progress_follows_the_operation() {
        let (ctx, _) = test_env::demo();
        let model =
            FilterHierarchyModel::new(ctx.clone(), AsyncRuntimeHandle::new(), FilterType::Artist);
        let (_token, events) = collect_ops(&ctx.hub);
        model.reload();
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + TIMEOUT;
        while model.is_building() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let event = events.recv_timeout(left).unwrap();
            model.on_long_op(&event);
            if matches!(event, Event::LongOperation(LongOpEvent::Progress(_))) {
                seen.push(model.progress().get());
            }
        }
        assert!(!seen.is_empty());
        assert!(
            seen.iter()
                .all(|p| p.is_some_and(|p| (0.0..=1.0).contains(&p)))
        );
        assert!(!model.is_building());
    }

    #[test]
    fn the_events_of_another_operation_are_ignored() {
        let (ctx, _) = test_env::demo();
        let model =
            FilterHierarchyModel::new(ctx.clone(), AsyncRuntimeHandle::new(), FilterType::Genre);
        let (_token, events) = collect_ops(&ctx.hub);
        let other = filters::build_filter_index(&ctx);
        // Wait for the other operation's events, then feed them to a model
        // that never started a build.
        let mut fed = 0;
        loop {
            let event = events.recv_timeout(TIMEOUT).unwrap();
            let done = matches!(&event, Event::LongOperation(LongOpEvent::Completed(info)) if info.op_id == other);
            model.on_long_op(&event);
            fed += 1;
            if done {
                break;
            }
        }
        assert!(fed >= 1);
        assert_eq!(model.version().get(), 0);
        assert!(
            ctx.ops.take_result::<FilterIndex>(other).is_some(),
            "the result was left alone"
        );
    }

    #[test]
    fn a_reload_during_a_build_cancels_the_running_one() {
        let (ctx, _) = test_env::demo();
        let model =
            FilterHierarchyModel::new(ctx.clone(), AsyncRuntimeHandle::new(), FilterType::Genre);
        let (_token, events) = collect_ops(&ctx.hub);
        model.reload();
        model.reload();
        finish_build(&model, &events);
        assert!(!model.tree().is_empty());
        assert_eq!(model.version().get(), 1, "only the second build was taken");
    }

    #[test]
    fn selecting_a_type_before_the_first_build_loads_just_that_type() {
        let (ctx, _) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let model = FilterHierarchyModel::new(ctx, runtime.clone(), FilterType::Genre);
        model.set_filter_type(FilterType::Year);
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        assert_eq!(model.filter_type().get(), FilterType::Year);
        assert_eq!(labels(&model.tree()), vec!["2015", "2019", "2021", "2023"]);
        assert!(model.tree_of(FilterType::Genre).is_empty());
        assert!(!model.is_building());
    }

    #[test]
    fn a_loaded_type_is_not_loaded_again() {
        let (ctx, _) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let model = FilterHierarchyModel::new(ctx, runtime.clone(), FilterType::Genre);
        model.set_filter_type(FilterType::Year);
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        model.set_filter_type(FilterType::Year);
        assert!(!run_until(
            &runtime,
            std::time::Duration::from_millis(200),
            || { model.version().get() > 1 }
        ));
    }

    #[test]
    fn a_burst_of_library_changes_starts_one_build() {
        let (ctx, _) = test_env::demo();
        let (mut tree, runtime) = tree_with_async(&ctx);
        let model = FilterHierarchyModel::new(ctx.clone(), runtime, FilterType::Genre);
        let slot = Rc::new(RefCell::new(None));
        {
            let (model, slot) = (model.clone(), slot.clone());
            test_env::mount(&mut tree, move |build| {
                *slot.borrow_mut() = Some(model.wire(build));
            });
        }
        let coalescer = slot.borrow().clone().unwrap();
        let wake = test_env::wake();
        for _ in 0..40 {
            coalescer.mark(&wake);
        }
        assert!(!model.is_building());
        tree.frame_tick().set(16.0);
        assert!(model.is_building());
        let first = model.inner.build_op.get();
        tree.frame_tick().set(16.0);
        assert_eq!(model.inner.build_op.get(), first, "no second build");
    }
}
