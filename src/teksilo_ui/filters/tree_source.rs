//! [`FilterTreeSource`]: the filter tree as a `TreeDataSource`.
//!
//! The Iced panel flattens its pruned tree itself (`LibraryFiltersManager::
//! refresh_flattened_tree`, `library_filters.rs:121`) and renders a window of
//! the rows it computes by hand (`get_visible_tree_items`, `:126`). Teksilo's
//! `TreeView` virtualizes by itself, so this source only has to answer for
//! the whole flattened list: the rows (`logic::filter_index::flatten_tree`,
//! the Iced flattening), their keys (the node ids), the parents the stock
//! keyboard navigation ascends to, and the expansion state, which lives here
//! (Iced's `expanded_nodes`) and which a chevron, `→` and `←` change through
//! [`set_expanded`](TreeDataSource::set_expanded).
//!
//! The view-model replaces the pruned tree with [`set_tree`](FilterTreeSource::set_tree)
//! whenever the filter type, the letter, the search or the index changes; the
//! expansion survives, as Iced's does, until the view-model clears it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use teksilo::data::{FlatEntry, TreeDataSource};
use teksilo::prelude::Signal;

use crate::logic::filter_index::{self, TreeNode, VisibleTreeItem};

#[derive(Default)]
struct State {
    /// The ids of the expanded nodes (`expanded_nodes`).
    expanded: RefCell<HashSet<String>>,
    /// The flattened rows, depth first, descending into expanded nodes only.
    rows: RefCell<Vec<VisibleTreeItem>>,
    /// The row index of every visible id.
    index_of: RefCell<HashMap<String, usize>>,
    /// The parent of every node of the pruned tree (`None` for a root).
    parents: RefCell<HashMap<String, Option<String>>>,
    /// The children of every node of the pruned tree.
    children: RefCell<HashMap<String, Vec<String>>>,
    /// The pruned tree the rows come from.
    tree: RefCell<Vec<TreeNode>>,
}

/// The filter tree for a `TreeView`. Clones share one state.
#[derive(Clone)]
pub struct FilterTreeSource {
    state: Rc<State>,
    version: Signal<u64>,
}

impl std::fmt::Debug for FilterTreeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterTreeSource")
            .field("rows", &self.state.rows.borrow().len())
            .finish_non_exhaustive()
    }
}

impl Default for FilterTreeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterTreeSource {
    /// An empty source.
    pub fn new() -> Self {
        Self {
            state: Rc::new(State::default()),
            version: Signal::new(0),
        }
    }

    /// Replaces the pruned tree. The expansion is kept.
    pub fn set_tree(&self, tree: Vec<TreeNode>) {
        let mut parents = HashMap::new();
        let mut children = HashMap::new();
        fn walk(
            nodes: &[TreeNode],
            parent: Option<&str>,
            parents: &mut HashMap<String, Option<String>>,
            children: &mut HashMap<String, Vec<String>>,
        ) {
            for node in nodes {
                parents.insert(node.id.clone(), parent.map(str::to_string));
                children.insert(
                    node.id.clone(),
                    node.children.iter().map(|c| c.id.clone()).collect(),
                );
                walk(&node.children, Some(&node.id), parents, children);
            }
        }
        walk(&tree, None, &mut parents, &mut children);
        *self.state.parents.borrow_mut() = parents;
        *self.state.children.borrow_mut() = children;
        *self.state.tree.borrow_mut() = tree;
        self.reflatten();
    }

    /// Collapses every node (a new filter type starts collapsed).
    pub fn clear_expanded(&self) {
        self.state.expanded.borrow_mut().clear();
        self.reflatten();
    }

    /// Expands a collapsed node or collapses an expanded one
    /// (`Message::ToggleTreeNode`).
    pub fn toggle(&self, id: &str) {
        filter_index::toggle_node(&mut self.state.expanded.borrow_mut(), id);
        self.reflatten();
    }

    /// The flattened rows.
    pub fn rows(&self) -> Vec<VisibleTreeItem> {
        self.state.rows.borrow().clone()
    }

    /// The id of the row at `index`.
    pub fn id_at(&self, index: usize) -> Option<String> {
        self.state.rows.borrow().get(index).map(|r| r.id.clone())
    }

    /// Whether the pruned tree holds the node, visible or not.
    pub fn has_node(&self, id: &str) -> bool {
        self.state.parents.borrow().contains_key(id)
    }

    fn reflatten(&self) {
        let rows =
            filter_index::flatten_tree(&self.state.tree.borrow(), &self.state.expanded.borrow());
        *self.state.index_of.borrow_mut() = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.id.clone(), i))
            .collect();
        *self.state.rows.borrow_mut() = rows;
        self.version.set(self.version.get().wrapping_add(1));
    }
}

impl TreeDataSource for FilterTreeSource {
    type Item = VisibleTreeItem;
    type Key = String;

    fn visible_count(&self) -> usize {
        self.state.rows.borrow().len()
    }

    fn with_entry<R>(
        &self,
        flat_index: usize,
        f: impl FnOnce(&VisibleTreeItem, &FlatEntry<String>) -> R,
    ) -> Option<R> {
        let rows = self.state.rows.borrow();
        let row = rows.get(flat_index)?;
        let entry = FlatEntry {
            node_id: row.id.clone(),
            depth: row.depth,
            has_children: row.has_children,
            is_expanded: row.is_expanded,
        };
        Some(f(row, &entry))
    }

    fn key_at(&self, flat_index: usize) -> Option<String> {
        self.id_at(flat_index)
    }

    fn flat_index_of(&self, key: &String) -> Option<usize> {
        self.state.index_of.borrow().get(key).copied()
    }

    fn parent(&self, key: &String) -> Option<String> {
        self.state.parents.borrow().get(key).cloned().flatten()
    }

    fn child_keys(&self, key: &String) -> Vec<String> {
        self.state
            .children
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or_default()
    }

    fn version_signal(&self) -> Signal<u64> {
        self.version.clone()
    }

    fn is_expanded(&self, key: &String) -> bool {
        self.state.expanded.borrow().contains(key)
    }

    fn set_expanded(&self, key: &String, expanded: bool) {
        let changed = {
            let mut set = self.state.expanded.borrow_mut();
            if expanded {
                set.insert(key.clone())
            } else {
                set.remove(key)
            }
        };
        if changed {
            self.reflatten();
        }
    }

    fn contains_key(&self, key: &String) -> bool {
        self.has_node(key)
    }
}
