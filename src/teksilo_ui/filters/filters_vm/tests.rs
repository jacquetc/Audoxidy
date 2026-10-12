use std::time::Duration;

use teksilo::data::TreeDataSource;

use super::*;
use crate::db::database::LibrarySearchParams;
use crate::teksilo_ui::app::services::testing::in_memory_services;
use crate::teksilo_ui::test_support::run_until;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Services over the mock library and a view-model showing `filter_type`
/// with its tree loaded.
fn loaded(filter_type: FilterType) -> (AppServices, FiltersViewModel) {
    let services = in_memory_services();
    let vm = FiltersViewModel::new(&services);
    if filter_type != FilterType::Genre {
        vm.set_filter_type(filter_type);
    } else {
        services.filter_hierarchy.set_filter_type(filter_type);
    }
    let hierarchy = services.filter_hierarchy.clone();
    assert!(
        run_until(&services.runtime, TIMEOUT, || !hierarchy
            .tree_of(filter_type)
            .is_empty()),
        "the {filter_type:?} tree did not load"
    );
    vm.refresh();
    (services, vm)
}

/// The library's search parameters as a comparable tuple (the type itself is
/// not `PartialEq`).
type Fields = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

fn fields(p: LibrarySearchParams) -> Fields {
    (p.query, p.artist, p.album, p.genre, p.year, p.folder_id)
}

fn params(services: &AppServices) -> LibrarySearchParams {
    services.library_songs.params()
}

/// The id of the first node of the shown tree whose label is `label`.
fn id_of(vm: &FiltersViewModel, label: &str) -> String {
    vm.tree_source()
        .rows()
        .into_iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| panic!("{label} is not a visible row"))
        .id
}

/// Expands every node of the shown tree.
fn expand_all(vm: &FiltersViewModel) {
    loop {
        let collapsed: Vec<_> = vm
            .visible_items()
            .into_iter()
            .filter(|row| row.has_children && !row.is_expanded)
            .collect();
        if collapsed.is_empty() {
            return;
        }
        for row in collapsed {
            vm.toggle_node(&row.id);
        }
    }
}

/// The first row of `depth` below the row `parent`.
fn child_of(vm: &FiltersViewModel, parent: &str) -> String {
    let rows = vm.visible_items();
    let at = rows.iter().position(|row| row.id == parent).unwrap();
    rows[at + 1].id.clone()
}

