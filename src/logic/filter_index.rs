//! The filter panel: a three-level tree per filter type, its alphabetical
//! subfilters, search pruning, flattening for display, and the mapping from a
//! selected node to library filters.
//!
//! Extracted from the Iced `LibraryFiltersManager` (`build_filter_index`,
//! `get_group_char`, `sort_tree`, `sort_subfilters`, `apply_view`,
//! `filter_node_by_search`, `flatten_tree_node`, `get_visible_tree_items`) and
//! the `SelectTreeNode` / `SelectSubfilter` handlers. The database query stays
//! with the caller, which passes the hierarchy rows in.

use crate::logic::id_codec;
use crate::utils::compare_strings_ignore_case;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// Group label of entries whose first character is a digit.
pub const DIGIT_GROUP: &str = "#";

/// Group label of entries whose first character is neither a digit nor a letter.
pub const SYMBOL_GROUP: &str = "•";

/// Height of a row of the filter tree.
pub const TREE_ROW_HEIGHT: f32 = 32.0;

/// The facet the filter panel browses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterType {
    /// Folder → artist → album. Selecting a node applies no library filter.
    Folder,
    /// Artist → album.
    Artist,
    /// Album → artist.
    Album,
    /// Genre → artist → album.
    Genre,
    /// Year → artist → album.
    Year,
}

impl FilterType {
    /// Every filter type, in the order of the filter menu.
    pub const ALL: [FilterType; 5] = [
        FilterType::Folder,
        FilterType::Artist,
        FilterType::Album,
        FilterType::Genre,
        FilterType::Year,
    ];

    /// The type's name: the key of the database hierarchy query and the first
    /// part of every node id (it equals the `Debug` output).
    pub fn name(self) -> &'static str {
        match self {
            FilterType::Folder => "Folder",
            FilterType::Artist => "Artist",
            FilterType::Album => "Album",
            FilterType::Genre => "Genre",
            FilterType::Year => "Year",
        }
    }
}

/// A node of a filter tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    /// Displayed label.
    pub label: String,
    /// Unique id: `"<FilterType name>|L1|L2|L3"` down to the node's level,
    /// e.g. `"Genre|Rock|Queen|Innuendo"`.
    pub id: String,
    /// Child nodes, sorted (see [`sort_tree`]).
    pub children: Vec<TreeNode>,
}

/// A row of the flattened filter tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleTreeItem {
    /// Displayed label.
    pub label: String,
    /// Node id.
    pub id: String,
    /// Nesting level, 0 for top-level nodes.
    pub depth: usize,
    /// Whether the node has children (shows an expander).
    pub has_children: bool,
    /// Whether the node is expanded.
    pub is_expanded: bool,
}

/// One row of the database filter hierarchy: level 1, level 2 and the
/// optional level 3 label.
pub type HierarchyRow = (Arc<str>, Arc<str>, Option<Arc<str>>);

/// The tree and the available subfilters of every filter type, built once
/// when the library is loaded.
#[derive(Debug, Clone, Default)]
pub struct FilterIndex {
    trees: HashMap<FilterType, Vec<TreeNode>>,
    subfilters: HashMap<FilterType, Vec<String>>,
}

impl FilterIndex {
    /// An index with no filter type (every tree and subfilter list empty).
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the tree and subfilters of `filter_type` from its hierarchy rows,
    /// replacing any previous ones.
    pub fn insert(
        &mut self,
        filter_type: FilterType,
        rows: impl IntoIterator<Item = HierarchyRow>,
    ) {
        let (tree, subfilters) = build_tree(filter_type, rows);
        self.trees.insert(filter_type, tree);
        self.subfilters.insert(filter_type, subfilters);
    }

    /// The full sorted tree of `filter_type`, if it was built.
    pub fn tree(&self, filter_type: FilterType) -> Option<&[TreeNode]> {
        self.trees.get(&filter_type).map(Vec::as_slice)
    }

    /// The sorted initial-character groups of `filter_type`, if it was built.
    pub fn subfilters(&self, filter_type: FilterType) -> Option<&[String]> {
        self.subfilters.get(&filter_type).map(Vec::as_slice)
    }
}

/// The initial-character group of a label: [`DIGIT_GROUP`] for a leading
/// digit, the uppercased first letter for a leading letter, [`SYMBOL_GROUP`]
/// otherwise (including the empty label).
pub fn group_char(label: &str) -> String {
    let first = label
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .next()
        .unwrap_or('?');
    if first.is_numeric() {
        DIGIT_GROUP.to_string()
    } else if first.is_alphabetic() {
        first.to_string()
    } else {
        SYMBOL_GROUP.to_string()
    }
}

