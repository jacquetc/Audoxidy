//! [`FiltersViewModel`]: the state of the filter column and what selecting
//! in it does to the library.
//!
//! Port of `LibraryFiltersManager` (`src/gui/library_filters.rs:25`) and of
//! the app's handlers for it (`src/gui/app.rs:4241-4390`):
//!
//! - the **filter type** (Genre at start) lives in the hierarchy model, which
//!   loads the tree of a type on first use; choosing one
//!   ([`set_filter_type`](FiltersViewModel::set_filter_type),
//!   `ChangeGeneralFilter`) clears the letter, the selected node and the
//!   expansion, and clears the library's artist / album / genre / year /
//!   folder filters;
//! - the **letter** (`selected_subfilter`): clicking the selected letter, or
//!   the restore icon while none is selected, clears it
//!   (`logic::filter_index::next_subfilter`); clearing it also clears the
//!   library's filters, choosing a letter leaves them (`SelectSubfilter`);
//! - the **search** prunes the tree and leaves the library alone
//!   (`FilterSearchChanged`);
//! - a **node** is selected on a click or Enter ([`select_node`](FiltersViewModel::select_node),
//!   `SelectTreeNode`): its id becomes the library's filter through
//!   `logic::filter_index::filters_for_node`, written to
//!   [`LibrarySongsModel::update_params`](crate::teksilo_ui::models::LibrarySongsModel::update_params);
//!   the library's search text is left as it is;
//! - the **expansion** is the tree source's (`ToggleTreeNode`).
//!
//! **Scrolling (G1).** Choosing a type, a letter or the restore icon, and
//! typing in the search, bring the tree back to its top
//! ([`tree_scroll`](FiltersViewModel::tree_scroll); `src/gui/app.rs:4274-4282`,
//! `:4293`, `:4379-4386`). Choosing a type, a letter, the restore icon or a
//! node also brings the library back to its top and hands its figures to the
//! filter until the filter's songs arrive (`:4265-4282`, `:4296-4316`,
//! `:4330-4354`): the column says so on
//! [`library_reset`](FiltersViewModel::library_reset), which the App passes to
//! the library (`LibraryViewModel::on_filter_changed`), since the two
//! view-models do not know each other.
//!
//! The view is the pruned tree (`apply_view`, `:104`): the letter and the
//! search applied to the full tree of the shown type, recomputed whenever one
//! of them, the type or the index changes. The Iced panel draws nothing under
//! its selector while the index is empty ("while the index builds"): neither
//! the letters nor the tree; [`subfilters`](FiltersViewModel::subfilters) is
//! empty then and the panel follows it.

use std::cell::RefCell;
use std::rc::Rc;

use teksilo::data::{KeyedSelectionModel, SelectionMode};
use teksilo::prelude::{BuildContext, Signal};

use super::tree_source::FilterTreeSource;
use crate::logic::filter_index::{self, FilterType, VisibleTreeItem};
use crate::teksilo_ui::app::services::{AppServices, Request};
use crate::teksilo_ui::models::{FilterHierarchyModel, LibrarySongsModel};

/// What the pruned view was last computed from, so an unchanged request is
/// not recomputed.
#[derive(Clone, PartialEq)]
struct Applied {
    filter_type: FilterType,
    version: u64,
    subfilter: Option<String>,
    search: String,
}

struct Inner {
    hierarchy: FilterHierarchyModel,
    library: LibrarySongsModel,
    subfilter: Signal<Option<String>>,
    subfilters: Signal<Vec<String>>,
    search: Signal<String>,
    selected_node: Signal<Option<String>>,
    selection: KeyedSelectionModel<String>,
    source: FilterTreeSource,
    applied: RefCell<Option<Applied>>,
    tree_scroll: Signal<f32>,
    library_reset: Request<()>,
}

/// The filter column's view-model. UI thread only; clones share one state.
#[derive(Clone)]
pub struct FiltersViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for FiltersViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FiltersViewModel")
            .field("filter_type", &self.inner.hierarchy.filter_type().get())
            .field("subfilter", &self.inner.subfilter.get())
            .field("search", &self.inner.search.get())
            .field("selected_node", &self.inner.selected_node.get())
            .finish_non_exhaustive()
    }
}

