//! Scaffolding shared by the library's view tests.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::time::Duration;

use teksilo::core::WidgetTree;
use teksilo::prelude::*;
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{FixedSize, HStack, Spacer, VStack};

use super::LibraryViewModel;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::app::services::testing::in_memory_services;
use crate::teksilo_ui::test_support::{VIEWPORT, run_until, tree_with_app_state};

/// How long a test waits for a worker's answer.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The library region of the app (`app.rs`): 602 px from the left, 40 px from
/// the top, in the test viewport.
pub const REGION: (f32, f32) = (VIEWPORT.0 - 602.0, VIEWPORT.1 - 40.0);

/// Services over the mock library, loaded, and a view-model over them.
pub fn loaded() -> (AppServices, LibraryViewModel) {
    let services = in_memory_services();
    services.library_songs.reload();
    let version = services.library_songs.version();
    assert!(run_until(&services.runtime, TIMEOUT, || version.get() >= 1));
    let vm = LibraryViewModel::with_low_resource(&services, false);
    (services, vm)
}

/// Waits until the library model answered its latest request.
pub fn settle(services: &AppServices) {
    let loading = services.library_songs.loading();
    assert!(run_until(&services.runtime, TIMEOUT, || !loading.get()));
    services.runtime.tick();
}

/// A tree whose app state holds the services' executor, as `install_async`
/// registers it.
pub fn tree_for(services: &AppServices) -> WidgetTree {
    let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
    state.insert(
        TypeId::of::<AsyncRuntimeHandle>(),
        Box::new(services.runtime.clone()),
    );
    tree_with_app_state(&services.ctx, state)
}

/// Mounts `widget` at the window's top-leading corner, `width` × `height`.
pub fn mount_at(
    tree: &mut WidgetTree,
    widget: impl Widget + 'static,
    width: f32,
    height: f32,
) -> WidgetId {
    let id = tree.add(widget);
    let sized = tree.add(FixedSize::new().width(width).height(height).child(id));
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Top)
                    .child(sized)
                    .child(Spacer::new()),
            )
            .child(Spacer::new()),
    );
    id
}

/// Lays `tree` out twice: a width written from a layout pass takes effect on
/// the next one (the grid's column count).
pub fn settle_layout(tree: &mut WidgetTree) {
    crate::teksilo_ui::test_support::layout(tree);
    crate::teksilo_ui::test_support::layout(tree);
}

/// Every node at or under `root` whose type name ends with `suffix`, depth
/// first.
pub fn all_of_type(tree: &WidgetTree, root: WidgetId, suffix: &str) -> Vec<WidgetId> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if tree
            .widget_type_name(id)
            .is_some_and(|name| name.ends_with(suffix))
        {
            found.push(id);
        }
        let mut children = tree.children(id);
        children.reverse();
        stack.extend(children);
    }
    found
}

/// `text` without the Unicode isolation marks Fluent puts around a variable.
pub fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
        .collect()
}

/// The nodes among `ids` that the next accessibility update re-sends as
/// selected. Sync once before the change, so that only what changed since
/// is sent.
pub fn resent_selected(tree: &mut WidgetTree, ids: &[WidgetId]) -> Vec<WidgetId> {
    let update = tree.sync_accessibility();
    ids.iter()
        .copied()
        .filter(|id| {
            let wanted = teksilo::core::accessibility::widget_id_to_node_id(*id);
            update
                .nodes
                .iter()
                .any(|(node, data)| *node == wanted && data.is_selected() == Some(true))
        })
        .collect()
}

/// The stacks at or under `root` whose children spill past their bounds by
/// more than half a pixel, as the inspector's overflow overlay finds them
/// (`docs/inspector.md`, "Overflow overlay": `HStack` and `VStack` only):
/// each with the overhanging child's type and bounds.
pub fn overflowing_stacks(tree: &WidgetTree, root: WidgetId) -> Vec<String> {
    let short = |id: WidgetId| {
        let name = tree.widget_type_name(id).unwrap_or("?");
        name.rsplit("::").next().unwrap_or(name).to_string()
    };
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let children = tree.children(id);
        let kind = short(id);
        if kind == "HStack" || kind == "VStack" {
            let outer = tree.bounds(id);
            for child in &children {
                let inner = tree.bounds(*child);
                // Nothing drawn (a hidden child is not laid out).
                if inner.width <= 0.0 || inner.height <= 0.0 {
                    continue;
                }
                let over = (outer.x - inner.x)
                    .max(outer.y - inner.y)
                    .max(inner.x + inner.width - (outer.x + outer.width))
                    .max(inner.y + inner.height - (outer.y + outer.height));
                if over > 0.5 {
                    found.push(format!(
                        "{kind} {outer:?} overflowed by {} {inner:?}",
                        short(*child)
                    ));
                }
            }
        }
        stack.extend(children);
    }
    found
}
