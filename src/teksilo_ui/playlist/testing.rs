//! The test environment of the playlist panel: services over the mock library
//! with the fakes behind them, and a view-model.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::time::Duration;

use teksilo::core::WidgetTree;
use teksilo::core::WindowStateInit;
use teksilo::core::accesskit::Role;
use teksilo::core::window::{TeksiloWindowId, WindowPlacement, WindowState};
use teksilo::platform::file_dialog::{FileDialogHandle, MemoryFileDialog};
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{FixedSize, Spacer, ToastInstallOptions, ToastRegistry, VStack};

use super::playlist_vm::PlaylistViewModel;
use super::rows::RowItem;
use crate::frontend::AppContext;
use crate::frontend::events::OpId;
use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
use crate::frontend::long_ops::OpStatus;
use crate::frontend::test_fixtures::SeededLibrary;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::mock_data::screenshot_library;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{layout, run_until, tree_with_app_state};

pub const TIMEOUT: Duration = Duration::from_secs(10);

/// What a test drives: the services, the fakes behind them, the seeded data
/// and the view-model.
pub struct Env {
    pub services: AppServices,
    pub fakes: InMemoryFakes,
    pub seeded: SeededLibrary,
    pub vm: PlaylistViewModel,
}

impl Env {
    pub fn runtime(&self) -> &AsyncRuntimeHandle {
        &self.services.runtime
    }

    pub fn wait(&self, done: impl Fn() -> bool) {
        assert!(run_until(self.runtime(), TIMEOUT, done), "timed out");
    }

    pub fn playlist_id(&self, name: &str) -> i64 {
        self.seeded.playlist(name).unwrap().id
    }

    pub fn song_id(&self, linear_idx: usize) -> i64 {
        self.vm.song_at(linear_idx).unwrap().song_id
    }

    pub fn rows(&self) -> Vec<usize> {
        self.vm
            .rows()
            .get()
            .iter()
            .map(RowItem::linear_idx)
            .collect()
    }

    pub fn selected(&self) -> Vec<usize> {
        let mut selected: Vec<usize> = self.vm.selection().get().selected.into_iter().collect();
        selected.sort_unstable();
        selected
    }

    pub fn loads(&self) -> usize {
        self.fakes
            .player
            .calls()
            .iter()
            .filter(|call| matches!(call, PlayerCall::Load { .. }))
            .count()
    }

    pub fn reload_content(&self) {
        let version = self.services.playlist_content.version().get();
        self.services.playlist_content.reload();
        self.wait(|| self.services.playlist_content.version().get() > version);
    }

    pub fn reload_tabs(&self) {
        let version = self.services.playlist_tabs.version().get();
        self.services.playlist_tabs.reload();
        self.wait(|| self.services.playlist_tabs.version().get() > version);
    }

    pub fn wait_op(&self, op: OpId) {
        let ops = self.services.ctx.ops.clone();
        self.wait(|| ops.status(op) != Some(OpStatus::Running));
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.services.ctx.shutdown();
    }
}

/// Services over the mock library with their fakes, and a view-model; the
/// session is not restored.
pub fn unrestored() -> Env {
    let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
    let seeded = screenshot_library()
        .seed_on(&ctx.db)
        .recv_blocking()
        .unwrap();
    let services = AppServices::new(&AppHandles::new(ctx), AsyncRuntimeHandle::new());
    let vm = PlaylistViewModel::new(&services);
    Env {
        services,
        fakes,
        seeded,
        vm,
    }
}

/// [`unrestored`], restored, with the tabs loaded.
pub fn restored() -> Env {
    let env = unrestored();
    env.services
        .session
        .start_restore(&env.services.ctx, &env.services.runtime);
    env.services.playlist_tabs.reload();
    env.wait(|| {
        env.services.session.status().get().is_settled()
            && env.services.playlist_tabs.list().len() == 3
    });
    env
}

/// The size of the playlist's region in the window (`app.rs`: the left
/// column under the player, in the test viewport).
pub const REGION: (f32, f32) = (400.0, 480.0);

/// A tree for `env` whose app state holds the executor, a toast registry and
/// an in-memory file dialog service, as the app installs them.
pub fn tree(env: &Env) -> (WidgetTree, ToastRegistry, FileDialogHandle) {
    let registry = ToastRegistry::new(ToastInstallOptions {
        archive: None,
        ..ToastInstallOptions::default()
    });
    let dialogs = FileDialogHandle::new(MemoryFileDialog::new());
    let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
    state.insert(
        TypeId::of::<AsyncRuntimeHandle>(),
        Box::new(env.services.runtime.clone()),
    );
    state.insert(TypeId::of::<ToastRegistry>(), Box::new(registry.clone()));
    state.insert(TypeId::of::<FileDialogHandle>(), Box::new(dialogs.clone()));
    let mut tree = tree_with_app_state(&env.services.ctx, state);
    // A window, as the app has one: the file dialogs are parented to it.
    tree.set_window_state(WindowState::new(WindowStateInit {
        id: TeksiloWindowId::new(1),
        string_id: Some("main".to_string()),
        placement: WindowPlacement::Floating,
        title: "Audoxidy".to_string(),
        size: (1360, 880),
        position: (0, 0),
        focused: true,
        resizable: true,
        always_on_top: false,
    }));
    (tree, registry, dialogs)
}

/// `widget` mounted at the window's top-left in a box of [`REGION`]; the
/// tree, the widget's id, the toast registry and the file dialogs.
pub fn mount(
    env: &Env,
    widget: impl Widget + 'static,
) -> (WidgetTree, WidgetId, ToastRegistry, FileDialogHandle) {
    mount_sized(env, widget, REGION)
}

/// [`mount`] in a box of `size`.
pub fn mount_sized(
    env: &Env,
    widget: impl Widget + 'static,
    size: (f32, f32),
) -> (WidgetTree, WidgetId, ToastRegistry, FileDialogHandle) {
    let (mut tree, registry, dialogs) = tree(env);
    let id = tree.add(widget);
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(FixedSize::new().width(size.0).height(size.1).child(id))
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    (tree, id, registry, dialogs)
}

/// Every node at or under `root` whose type name ends with `suffix`, in tree
/// order.
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

/// The menus (`Role::Menu` nodes) in the tree, in tree order.
pub fn menus(tree: &WidgetTree) -> Vec<WidgetId> {
    let mut menus = Vec::new();
    for root in tree.roots() {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if tree.accessibility_node(id).role() == Role::Menu {
                menus.push(id);
            }
            let mut children = tree.children(id);
            children.reverse();
            stack.extend(children);
        }
    }
    menus
}

/// The entries of the menu `menu` (dividers left out), with whether each is
/// enabled.
pub fn menu_entries(tree: &WidgetTree, menu: WidgetId) -> Vec<(String, bool)> {
    let mut entries = Vec::new();
    let mut stack = vec![menu];
    while let Some(id) = stack.pop() {
        let node = tree.accessibility_node(id);
        if node.role() == Role::MenuItem
            && let Some(name) = node.name()
        {
            entries.push((name.to_string(), !node.is_disabled()));
        }
        let mut children = tree.children(id);
        children.reverse();
        stack.extend(children);
    }
    entries
}

/// The entries of the last menu opened (a submenu that is not open has no
/// entries yet).
pub fn open_menu(tree: &WidgetTree) -> Vec<(String, bool)> {
    menus(tree)
        .into_iter()
        .map(|menu| menu_entries(tree, menu))
        .rfind(|entries| !entries.is_empty())
        .unwrap_or_default()
}
