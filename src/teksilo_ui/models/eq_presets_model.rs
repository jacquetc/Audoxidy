//! `EqPresetsModel`: the equalizer's preset list, as the Load dialog shows it.
//!
//! A `ListModel<PresetEntry>` of the unified list (`PresetList::unified`, in
//! `logic::eq::unified_presets` order: the built-in "Default", the custom
//! presets A to Z, the visible built-ins A to Z), from
//! `commands::eq::presets`. Rows are matched by name and kind on every
//! update, so a row keeps its identity and the dialog's selection survives.
//!
//! The commands that change the presets (`save_preset`, `delete_preset`,
//! `restore_default_presets`) answer with the new [`PresetList`]; the model
//! replaces its rows from it ([`replace_from`](EqPresetsModel::replace_from),
//! or [`apply_reply`](EqPresetsModel::apply_reply) to await the reply).
//! Nothing publishes an event for the presets, so there is no `wire`.

use std::rc::Rc;

use teksilo::data::ListModel;
use teksilo::prelude::{AsyncRuntimeHandle, Signal};

use super::load::finish_tracked;
use super::reconcile::reconcile_unique;
use crate::frontend::commands::eq::{self, PresetEntry, PresetKind, PresetList};
use crate::frontend::{AppContext, FrontendError, Latest, Reply};

/// A key that tells entries apart: a custom preset may share its name with a
/// built-in.
fn key(entry: &PresetEntry) -> (String, u8) {
    let kind = match entry.kind {
        PresetKind::Default => 0,
        PresetKind::Custom => 1,
        PresetKind::Builtin => 2,
    };
    (entry.preset.name.clone(), kind)
}

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    list: ListModel<PresetEntry>,
    latest: Latest,
    version: Signal<u64>,
}

/// The preset list. UI thread only; clones share one list.
#[derive(Clone)]
pub struct EqPresetsModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for EqPresetsModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EqPresetsModel")
            .field("presets", &self.inner.list.len())
            .finish_non_exhaustive()
    }
}

