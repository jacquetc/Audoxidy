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

    use std::any::{Any, TypeId};
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    use teksilo::core::accessibility::widget_id_to_node_id;
    use teksilo::core::accesskit::Role;
    use teksilo::core::widget_id::WidgetId;
    use teksilo::core::widget_tree::WidgetTree;
    use teksilo::core::window::{WindowPlacement, WindowState};
    use teksilo::core::{TeksiloWindowId, WindowStateInit};
    use teksilo::prelude::AsyncRuntimeHandle;
    use teksilo::widgets::{NotificationArchiveModel, ToastInstallOptions, ToastRegistry};

    use crate::frontend::AppContext;
    use crate::frontend::fakes::InMemoryFakes;
    use crate::teksilo_ui::app::services::AppServices;
    use crate::teksilo_ui::shell::main_window::AppHandles;
    use crate::teksilo_ui::test_support::tree_with_app_state;

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
    /// platform adapter receives it, names `name` (a text widget is named by
    /// its text). Unlike
    /// `test_support::find_by_name` it skips the parked pages of a `Switcher`
    /// and widgets hidden with `visible_when`: asking those for their node
    /// panics.
    pub fn find_by_name(tree: &mut WidgetTree, root: WidgetId, name: &str) -> Vec<WidgetId> {
        let published: HashSet<_> = tree
            .sync_accessibility()
            .nodes
            .into_iter()
            .filter(|(_, node)| {
                node.label() == Some(name)
                    || (node.role() == Role::Label && node.value() == Some(name))
            })
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

    /// Every widget at or under `root` whose published accessibility node has
    /// `role`, in tree order, skipping parked ones.
    pub fn all_of_role(tree: &mut WidgetTree, root: WidgetId, role: Role) -> Vec<WidgetId> {
        let published: HashSet<_> = tree
            .sync_accessibility()
            .nodes
            .into_iter()
            .filter(|(_, node)| node.role() == role)
            .map(|(id, _)| id)
            .collect();
        let mut found = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if published.contains(&widget_id_to_node_id(id)) {
                found.push(id);
            }
            stack.extend(tree.children(id).into_iter().rev());
        }
        found
    }

    /// The toasts a test tree has shown.
    pub struct Toasts {
        archive: Rc<NotificationArchiveModel>,
    }

    impl Toasts {
        /// The toasts shown so far, oldest first, as their debug text
        /// (severity and title included).
        pub fn shown(&self) -> Vec<String> {
            let entries = self.archive.entries();
            (0..entries.len())
                .filter_map(|index| entries.with_item(index, |entry| format!("{entry:?}")))
                .collect()
        }
    }

    /// A tree over `services` with the main-thread executor of the services and
    /// a toast registry (with an in-memory archive) in its app state, as the
    /// app installs them, and the toasts it will show.
    pub fn tree_for(services: &AppServices) -> (WidgetTree, Toasts) {
        let archive = Rc::new(NotificationArchiveModel::in_memory());
        let registry = ToastRegistry::with_archive(ToastInstallOptions::default(), archive.clone());
        let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
        state.insert(
            TypeId::of::<AsyncRuntimeHandle>(),
            Box::new(services.runtime.clone()),
        );
        state.insert(TypeId::of::<ToastRegistry>(), Box::new(registry.clone()));
        let mut tree = tree_with_app_state(&services.ctx, state);
        // Work started from a handler delivers its result to the window that
        // started it (`spawn_local_with`), so the tree needs one.
        tree.set_window_state(WindowState::new(WindowStateInit {
            id: TeksiloWindowId::new(1),
            string_id: None,
            placement: WindowPlacement::Floating,
            title: "Audoxidy".to_string(),
            size: (1360, 880),
            position: (0, 0),
            focused: true,
            resizable: true,
            always_on_top: false,
        }));
        (tree, Toasts { archive })
    }
}