/// Sort weight of a group: digits, then symbols, then letters.
fn group_weight(group: &str) -> u8 {
    if group == DIGIT_GROUP {
        0
    } else if group == SYMBOL_GROUP {
        1
    } else {
        2
    }
}

/// Sorts subfilter groups: [`DIGIT_GROUP`] first, [`SYMBOL_GROUP`] second,
/// then letters in code-point order.
pub fn sort_subfilters(groups: &mut [String]) {
    groups.sort_by(|a, b| group_weight(a).cmp(&group_weight(b)).then_with(|| a.cmp(b)));
}

/// Sorts nodes recursively (stable): labels starting with a digit first, then
/// those starting with a symbol, then letters; case-insensitively within a group.
pub fn sort_tree(nodes: &mut [TreeNode]) {
    nodes.sort_by(|a, b| {
        group_weight(&group_char(&a.label))
            .cmp(&group_weight(&group_char(&b.label)))
            .then_with(|| compare_strings_ignore_case(&a.label, &b.label))
    });
    for node in nodes.iter_mut() {
        if !node.children.is_empty() {
            sort_tree(&mut node.children);
        }
    }
}

/// Builds the sorted three-level tree of `filter_type` and its sorted
/// initial-character groups (of the level-1 labels) from hierarchy rows.
///
/// Rows sharing a level-1 label become one node, and likewise for level 2
/// under the same level 1; level-3 labels are deduplicated. Node ids are
/// `"<FilterType name>|L1"`, `"…|L1|L2"` and `"…|L1|L2|L3"`, joined with
/// [`id_codec::join_parts`] so that a `|` or `\` in a label is escaped.
pub fn build_tree(
    filter_type: FilterType,
    rows: impl IntoIterator<Item = HierarchyRow>,
) -> (Vec<TreeNode>, Vec<String>) {
    let mut levels: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    let mut groups: HashSet<String> = HashSet::new();

    for (l1, l2, l3) in rows {
        groups.insert(group_char(&l1));
        let l3_set = levels
            .entry(l1.to_string())
            .or_default()
            .entry(l2.to_string())
            .or_default();
        if let Some(l3) = l3 {
            l3_set.insert(l3.to_string());
        }
    }

    let mut tree: Vec<TreeNode> = Vec::with_capacity(levels.len());
    let type_name = filter_type.name();
    for (l1_label, l2_map) in levels {
        let id1 = id_codec::join_parts(&[type_name, &l1_label]);
        let mut children1: Vec<TreeNode> = Vec::with_capacity(l2_map.len());
        for (l2_label, l3_set) in l2_map {
            let id2 = id_codec::join_parts(&[type_name, &l1_label, &l2_label]);
            let mut children2: Vec<TreeNode> = l3_set
                .into_iter()
                .map(|l3| TreeNode {
                    id: id_codec::join_parts(&[type_name, &l1_label, &l2_label, &l3]),
                    label: l3,
                    children: Vec::new(),
                })
                .collect();
            sort_tree(&mut children2);
            children1.push(TreeNode {
                label: l2_label,
                id: id2,
                children: children2,
            });
        }
        sort_tree(&mut children1);
        tree.push(TreeNode {
            label: l1_label,
            id: id1,
            children: children1,
        });
    }
    sort_tree(&mut tree);

    let mut subfilters: Vec<String> = groups.into_iter().collect();
    sort_subfilters(&mut subfilters);
    (tree, subfilters)
}

/// Prunes a node by a lowercased search query.
///
/// A node whose label contains the query (case-insensitively) is kept with all
/// its children; otherwise it is kept only if some descendant matches, with
/// only the matching branches. Returns `None` when nothing matches.
pub fn filter_node_by_search(node: &TreeNode, query_lower: &str) -> Option<TreeNode> {
    if node.label.to_lowercase().contains(query_lower) {
        return Some(node.clone());
    }
    let children: Vec<TreeNode> = node
        .children
        .iter()
        .filter_map(|child| filter_node_by_search(child, query_lower))
        .collect();
    (!children.is_empty()).then(|| TreeNode {
        label: node.label.clone(),
        id: node.id.clone(),
        children,
    })
}