impl EqPresetsModel {
    /// An empty model. Nothing is loaded until [`reload`](Self::reload).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                list: ListModel::new(),
                latest: Latest::new(),
                version: Signal::new(0),
            }),
        }
    }

    /// The presets, in the Load dialog's order.
    pub fn list(&self) -> ListModel<PresetEntry> {
        self.inner.list.clone()
    }

    /// A counter that bumps each time the list was replaced.
    pub fn version(&self) -> Signal<u64> {
        self.inner.version.clone()
    }

    /// The entry called `name` (the custom preset when a built-in has the same
    /// name).
    pub fn entry(&self, name: &str) -> Option<PresetEntry> {
        let list = &self.inner.list;
        (0..list.len()).find_map(|index| {
            list.with_item(index, |entry| {
                (entry.preset.name == name).then(|| entry.clone())
            })
            .flatten()
        })
    }

    /// Queries the presets again.
    pub fn reload(&self) {
        let inner = &self.inner;
        let reply = inner.latest.track(eq::presets(&inner.ctx));
        let model = self.clone();
        finish_tracked(
            &inner.runtime,
            "the equalizer presets",
            reply,
            move |list| {
                if let Some(list) = list {
                    model.replace_from(&list);
                }
            },
        );
    }

    /// Replaces the rows with the unified list of `list`, the answer of a
    /// preset command.
    pub fn replace_from(&self, list: &PresetList) {
        // Any older query is stale now: this list is newer than it.
        self.inner.latest.invalidate();
        reconcile_unique(&self.inner.list, list.unified(), key);
        let version = &self.inner.version;
        version.set(version.get().wrapping_add(1));
    }

    /// Awaits the answer of a preset command and replaces the rows from it,
    /// then calls `done` with the outcome (so the caller can show an error,
    /// quirk fix 8).
    pub fn apply_reply(
        &self,
        reply: Reply<PresetList>,
        done: impl FnOnce(Result<(), FrontendError>) + 'static,
    ) {
        let model = self.clone();
        self.inner
            .runtime
            .spawn_local(async move {
                match reply.recv().await {
                    Ok(list) => {
                        model.replace_from(&list);
                        done(Ok(()));
                    }
                    Err(error) => done(Err(error)),
                }
            })
            .detach();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::logic::eq::DEFAULT_PRESET_NAME;
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::run_until;

    fn names(model: &EqPresetsModel) -> Vec<String> {
        let list = model.list();
        (0..list.len())
            .filter_map(|i| list.with_item(i, |entry| entry.preset.name.clone()))
            .collect()
    }

    fn loaded() -> (AppContext, AsyncRuntimeHandle, EqPresetsModel) {
        let (ctx, _) = test_env::demo();
        let runtime = AsyncRuntimeHandle::new();
        let model = EqPresetsModel::new(ctx.clone(), runtime.clone());
        model.reload();
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        (ctx, runtime, model)
    }

    /// Settles an `apply_reply`.
    fn settle(runtime: &AsyncRuntimeHandle, model: &EqPresetsModel, version: u64) {
        assert!(run_until(runtime, TIMEOUT, || model.version().get() >= version));
    }

    #[test]
    fn the_list_starts_with_the_default_preset_then_the_built_ins_a_to_z() {
        let (_ctx, _runtime, model) = loaded();
        let names = names(&model);
        assert_eq!(names.first().map(String::as_str), Some(DEFAULT_PRESET_NAME));
        let mut rest: Vec<String> = names[1..].to_vec();
        let sorted = {
            let mut copy = rest.clone();
            copy.sort_by_key(|name| name.to_lowercase());
            copy
        };
        assert_eq!(rest, sorted);
        rest.retain(|name| name == DEFAULT_PRESET_NAME);
        assert!(rest.is_empty());
        assert_eq!(
            model.entry(DEFAULT_PRESET_NAME).unwrap().kind,
            PresetKind::Default
        );
    }

    #[test]
    fn saving_a_preset_adds_a_custom_row_after_the_default() {
        let (ctx, runtime, model) = loaded();
        let before = names(&model).len();
        let state = eq::load_state(&ctx).recv_blocking().unwrap();
        let outcome = Rc::new(RefCell::new(None));
        let sink = outcome.clone();
        model.apply_reply(eq::save_preset(&ctx, &state, "Zzz mine"), move |result| {
            *sink.borrow_mut() = Some(result.is_ok());
        });
        settle(&runtime, &model, 2);
        assert_eq!(*outcome.borrow(), Some(true));
        let names = names(&model);
        assert_eq!(names.len(), before + 1);
        assert_eq!(names[1], "Zzz mine", "customs come right after Default");
        assert_eq!(model.entry("Zzz mine").unwrap().kind, PresetKind::Custom);
    }

    #[test]
    fn deleting_a_custom_preset_removes_its_row_and_hiding_a_built_in_removes_that_one() {
        let (ctx, runtime, model) = loaded();
        let state = eq::load_state(&ctx).recv_blocking().unwrap();
        model.apply_reply(eq::save_preset(&ctx, &state, "Mine"), |_| {});
        settle(&runtime, &model, 2);
        model.apply_reply(eq::delete_preset(&ctx, &state, "Mine"), |_| {});
        settle(&runtime, &model, 3);
        assert!(model.entry("Mine").is_none());

        let builtin = names(&model)
            .into_iter()
            .find(|name| name != DEFAULT_PRESET_NAME)
            .unwrap();
        model.apply_reply(eq::delete_preset(&ctx, &state, &builtin), |_| {});
        settle(&runtime, &model, 4);
        assert!(model.entry(&builtin).is_none(), "quirk fix 14: hidden");

        model.apply_reply(eq::restore_default_presets(&ctx), |_| {});
        settle(&runtime, &model, 5);
        assert!(model.entry(&builtin).is_some(), "restored");
    }

    #[test]
    fn a_failed_command_reports_its_error_and_keeps_the_rows() {
        let (ctx, runtime, model) = loaded();
        let state = eq::load_state(&ctx).recv_blocking().unwrap();
        let before = names(&model);
        let outcome = Rc::new(RefCell::new(None));
        let sink = outcome.clone();
        model.apply_reply(eq::save_preset(&ctx, &state, ""), move |result| {
            *sink.borrow_mut() = Some(result);
        });
        assert!(run_until(&runtime, TIMEOUT, || outcome.borrow().is_some()));
        assert!(matches!(
            outcome.borrow().as_ref(),
            Some(Err(FrontendError::InvalidInput(_)))
        ));
        assert_eq!(names(&model), before);
        assert_eq!(model.version().get(), 1);
    }

    #[test]
    fn a_command_answer_supersedes_an_older_query() {
        let (ctx, runtime, model) = loaded();
        let state = eq::load_state(&ctx).recv_blocking().unwrap();
        // The query is sent first, the command's answer arrives (and
        // replaces the rows) first, then the older answer must not undo it.
        model.reload();
        model.apply_reply(eq::save_preset(&ctx, &state, "Fresh"), |_| {});
        assert!(run_until(&runtime, TIMEOUT, || model
            .entry("Fresh")
            .is_some()));
        runtime.tick();
        runtime.tick();
        assert!(model.entry("Fresh").is_some());
    }
}