impl FiltersViewModel {
    /// The column's state over the hierarchy and library models of
    /// `services`.
    pub fn new(services: &AppServices) -> Self {
        let vm = Self {
            inner: Rc::new(Inner {
                hierarchy: services.filter_hierarchy.clone(),
                library: services.library_songs.clone(),
                subfilter: Signal::new(None),
                subfilters: Signal::new(Vec::new()),
                search: Signal::new(String::new()),
                selected_node: Signal::new(None),
                selection: KeyedSelectionModel::new(SelectionMode::Single),
                source: FilterTreeSource::new(),
                applied: RefCell::new(None),
                tree_scroll: Signal::new(0.0),
                library_reset: Request::default(),
            }),
        };
        vm.refresh();
        vm
    }

    /// Follows the index and the search field. Call it from the panel's
    /// `build`; the effects live as long as the panel.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let vm = self.clone();
        ctx.effect(&self.inner.hierarchy.version(), move |_| vm.refresh());
        let vm = self.clone();
        ctx.effect(&self.inner.search, move |_| vm.on_search_changed());
        self.refresh();
    }

    // ---- state ----

    /// The shown filter type (Genre at start).
    pub fn filter_type(&self) -> Signal<FilterType> {
        self.inner.hierarchy.filter_type()
    }

    /// The selected letter, `None` for all (`selected_subfilter`).
    pub fn subfilter(&self) -> Signal<Option<String>> {
        self.inner.subfilter.clone()
    }

    /// The letters of the shown type, in Iced's order (`active_subfilters`):
    /// empty until the index holds the type.
    pub fn subfilters(&self) -> Signal<Vec<String>> {
        self.inner.subfilters.clone()
    }

    /// The search text. The search field edits this signal.
    pub fn search(&self) -> Signal<String> {
        self.inner.search.clone()
    }

    /// The node whose click or Enter last filtered the library
    /// (`selected_tree_node`).
    pub fn selected_node(&self) -> Signal<Option<String>> {
        self.inner.selected_node.clone()
    }

    /// The tree view's selection (the highlighted row; keyboard navigation
    /// moves it, Enter and a click make it the [`selected_node`](Self::selected_node)).
    pub fn selection(&self) -> KeyedSelectionModel<String> {
        self.inner.selection.clone()
    }

    /// The tree the view reads.
    pub fn tree_source(&self) -> FilterTreeSource {
        self.inner.source.clone()
    }

    /// The tree's rows as displayed (the pruned tree, flattened).
    pub fn visible_items(&self) -> Vec<VisibleTreeItem> {
        self.inner.source.rows()
    }

    /// The tree's vertical scroll offset; the tree view scrolls by it, and the
    /// column's actions write 0 to it (G1).
    pub fn tree_scroll(&self) -> Signal<f32> {
        self.inner.tree_scroll.clone()
    }

    /// Sent each time a type, a letter, the restore icon or a node is
    /// chosen, after the library's filters are set: the library goes back to
    /// its top and gives its figures to the filter (G1, G6). The App passes
    /// it to `LibraryViewModel::on_filter_changed`.
    pub fn library_reset(&self) -> Signal<Option<(u64, ())>> {
        self.inner.library_reset.latest()
    }

    /// Whether an index build is running (the index is empty until the first
    /// one completes).
    pub fn is_building(&self) -> bool {
        self.inner.hierarchy.is_building()
    }

    // ---- actions ----

    /// `ChangeGeneralFilter`: shows `filter_type` with no letter, no selected
    /// node and everything collapsed at the tree's top, and clears the
    /// library's filters; the library goes back to its top
    /// (`src/gui/app.rs:4276-4282`).
    pub fn set_filter_type(&self, filter_type: FilterType) {
        let inner = &self.inner;
        inner.subfilter.set(None);
        inner.selected_node.set(None);
        inner.selection.clear();
        inner.source.clear_expanded();
        inner.hierarchy.set_filter_type(filter_type);
        self.clear_library_filters();
        self.refresh();
        inner.tree_scroll.set(0.0);
        inner.library_reset.send(());
    }

    /// `SelectSubfilter`: `letter` becomes the letter, or clears it when it
    /// already is (`None`, the restore icon, clears it too). The selected node
    /// is dropped; when no letter remains the library's filters are cleared,
    /// otherwise they stay as they were. The tree and the library go back to
    /// their top (`src/gui/app.rs:4293`, `:4313-4316`).
    pub fn select_subfilter(&self, letter: Option<&str>) {
        let inner = &self.inner;
        let next = filter_index::next_subfilter(inner.subfilter.get().as_deref(), letter);
        inner.subfilter.set(next.clone());
        inner.selected_node.set(None);
        inner.selection.clear();
        if next.is_none() {
            self.clear_library_filters();
        }
        self.refresh();
        inner.tree_scroll.set(0.0);
        inner.library_reset.send(());
    }

    /// Clears the letter and the selected node and leaves the library's
    /// filters alone: what the Iced UI does to the filter column when a
    /// reveal has cleared the library's search and filters itself
    /// (`src/gui/app.rs:6108-6110`). The app calls it on
    /// [`LibraryViewModel::reveal_cleared`](crate::teksilo_ui::library::LibraryViewModel::reveal_cleared).
    pub fn clear_highlight(&self) {
        let inner = &self.inner;
        inner.subfilter.set(None);
        inner.selected_node.set(None);
        inner.selection.clear();
        self.refresh();
    }

    /// `SelectTreeNode`: `id` becomes the selected node and the library is
    /// filtered by it (`filters_for_node`), back at its top
    /// (`src/gui/app.rs:4349-4354`); the tree stays where it is. A folder
    /// node filters nothing, as in Iced.
    pub fn select_node(&self, id: &str) {
        let inner = &self.inner;
        inner.selected_node.set(Some(id.to_string()));
        if inner.selection.selected_keys() != [id.to_string()] {
            inner.selection.select(id.to_string());
        }
        let filter_type = inner.hierarchy.filter_type().get();
        // TODO(iced-stub): the Iced UI selects the folder node but applies no
        // library filter (SelectFolder is never emitted)
        let filters = filter_index::filters_for_node(filter_type, id);
        inner.library.update_params(|params| {
            params.artist = filters.artist;
            params.album = filters.album;
            params.genre = filters.genre;
            params.year = filters.year;
            params.folder_id = None;
        });
        inner.library_reset.send(());
    }

    /// `ToggleTreeNode`: expands or collapses `id`.
    pub fn toggle_node(&self, id: &str) {
        self.inner.source.toggle(id);
    }

    /// `FilterSearchChanged`: prunes the tree by `text` and brings it back to
    /// its top (`src/gui/app.rs:4379-4386`); the library is left alone.
    pub fn set_search(&self, text: &str) {
        self.inner.search.set(text.to_string());
        self.on_search_changed();
    }

    /// The search text changed (the field edits the signal; the panel's
    /// effect calls this): the pruned tree, at its top.
    fn on_search_changed(&self) {
        self.refresh();
        self.inner.tree_scroll.set(0.0);
    }

    /// Recomputes the pruned tree and the letters from the index, the shown
    /// type, the letter and the search (`apply_view`). Does nothing when none
    /// of them changed since the last time.
    pub fn refresh(&self) {
        let inner = &self.inner;
        let now = Applied {
            filter_type: inner.hierarchy.filter_type().get(),
            version: inner.hierarchy.version().get(),
            subfilter: inner.subfilter.get(),
            search: inner.search.get(),
        };
        if inner.applied.borrow().as_ref() == Some(&now) {
            return;
        }
        let letters = inner.hierarchy.subfilters();
        if inner.subfilters.get() != letters {
            inner.subfilters.set(letters);
        }
        let pruned = filter_index::apply_view(
            &inner.hierarchy.tree(),
            now.subfilter.as_deref(),
            &now.search,
        );
        inner.source.set_tree(pruned);
        *inner.applied.borrow_mut() = Some(now);
    }

    /// Clears the library's artist / album / genre / year / folder filters,
    /// as `ChangeGeneralFilter` and `SelectSubfilter(None)` do.
    fn clear_library_filters(&self) {
        self.inner.library.update_params(|params| {
            params.artist = None;
            params.album = None;
            params.genre = None;
            params.year = None;
            params.folder_id = None;
        });
    }
}

#[cfg(test)]
mod tests;