/// Applies the subfilter and the search to one top-level node.
///
/// With a subfilter, the node is dropped unless its [`group_char`] equals it.
/// With a non-empty `search_query`, the node is pruned with
/// [`filter_node_by_search`] on the lowercased query.
pub fn filter_root(
    node: &TreeNode,
    subfilter: Option<&str>,
    search_query: &str,
) -> Option<TreeNode> {
    if let Some(sub) = subfilter
        && group_char(&node.label) != sub
    {
        return None;
    }
    if search_query.is_empty() {
        Some(node.clone())
    } else {
        filter_node_by_search(node, &search_query.to_lowercase())
    }
}

/// The visible top-level nodes of a full tree for a subfilter and a search
/// (see [`filter_root`]), in tree order.
pub fn apply_view(
    full_tree: &[TreeNode],
    subfilter: Option<&str>,
    search_query: &str,
) -> Vec<TreeNode> {
    full_tree
        .iter()
        .filter_map(|node| filter_root(node, subfilter, search_query))
        .collect()
}

/// Flattens a tree depth-first into display rows, descending only into
/// expanded nodes (by id).
pub fn flatten_tree(nodes: &[TreeNode], expanded: &HashSet<String>) -> Vec<VisibleTreeItem> {
    fn walk(
        node: &TreeNode,
        depth: usize,
        expanded: &HashSet<String>,
        out: &mut Vec<VisibleTreeItem>,
    ) {
        let is_expanded = expanded.contains(&node.id);
        let has_children = !node.children.is_empty();
        out.push(VisibleTreeItem {
            label: node.label.clone(),
            id: node.id.clone(),
            depth,
            has_children,
            is_expanded,
        });
        if is_expanded && has_children {
            for child in &node.children {
                walk(child, depth + 1, expanded, out);
            }
        }
    }

    let mut out = Vec::new();
    for node in nodes {
        walk(node, 0, expanded, &mut out);
    }
    out
}

/// Expands a collapsed node or collapses an expanded one.
pub fn toggle_node(expanded: &mut HashSet<String>, id: &str) {
    if !expanded.remove(id) {
        expanded.insert(id.to_string());
    }
}

/// The subfilter after a click on `clicked`: clicking the selected group (or
/// "all" while nothing is selected) clears it, any other click selects it.
pub fn next_subfilter(current: Option<&str>, clicked: Option<&str>) -> Option<String> {
    if clicked == current {
        None
    } else {
        clicked.map(str::to_string)
    }
}

/// The rows of the flattened tree to render: `(start, end, top_space, bottom_space)`.
///
/// Rows are [`TREE_ROW_HEIGHT`] px. The window starts at the first row under
/// `scroll_y` and covers the viewport, extended by a margin of half the visible
/// row count (at least 5 rows) on each side; the spaces stand for the rows
/// outside it.
pub fn tree_window(total_items: usize, scroll_y: f32, viewport_h: f32) -> (usize, usize, f32, f32) {
    let first_visible = (scroll_y / TREE_ROW_HEIGHT).floor() as usize;
    let rows_in_view = (viewport_h / TREE_ROW_HEIGHT).ceil() as usize;
    let margin = (rows_in_view / 2).max(5);
    let start = first_visible.saturating_sub(margin);
    let end = (start + rows_in_view + margin * 2).min(total_items);
    let start = start.min(total_items);
    let top_space = start as f32 * TREE_ROW_HEIGHT;
    let bottom_space = total_items.saturating_sub(end) as f32 * TREE_ROW_HEIGHT;
    (start, end, top_space, bottom_space)
}

/// The library filters a selected tree node stands for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeFilters {
    /// Artist filter.
    pub artist: Option<String>,
    /// Album filter.
    pub album: Option<String>,
    /// Genre filter.
    pub genre: Option<String>,
    /// Year filter.
    pub year: Option<String>,
}