#[test]
fn the_column_starts_on_genres_with_nothing_chosen() {
    let services = in_memory_services();
    let vm = FiltersViewModel::new(&services);
    assert_eq!(vm.filter_type().get(), FilterType::Genre);
    assert_eq!(vm.subfilter().get(), None);
    assert_eq!(vm.search().get(), "");
    assert_eq!(vm.selected_node().get(), None);
    assert!(vm.visible_items().is_empty(), "the index is not built yet");
    assert!(vm.subfilters().get().is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_genre_node_filters_the_library_by_genre() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.select_node(&rock);
    assert_eq!(vm.selected_node().get(), Some(rock.clone()));
    assert_eq!(
        fields(params(&services)),
        fields(LibrarySearchParams {
            genre: Some("Rock".to_string()),
            ..LibrarySearchParams::default()
        })
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn the_deeper_nodes_of_a_genre_add_the_artist_and_the_album() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.toggle_node(&rock);
    let artist = child_of(&vm, &rock);
    vm.select_node(&artist);
    let p = params(&services);
    assert_eq!(p.genre.as_deref(), Some("Rock"));
    assert!(p.artist.is_some());
    assert_eq!(p.album, None);
    vm.toggle_node(&artist);
    let album = child_of(&vm, &artist);
    vm.select_node(&album);
    let p = params(&services);
    assert_eq!(p.genre.as_deref(), Some("Rock"));
    assert!(p.artist.is_some() && p.album.is_some());
    assert_eq!(p.year, None);
    assert!(services.ctx.shutdown());
}

#[test]
fn an_artist_node_filters_by_artist_and_its_album_node_by_both() {
    let (services, vm) = loaded(FilterType::Artist);
    let bowie = id_of(&vm, "David Bowie");
    vm.select_node(&bowie);
    assert_eq!(
        fields(params(&services)),
        fields(LibrarySearchParams {
            artist: Some("David Bowie".to_string()),
            ..LibrarySearchParams::default()
        })
    );
    vm.toggle_node(&bowie);
    let album = child_of(&vm, &bowie);
    vm.select_node(&album);
    let p = params(&services);
    assert_eq!(p.artist.as_deref(), Some("David Bowie"));
    assert!(p.album.is_some());
    assert_eq!((p.genre, p.year, p.folder_id), (None, None, None));
    assert!(services.ctx.shutdown());
}

#[test]
fn an_album_node_filters_by_album_and_its_artist_node_by_both() {
    let (services, vm) = loaded(FilterType::Album);
    let first = vm.visible_items()[0].clone();
    vm.select_node(&first.id);
    let p = params(&services);
    assert_eq!(p.album.as_deref(), Some(first.label.as_str()));
    assert_eq!(p.artist, None);
    vm.toggle_node(&first.id);
    let artist = child_of(&vm, &first.id);
    vm.select_node(&artist);
    let p = params(&services);
    assert_eq!(p.album.as_deref(), Some(first.label.as_str()));
    assert!(p.artist.is_some());
    assert!(services.ctx.shutdown());
}

#[test]
fn a_year_node_filters_by_year() {
    let (services, vm) = loaded(FilterType::Year);
    let year = vm.visible_items()[0].clone();
    vm.select_node(&year.id);
    assert_eq!(
        fields(params(&services)),
        fields(LibrarySearchParams {
            year: Some(year.label),
            ..LibrarySearchParams::default()
        })
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_folder_node_is_selected_but_applies_no_filter() {
    let (services, vm) = loaded(FilterType::Folder);
    // A filter from an earlier choice, which a folder node does not touch
    // because it sets all of them to nothing, as Iced's handler does.
    services
        .library_songs
        .update_params(|p| p.genre = Some("Rock".to_string()));
    let folder = vm.visible_items()[0].clone();
    expand_all(&vm);
    for row in vm.visible_items() {
        vm.select_node(&row.id);
        assert_eq!(vm.selected_node().get(), Some(row.id.clone()));
        assert_eq!(
            fields(params(&services)),
            fields(LibrarySearchParams::default()),
            "{}",
            row.id
        );
    }
    assert_eq!(vm.visible_items()[0].id, folder.id);
    assert!(services.ctx.shutdown());
}

#[test]
fn selecting_a_node_keeps_the_librarys_search_text() {
    let (services, vm) = loaded(FilterType::Genre);
    services.library_songs.set_query("space");
    vm.select_node(&id_of(&vm, "Rock"));
    let p = params(&services);
    assert_eq!(p.query.as_deref(), Some("space"));
    assert_eq!(p.genre.as_deref(), Some("Rock"));
    assert!(services.ctx.shutdown());
}

#[test]
fn selecting_a_node_moves_the_trees_highlight_onto_it() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.select_node(&rock);
    assert_eq!(vm.selection().selected_keys(), vec![rock]);
    assert!(services.ctx.shutdown());
}

#[test]
fn choosing_a_filter_type_resets_the_column_and_the_librarys_filters() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.toggle_node(&rock);
    vm.select_node(&rock);
    vm.select_subfilter(Some("R"));
    services
        .library_songs
        .update_params(|p| p.genre = Some("Rock".to_string()));
    vm.set_filter_type(FilterType::Year);
    assert_eq!(vm.filter_type().get(), FilterType::Year);
    assert_eq!(vm.subfilter().get(), None);
    assert_eq!(vm.selected_node().get(), None);
    assert!(vm.selection().selected_keys().is_empty());
    assert_eq!(
        fields(params(&services)),
        fields(LibrarySearchParams::default())
    );
    assert!(!vm.tree_source().is_expanded(&rock));
    assert!(services.ctx.shutdown());
}

#[test]
fn choosing_the_same_type_again_resets_it_too() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.toggle_node(&rock);
    vm.select_node(&rock);
    vm.set_filter_type(FilterType::Genre);
    assert_eq!(vm.selected_node().get(), None);
    assert_eq!(
        fields(params(&services)),
        fields(LibrarySearchParams::default())
    );
    assert!(!vm.tree_source().is_expanded(&rock));
    assert!(services.ctx.shutdown());
}

#[test]
fn a_letter_prunes_the_tree_to_its_group() {
    let (services, vm) = loaded(FilterType::Genre);
    let all = vm.visible_items().len();
    assert!(vm.subfilters().get().contains(&"R".to_string()));
    vm.select_subfilter(Some("R"));
    assert_eq!(vm.subfilter().get(), Some("R".to_string()));
    let labels: Vec<_> = vm.visible_items().into_iter().map(|r| r.label).collect();
    assert!(!labels.is_empty() && labels.len() < all);
    assert!(
        labels.iter().all(|l| filter_index::group_char(l) == "R"),
        "{labels:?}"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn the_selected_letter_clears_on_a_second_click_and_so_does_the_restore_icon() {
    let (services, vm) = loaded(FilterType::Genre);
    let all = vm.visible_items().len();
    vm.select_subfilter(Some("R"));
    vm.select_subfilter(Some("R"));
    assert_eq!(vm.subfilter().get(), None);
    assert_eq!(vm.visible_items().len(), all);
    vm.select_subfilter(Some("R"));
    vm.select_subfilter(None);
    assert_eq!(vm.subfilter().get(), None);
    // With nothing selected, the restore icon is a no-op.
    vm.select_subfilter(None);
    assert_eq!(vm.subfilter().get(), None);
    assert_eq!(vm.visible_items().len(), all);
    assert!(services.ctx.shutdown());
}

#[test]
fn a_letter_drops_the_selected_node_but_keeps_the_librarys_filters() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.select_node(&rock);
    vm.select_subfilter(Some("P"));
    assert_eq!(vm.selected_node().get(), None);
    assert!(vm.selection().selected_keys().is_empty());
    assert_eq!(
        params(&services).genre.as_deref(),
        Some("Rock"),
        "Iced leaves the library's filters when a letter is chosen"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn clearing_the_highlight_drops_the_letter_and_the_node_and_leaves_the_library() {
    // What a reveal does to the column after clearing the library itself
    // (`app.rs:6108-6110`).
    let (services, vm) = loaded(FilterType::Genre);
    let all = vm.visible_items().len();
    vm.select_subfilter(Some("R"));
    vm.select_node(&id_of(&vm, "Rock"));
    assert!(vm.visible_items().len() < all);
    vm.clear_highlight();
    assert_eq!(vm.subfilter().get(), None);
    assert_eq!(vm.selected_node().get(), None);
    assert!(vm.selection().selected_keys().is_empty());
    assert_eq!(vm.visible_items().len(), all, "the whole tree is back");
    assert_eq!(
        params(&services).genre.as_deref(),
        Some("Rock"),
        "the library's filters are the library's to clear"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn clearing_the_letter_clears_the_librarys_filters() {
    let (services, vm) = loaded(FilterType::Genre);
    vm.select_node(&id_of(&vm, "Rock"));
    vm.select_subfilter(Some("P"));
    vm.select_subfilter(Some("P"));
    assert_eq!(
        fields(params(&services)),
        fields(LibrarySearchParams::default())
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn the_search_prunes_the_tree_without_touching_the_library() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.select_node(&rock);
    let before = params(&services);
    let all = vm.visible_items().len();
    vm.set_search("ROCK");
    let labels: Vec<_> = vm.visible_items().into_iter().map(|r| r.label).collect();
    assert!(!labels.is_empty() && labels.len() < all, "{labels:?}");
    assert!(labels.iter().any(|l| l == "Rock"));
    assert_eq!(fields(params(&services)), fields(before));
    assert_eq!(vm.selected_node().get(), Some(rock));
    vm.set_search("zzzz-no-such-genre");
    assert!(vm.visible_items().is_empty());
    vm.set_search("");
    assert_eq!(vm.visible_items().len(), all);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_search_keeps_the_ancestors_of_a_matching_descendant() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.toggle_node(&rock);
    let artist = vm
        .visible_items()
        .into_iter()
        .find(|row| row.depth == 1)
        .unwrap()
        .label;
    vm.set_search(&artist);
    let rows = vm.visible_items();
    assert!(rows.iter().any(|r| r.id == rock), "the genre stays");
    assert!(rows.iter().any(|r| r.label == artist && r.depth == 1));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_search_and_a_letter_apply_together() {
    let (services, vm) = loaded(FilterType::Genre);
    vm.select_subfilter(Some("R"));
    vm.set_search("rock");
    let labels: Vec<_> = vm.visible_items().into_iter().map(|r| r.label).collect();
    assert!(labels.iter().all(|l| l.to_lowercase().contains("rock")));
    assert!(labels.iter().all(|l| filter_index::group_char(l) == "R"));
    assert!(!labels.is_empty());
    assert!(services.ctx.shutdown());
}

#[test]
fn toggling_a_node_shows_and_hides_its_children() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    let collapsed = vm.visible_items().len();
    vm.toggle_node(&rock);
    let rows = vm.visible_items();
    assert!(rows.len() > collapsed);
    let at = rows.iter().position(|r| r.id == rock).unwrap();
    assert!(rows[at].is_expanded);
    assert_eq!(rows[at + 1].depth, 1);
    vm.toggle_node(&rock);
    assert_eq!(vm.visible_items().len(), collapsed);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_expansion_survives_a_letter_and_a_search() {
    let (services, vm) = loaded(FilterType::Genre);
    let rock = id_of(&vm, "Rock");
    vm.toggle_node(&rock);
    vm.select_subfilter(Some("R"));
    vm.set_search("rock");
    assert!(vm.tree_source().is_expanded(&rock));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_tree_source_answers_for_the_keyboard_navigation() {
    use teksilo::data::TreeDataSource;
    let (services, vm) = loaded(FilterType::Genre);
    let source = vm.tree_source();
    let rock = id_of(&vm, "Rock");
    source.set_expanded(&rock, true);
    let artist = child_of(&vm, &rock);
    assert_eq!(source.parent(&artist), Some(rock.clone()));
    assert_eq!(source.parent(&rock), None);
    assert!(source.child_keys(&rock).contains(&artist));
    assert_eq!(
        source.flat_index_of(&artist),
        Some(source.flat_index_of(&rock).unwrap() + 1)
    );
    assert_eq!(
        source.key_at(source.flat_index_of(&rock).unwrap()),
        Some(rock.clone())
    );
    // A node hidden under a collapsed ancestor still exists.
    source.set_expanded(&rock, false);
    assert!(source.contains_key(&artist));
    assert_eq!(source.flat_index_of(&artist), None);
    assert!(services.ctx.shutdown());
}

#[test]
fn the_letters_follow_the_shown_type() {
    let (services, vm) = loaded(FilterType::Genre);
    let genres = vm.subfilters().get();
    assert!(!genres.is_empty());
    vm.set_filter_type(FilterType::Year);
    let hierarchy = services.filter_hierarchy.clone();
    assert!(run_until(&services.runtime, TIMEOUT, || !hierarchy
        .tree_of(FilterType::Year)
        .is_empty()));
    vm.refresh();
    let years = vm.subfilters().get();
    assert_ne!(years, genres);
    assert_eq!(years, services.filter_hierarchy.subfilters());
    assert!(services.ctx.shutdown());
}
