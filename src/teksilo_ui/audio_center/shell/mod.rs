//! The Audio Center's shell: the floating 940 × 480 panel (frame, header,
//! tab strip, dragging, closing) and its view-model.
//!
//! - [`AudioCenterViewModel`] holds what the shell owns: whether the panel is
//!   open, on which tab, and where the user dragged it
//!   (`AppServices::links.audio_center`, whose `toggle` is Iced's
//!   `ToggleAudioCenter`), and Iced's drag rule with its clamp;
//! - [`AudioCenterPanel`] is the panel, taking the four tab bodies as widgets.
//!
//! Closing the Audio Center has nothing left to save. Iced's `Close` calls
//! `save_eq_settings_to_db` and `save_dsp_settings_to_db`
//! (`src/gui/audio_center.rs:1601`); the facade's equalizer, effects and mix
//! commands persist every change as it is made, so the stored settings
//! already equal the engine's (pinned by
//! `the_equalizer_and_the_effects_are_stored_without_a_close`).

pub mod audio_center_vm;
pub mod panel;

pub use audio_center_vm::AudioCenterViewModel;
pub use panel::AudioCenterPanel;

#[cfg(test)]
pub(crate) mod testing {
    //! Scaffolding for the tests of the Audio Center's view-models and views:
    //! services over an in-memory context with its fakes, and a finder of
    //! accessible names that tolerates the panel's parked (dormant) tabs.

    use std::collections::HashSet;

    use teksilo::core::accessibility::widget_id_to_node_id;
    use teksilo::core::widget_id::WidgetId;
    use teksilo::core::widget_tree::WidgetTree;
    use teksilo::prelude::AsyncRuntimeHandle;

    use crate::frontend::AppContext;
    use crate::frontend::fakes::InMemoryFakes;
    use crate::teksilo_ui::app::services::AppServices;
    use crate::teksilo_ui::shell::main_window::AppHandles;

    /// Services over a fresh in-memory context, and the fakes behind its
    /// player and sound server. Nothing is loaded.
    pub fn services_with_fakes() -> (AppServices, InMemoryFakes) {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        (
            AppServices::new(&AppHandles::new(ctx), AsyncRuntimeHandle::new()),
            fakes,
        )
    }

    /// Every widget at or under `root` that the accessibility tree, as the
    /// platform adapter receives it, names `name`. Unlike
    /// `test_support::find_by_name` it skips the parked pages of a `Switcher`
    /// and widgets hidden with `visible_when`: asking those for their node
    /// panics.
    pub fn find_by_name(tree: &mut WidgetTree, root: WidgetId, name: &str) -> Vec<WidgetId> {
        let published: HashSet<_> = tree
            .sync_accessibility()
            .nodes
            .into_iter()
            .filter(|(_, node)| node.label() == Some(name))
            .map(|(id, _)| id)
            .collect();
        let mut found = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if published.contains(&widget_id_to_node_id(id)) {
                found.push(id);
            }
            stack.extend(tree.children(id));
        }
        found
    }
}