/// The library filters for the node `node_id` of a `filter_type` tree.
///
/// The id is split into its parts with [`id_codec::split_parts`]; parts 1, 2
/// and 3 are the node's level labels.
/// - Artist: level 1 = artist, level 2 = album.
/// - Album: level 1 = album, level 2 = artist.
/// - Genre / Year: level 1 = genre / year, level 2 = artist, level 3 = album.
/// - Folder: no filter at all. The Iced UI behaves the same: its
///   `SelectTreeNode` handler leaves folder nodes to a `SelectFolder` message
///   that nothing emits, so a folder node selects but filters nothing.
pub fn filters_for_node(filter_type: FilterType, node_id: &str) -> NodeFilters {
    let mut parts = id_codec::split_parts(node_id).into_iter().skip(1);
    let (l1, l2, l3) = (parts.next(), parts.next(), parts.next());
    match filter_type {
        FilterType::Artist => NodeFilters {
            artist: l1,
            album: l2,
            ..NodeFilters::default()
        },
        FilterType::Album => NodeFilters {
            album: l1,
            artist: l2,
            ..NodeFilters::default()
        },
        FilterType::Genre => NodeFilters {
            genre: l1,
            artist: l2,
            album: l3,
            ..NodeFilters::default()
        },
        FilterType::Year => NodeFilters {
            year: l1,
            artist: l2,
            album: l3,
            ..NodeFilters::default()
        },
        FilterType::Folder => NodeFilters::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(l1: &str, l2: &str, l3: Option<&str>) -> HierarchyRow {
        (Arc::from(l1), Arc::from(l2), l3.map(Arc::from))
    }

    fn labels(nodes: &[TreeNode]) -> Vec<&str> {
        nodes.iter().map(|n| n.label.as_str()).collect()
    }

    fn genre_rows() -> Vec<HierarchyRow> {
        vec![
            row("Rock", "Queen", Some("Innuendo")),
            row("Rock", "Queen", Some("Jazz")),
            row("Rock", "ABBA", Some("Arrival")),
            row("80s Pop", "a-ha", Some("Hunting High and Low")),
            row("¡Latino!", "Shakira", Some("Laundry Service")),
            row("blues", "B.B. King", Some("Live")),
            row("Rock", "Queen", Some("Innuendo")),
        ]
    }

    #[test]
    fn the_type_name_matches_its_debug_output() {
        for ft in FilterType::ALL {
            assert_eq!(ft.name(), format!("{ft:?}"));
        }
    }

    #[test]
    fn group_chars_are_hash_for_digits_bullet_for_symbols_and_the_upper_letter() {
        assert_eq!(group_char("80s"), "#");
        assert_eq!(group_char("¡Hola"), "•");
        assert_eq!(group_char(""), "•");
        assert_eq!(group_char("rock"), "R");
        assert_eq!(group_char("Émile"), "É");
        assert_eq!(group_char("ßand"), "S");
    }

    #[test]
    fn subfilters_put_digits_then_symbols_then_letters() {
        let mut groups: Vec<String> = ["R", "•", "B", "#", "A"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        sort_subfilters(&mut groups);
        assert_eq!(groups, vec!["#", "•", "A", "B", "R"]);
    }

    #[test]
    fn the_tree_has_three_sorted_levels_with_path_ids() {
        let (tree, subfilters) = build_tree(FilterType::Genre, genre_rows());
        assert_eq!(labels(&tree), vec!["80s Pop", "¡Latino!", "blues", "Rock"]);
        assert_eq!(subfilters, vec!["#", "•", "B", "R"]);

        let rock = &tree[3];
        assert_eq!(rock.id, "Genre|Rock");
        assert_eq!(labels(&rock.children), vec!["ABBA", "Queen"]);
        let queen = &rock.children[1];
        assert_eq!(queen.id, "Genre|Rock|Queen");
        assert_eq!(labels(&queen.children), vec!["Innuendo", "Jazz"]);
        assert_eq!(queen.children[0].id, "Genre|Rock|Queen|Innuendo");
        assert!(queen.children[0].children.is_empty());
    }

    #[test]
    fn two_level_hierarchies_have_no_third_level() {
        let (tree, _) = build_tree(
            FilterType::Artist,
            vec![row("Queen", "Jazz", None), row("Queen", "Innuendo", None)],
        );
        assert_eq!(tree[0].id, "Artist|Queen");
        assert_eq!(labels(&tree[0].children), vec!["Innuendo", "Jazz"]);
        assert!(tree[0].children.iter().all(|c| c.children.is_empty()));
    }

    #[test]
    fn the_index_keeps_one_tree_per_type() {
        let mut index = FilterIndex::new();
        assert!(index.tree(FilterType::Genre).is_none());
        index.insert(FilterType::Genre, genre_rows());
        index.insert(
            FilterType::Year,
            vec![row("1991", "Queen", Some("Innuendo"))],
        );
        assert_eq!(
            index.tree(FilterType::Genre).map(<[TreeNode]>::len),
            Some(4)
        );
        assert_eq!(
            index.subfilters(FilterType::Year),
            Some(&["#".to_string()][..])
        );
        assert!(index.subfilters(FilterType::Folder).is_none());
    }

    #[test]
    fn a_subfilter_keeps_only_top_level_nodes_of_that_group() {
        let (tree, _) = build_tree(FilterType::Genre, genre_rows());
        assert_eq!(labels(&apply_view(&tree, Some("R"), "")), vec!["Rock"]);
        assert_eq!(labels(&apply_view(&tree, Some("#"), "")), vec!["80s Pop"]);
        assert!(apply_view(&tree, Some("Z"), "").is_empty());
        assert_eq!(apply_view(&tree, None, "").len(), 4);
    }

    #[test]
    fn a_search_match_keeps_all_children_of_the_matching_node() {
        let (tree, _) = build_tree(FilterType::Genre, genre_rows());
        let view = apply_view(&tree, None, "QUEEN");
        assert_eq!(labels(&view), vec!["Rock"]);
        assert_eq!(labels(&view[0].children), vec!["Queen"]);
        assert_eq!(
            labels(&view[0].children[0].children),
            vec!["Innuendo", "Jazz"]
        );

        let view = apply_view(&tree, None, "rock");
        assert_eq!(labels(&view[0].children), vec!["ABBA", "Queen"]);
    }

    #[test]
    fn a_search_keeps_only_the_matching_descendants_of_a_non_matching_node() {
        let (tree, _) = build_tree(FilterType::Genre, genre_rows());
        let view = apply_view(&tree, None, "jaz");
        assert_eq!(labels(&view), vec!["Rock"]);
        assert_eq!(labels(&view[0].children), vec!["Queen"]);
        assert_eq!(labels(&view[0].children[0].children), vec!["Jazz"]);
        assert!(apply_view(&tree, None, "nothing like this").is_empty());
    }

    #[test]
    fn the_subfilter_and_the_search_combine() {
        let (tree, _) = build_tree(FilterType::Genre, genre_rows());
        assert!(apply_view(&tree, Some("B"), "queen").is_empty());
        assert_eq!(labels(&apply_view(&tree, Some("B"), "king")), vec!["blues"]);
    }

    #[test]
    fn flattening_descends_only_into_expanded_nodes() {
        let (tree, _) = build_tree(FilterType::Genre, genre_rows());
        let mut expanded = HashSet::new();
        assert_eq!(flatten_tree(&tree, &expanded).len(), 4);

        toggle_node(&mut expanded, "Genre|Rock");
        toggle_node(&mut expanded, "Genre|Rock|Queen");
        toggle_node(&mut expanded, "Genre|Rock|Queen|Innuendo");
        let rows = flatten_tree(&tree, &expanded);
        let summary: Vec<(&str, usize, bool, bool)> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.depth, r.has_children, r.is_expanded))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("80s Pop", 0, true, false),
                ("¡Latino!", 0, true, false),
                ("blues", 0, true, false),
                ("Rock", 0, true, true),
                ("ABBA", 1, true, false),
                ("Queen", 1, true, true),
                ("Innuendo", 2, false, true),
                ("Jazz", 2, false, false),
            ]
        );

        toggle_node(&mut expanded, "Genre|Rock");
        assert_eq!(flatten_tree(&tree, &expanded).len(), 4);
        assert!(!expanded.contains("Genre|Rock"));
    }

    #[test]
    fn clicking_the_selected_subfilter_clears_it() {
        assert_eq!(next_subfilter(None, Some("A")), Some("A".to_string()));
        assert_eq!(next_subfilter(Some("A"), Some("A")), None);
        assert_eq!(next_subfilter(Some("A"), Some("B")), Some("B".to_string()));
        assert_eq!(next_subfilter(Some("A"), None), None);
        assert_eq!(next_subfilter(None, None), None);
    }

    #[test]
    fn the_tree_window_covers_the_viewport_plus_a_margin() {
        // 800 px viewport: 25 rows in view, margin 12.
        assert_eq!(tree_window(1000, 0.0, 800.0), (0, 49, 0.0, 951.0 * 32.0));
        assert_eq!(
            tree_window(1000, 3200.0, 800.0),
            (88, 137, 88.0 * 32.0, 863.0 * 32.0)
        );
        // Small viewports keep at least 5 rows of margin.
        assert_eq!(tree_window(100, 320.0, 64.0), (5, 17, 160.0, 83.0 * 32.0));
        // Short lists are rendered entirely.
        assert_eq!(tree_window(3, 0.0, 800.0), (0, 3, 0.0, 0.0));
        assert_eq!(tree_window(0, 500.0, 800.0), (0, 0, 0.0, 0.0));
    }

    #[test]
    fn artist_and_album_nodes_map_to_their_two_levels() {
        assert_eq!(
            filters_for_node(FilterType::Artist, "Artist|Queen|Jazz"),
            NodeFilters {
                artist: Some("Queen".into()),
                album: Some("Jazz".into()),
                ..NodeFilters::default()
            }
        );
        assert_eq!(
            filters_for_node(FilterType::Artist, "Artist|Queen"),
            NodeFilters {
                artist: Some("Queen".into()),
                ..NodeFilters::default()
            }
        );
        assert_eq!(
            filters_for_node(FilterType::Album, "Album|Jazz|Queen"),
            NodeFilters {
                album: Some("Jazz".into()),
                artist: Some("Queen".into()),
                ..NodeFilters::default()
            }
        );
    }

    #[test]
    fn genre_and_year_nodes_map_to_three_levels() {
        assert_eq!(
            filters_for_node(FilterType::Genre, "Genre|Rock|Queen|Innuendo"),
            NodeFilters {
                genre: Some("Rock".into()),
                artist: Some("Queen".into()),
                album: Some("Innuendo".into()),
                year: None,
            }
        );
        assert_eq!(
            filters_for_node(FilterType::Year, "Year|1991|Queen"),
            NodeFilters {
                year: Some("1991".into()),
                artist: Some("Queen".into()),
                ..NodeFilters::default()
            }
        );
    }

    #[test]
    fn folder_nodes_apply_no_filter() {
        assert_eq!(
            filters_for_node(FilterType::Folder, "Folder|Music|Queen|Jazz"),
            NodeFilters::default()
        );
    }

    #[test]
    fn d4_a_label_containing_a_pipe_selects_the_right_filters() {
        // Before the fix the id was `Artist|AC|DC|Back in Black`, which
        // selected artist "AC" and album "DC".
        let (tree, _) = build_tree(
            FilterType::Artist,
            vec![row("AC|DC", "Back in Black", None), row("AC", "DC", None)],
        );
        assert_eq!(labels(&tree), vec!["AC", "AC|DC"]);
        let acdc = &tree[1];
        assert_eq!(acdc.id, r"Artist|AC\|DC");
        let album = &acdc.children[0];
        assert_eq!(album.id, r"Artist|AC\|DC|Back in Black");
        assert_eq!(
            filters_for_node(FilterType::Artist, &album.id),
            NodeFilters {
                artist: Some("AC|DC".into()),
                album: Some("Back in Black".into()),
                ..NodeFilters::default()
            }
        );
        // The plain `AC` → `DC` node is a different one.
        assert_eq!(
            filters_for_node(FilterType::Artist, &tree[0].children[0].id),
            NodeFilters {
                artist: Some("AC".into()),
                album: Some("DC".into()),
                ..NodeFilters::default()
            }
        );
    }

    #[test]
    fn d4_three_level_labels_with_pipes_and_backslashes_round_trip() {
        let (tree, _) = build_tree(
            FilterType::Genre,
            vec![row(r"Rock|Pop", r"A\B|C", Some(r"Live | \ Studio"))],
        );
        let leaf = &tree[0].children[0].children[0];
        assert_eq!(
            filters_for_node(FilterType::Genre, &leaf.id),
            NodeFilters {
                genre: Some("Rock|Pop".into()),
                artist: Some(r"A\B|C".into()),
                album: Some(r"Live | \ Studio".into()),
                year: None,
            }
        );
        let (tree, _) = build_tree(FilterType::Album, vec![row("x|y", r"z\", None)]);
        assert_eq!(
            filters_for_node(FilterType::Album, &tree[0].children[0].id),
            NodeFilters {
                album: Some("x|y".into()),
                artist: Some(r"z\".into()),
                ..NodeFilters::default()
            }
        );
    }

    #[test]
    fn d4_node_ids_without_pipes_or_backslashes_are_unchanged() {
        let (tree, _) = build_tree(FilterType::Genre, genre_rows());
        for l1 in &tree {
            assert_eq!(l1.id, format!("Genre|{}", l1.label));
            for l2 in &l1.children {
                assert_eq!(l2.id, format!("Genre|{}|{}", l1.label, l2.label));
                for l3 in &l2.children {
                    assert_eq!(
                        l3.id,
                        format!("Genre|{}|{}|{}", l1.label, l2.label, l3.label)
                    );
                }
            }
        }
    }
}
