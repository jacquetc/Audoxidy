use super::*;

// ------------------------------------------------------------
// Fixtures
// ------------------------------------------------------------

fn song(song_id: i64, enabled: bool) -> PlaylistSongRef {
    PlaylistSongRef {
        item_id: song_id,
        song_id,
        sequence_order: song_id as f64,
        enabled,
        title: format!("Title {song_id}").into(),
        artist_name: "Artist".into(),
        album_title: "Album".into(),
        album_artist_name: "Artist".into(),
        year: None,
        genre: None,
        track_number: None,
        duration: 100.0 + song_id as f64,
        file_path: format!("/m/{song_id}.flac").into(),
        folder_path: "/m".into(),
        folder_name: "m".into(),
        cover_path: None,
        cover_override: None,
    }
}

fn group(anchor: i64, songs: Vec<PlaylistSongRef>) -> PlaylistFolderGroup {
    PlaylistFolderGroup {
        folder_path: format!("/f{anchor}").into(),
        folder_name: format!("f{anchor}").into(),
        total_duration: songs.iter().map(|s| s.duration).sum(),
        songs,
        first_item_id: anchor,
    }
}

/// Group A (anchor 10): songs 1, 2. Group B (anchor 20): songs 3, 4, 5.
/// Linear: 0 sep A, 1 s1, 2 s2, 3 sep B, 4 s3, 5 s4, 6 s5.
fn two_groups() -> Vec<PlaylistFolderGroup> {
    vec![
        group(10, vec![song(1, true), song(2, true)]),
        group(20, vec![song(3, true), song(4, true), song(5, true)]),
    ]
}

fn one_group(songs: Vec<PlaylistSongRef>) -> Vec<PlaylistFolderGroup> {
    vec![group(1, songs)]
}

fn collapsed(anchors: &[i64]) -> HashSet<i64> {
    anchors.iter().copied().collect()
}

fn set(idxs: &[usize]) -> HashSet<usize> {
    idxs.iter().copied().collect()
}

fn session(order: &[i64], position: usize, history: &[i64]) -> ShuffleSession {
    ShuffleSession {
        session_id: "s1".into(),
        shuffle_order: order.to_vec(),
        current_position: position,
        history: history.to_vec(),
    }
}

const PLAIN: SelectModifiers = SelectModifiers {
    toggle: false,
    range: false,
};
const TOGGLE: SelectModifiers = SelectModifiers {
    toggle: true,
    range: false,
};
const RANGE: SelectModifiers = SelectModifiers {
    toggle: false,
    range: true,
};

// ------------------------------------------------------------
// Linear index
// ------------------------------------------------------------

#[test]
fn each_group_contributes_a_separator_and_its_songs() {
    let g = two_groups();
    assert_eq!(count_linear_items(&g), 7);
    assert_eq!(flatten_groups(&g).len(), 5);
    assert_eq!(item_at(&g, 0), Some(PlaylistItemType::Separator(0, 10)));
    assert_eq!(item_at(&g, 2), Some(PlaylistItemType::Song(0, 1, 10)));
    assert_eq!(item_at(&g, 3), Some(PlaylistItemType::Separator(1, 20)));
    assert_eq!(item_at(&g, 6), Some(PlaylistItemType::Song(1, 2, 20)));
    assert_eq!(item_at(&g, 7), None);
}

#[test]
fn song_lookup_returns_none_on_separators_and_past_the_end() {
    let g = two_groups();
    assert_eq!(song_at(&g, 1).map(|s| s.song_id), Some(1));
    assert_eq!(song_at(&g, 5).map(|s| s.song_id), Some(4));
    assert!(song_at(&g, 0).is_none());
    assert!(song_at(&g, 3).is_none());
    assert!(song_at(&g, 7).is_none());
}

#[test]
fn mutable_song_lookup_matches_the_shared_one() {
    let mut g = two_groups();
    if let Some(s) = song_at_mut(&mut g, 4) {
        s.enabled = false;
    }
    assert!(!g[1].songs[0].enabled);
    assert!(song_at_mut(&mut g, 3).is_none());
}

#[test]
fn group_info_tells_separators_from_songs() {
    let g = two_groups();
    assert_eq!(group_info_at(&g, 0), Some((0, true)));
    assert_eq!(group_info_at(&g, 2), Some((0, false)));
    assert_eq!(group_info_at(&g, 3), Some((1, true)));
    assert_eq!(group_info_at(&g, 9), None);
}

#[test]
fn a_separator_is_found_by_its_anchor() {
    let g = two_groups();
    assert_eq!(separator_index_for_anchor(&g, 10), Some(0));
    assert_eq!(separator_index_for_anchor(&g, 20), Some(3));
    assert_eq!(separator_index_for_anchor(&g, 99), None);
}

#[test]
fn a_song_id_maps_to_its_first_linear_index() {
    let mut g = two_groups();
    assert_eq!(linear_index_of_song_id(&g, 1), Some(1));
    assert_eq!(linear_index_of_song_id(&g, 5), Some(6));
    assert_eq!(linear_index_of_song_id(&g, 42), None);
    g[1].songs.push(song(1, true));
    assert_eq!(linear_index_of_song_id(&g, 1), Some(1));
}

#[test]
fn the_first_enabled_song_of_a_group_is_found_from_its_separator() {
    let mut g = two_groups();
    g[1].songs[0].enabled = false;
    assert_eq!(first_enabled_song_in_group(&g, 3), Some(5));
    assert_eq!(first_enabled_song_in_group(&g, 0), Some(1));
    // Not a separator.
    assert_eq!(first_enabled_song_in_group(&g, 1), None);
    g[0].songs.iter_mut().for_each(|s| s.enabled = false);
    assert_eq!(first_enabled_song_in_group(&g, 0), None);
}

// ------------------------------------------------------------
// Visible rows
// ------------------------------------------------------------

#[test]
fn every_item_is_visible_when_nothing_is_collapsed() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let linear: Vec<usize> = v.iter().map(|(_, l)| *l).collect();
    assert_eq!(linear, [0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(v[4].0, PlaylistItemType::Song(1, 0, 20));
}

#[test]
fn a_collapsed_group_shows_only_its_separator() {
    let g = two_groups();
    let v = visible_items(&g, &collapsed(&[10]));
    let linear: Vec<usize> = v.iter().map(|(_, l)| *l).collect();
    assert_eq!(linear, [0, 3, 4, 5, 6]);
}

#[test]
fn a_song_in_a_collapsed_group_maps_to_its_separator_row() {
    let g = two_groups();
    let c = collapsed(&[10]);
    assert_eq!(visible_row_of(&g, &c, 0), Some(0));
    assert_eq!(visible_row_of(&g, &c, 1), Some(0));
    assert_eq!(visible_row_of(&g, &c, 2), Some(0));
    assert_eq!(visible_row_of(&g, &c, 3), Some(1));
    assert_eq!(visible_row_of(&g, &c, 6), Some(4));
    assert_eq!(visible_row_of(&g, &c, 7), None);
}

#[test]
fn rows_map_back_to_the_visible_linear_indices() {
    let g = two_groups();
    for c in [
        collapsed(&[]),
        collapsed(&[10]),
        collapsed(&[20]),
        collapsed(&[10, 20]),
    ] {
        let v = visible_items(&g, &c);
        for (row, (_, linear)) in v.iter().enumerate() {
            assert_eq!(linear_index_at_row(&g, &c, row), Some(*linear));
            assert_eq!(visible_row_of(&g, &c, *linear), Some(row));
        }
        assert_eq!(linear_index_at_row(&g, &c, v.len()), None);
    }
}

#[test]
fn y_bounds_follow_the_visible_rows() {
    let g = two_groups();
    let c = collapsed(&[10]);
    for linear in 0..count_linear_items(&g) {
        let row = visible_row_of(&g, &c, linear).unwrap_or(usize::MAX) as f32;
        assert_eq!(
            item_y_bounds(&g, &c, linear),
            (
                row * SONG_ROW_HEIGHT,
                row * SONG_ROW_HEIGHT + SONG_ROW_HEIGHT
            )
        );
    }
}

#[test]
fn y_bounds_of_a_hidden_song_are_its_separator_band() {
    let g = two_groups();
    let c = collapsed(&[20]);
    assert_eq!(item_y_bounds(&g, &c, 5), (138.0, 184.0));
    assert_eq!(item_y_bounds(&g, &HashSet::new(), 5), (230.0, 276.0));
}

#[test]
fn y_bounds_past_the_end_are_zero() {
    let g = two_groups();
    assert_eq!(item_y_bounds(&g, &HashSet::new(), 99), (0.0, 0.0));
    assert_eq!(item_y_bounds(&[], &HashSet::new(), 0), (0.0, 0.0));
}

// ------------------------------------------------------------
// Selection
// ------------------------------------------------------------

#[test]
fn a_plain_click_selects_focuses_and_pivots_on_the_item() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.selected.insert(5);
    s.click(2, PLAIN, &v);
    assert_eq!(s.selected, set(&[2]));
    assert_eq!(s.focused, Some(2));
    assert_eq!(s.pivot, Some(2));
}

#[test]
fn a_toggle_click_flips_the_item_and_moves_focus_and_pivot() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.click(1, PLAIN, &v);
    s.click(4, TOGGLE, &v);
    assert_eq!(s.selected, set(&[1, 4]));
    assert_eq!(s.focused, Some(4));
    assert_eq!(s.pivot, Some(4));
    s.click(1, TOGGLE, &v);
    assert_eq!(s.selected, set(&[4]));
    assert_eq!(s.focused, Some(1));
}

#[test]
fn toggle_wins_over_range_when_both_modifiers_are_held() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.click(1, PLAIN, &v);
    s.click(
        5,
        SelectModifiers {
            toggle: true,
            range: true,
        },
        &v,
    );
    assert_eq!(s.selected, set(&[1, 5]));
}

#[test]
fn a_range_click_selects_from_the_pivot_over_the_visible_order() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.click(5, PLAIN, &v);
    s.click(2, RANGE, &v);
    assert_eq!(s.selected, set(&[2, 3, 4, 5]));
    assert_eq!(s.focused, Some(2));
    assert_eq!(s.pivot, Some(5));
}

#[test]
fn a_range_over_a_collapsed_group_skips_its_hidden_songs() {
    let g = two_groups();
    let v = visible_items(&g, &collapsed(&[10]));
    let mut s = PlaylistSelection::default();
    s.click(0, PLAIN, &v);
    s.click(4, RANGE, &v);
    assert_eq!(s.selected, set(&[0, 3, 4]));
}

#[test]
fn a_range_click_without_pivot_selects_only_the_item() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.click(4, RANGE, &v);
    assert_eq!(s.selected, set(&[4]));
    assert_eq!(s.pivot, Some(4));
}

#[test]
fn a_range_to_a_hidden_item_changes_nothing() {
    let g = two_groups();
    let v = visible_items(&g, &collapsed(&[20]));
    let mut s = PlaylistSelection::default();
    s.click(1, PLAIN, &v);
    s.select_range(1, 5, &v);
    assert_eq!(s.selected, set(&[1]));
    assert_eq!(s.focused, Some(1));
}

#[test]
fn down_moves_to_the_next_visible_row_and_clamps_at_the_end() {
    let g = two_groups();
    let v = visible_items(&g, &collapsed(&[10]));
    let mut s = PlaylistSelection::default();
    s.select_only(0);
    s.navigate(VerticalDir::Down, false, &v);
    assert_eq!(s.focused, Some(3));
    assert_eq!(s.selected, set(&[3]));
    s.select_only(6);
    s.navigate(VerticalDir::Down, false, &v);
    assert_eq!(s.focused, Some(6));
}

#[test]
fn up_moves_to_the_previous_row_and_clamps_at_the_top() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.select_only(2);
    s.navigate(VerticalDir::Up, false, &v);
    assert_eq!(s.focused, Some(1));
    s.select_only(0);
    s.navigate(VerticalDir::Up, false, &v);
    assert_eq!(s.focused, Some(0));
    assert_eq!(s.pivot, Some(0));
}

#[test]
fn navigation_from_a_hidden_or_missing_focus_goes_to_the_first_row() {
    let g = two_groups();
    let v = visible_items(&g, &collapsed(&[20]));
    let mut s = PlaylistSelection::default();
    s.navigate(VerticalDir::Down, false, &v);
    assert_eq!(s.focused, Some(0));
    s.select_only(5); // hidden in the collapsed group
    s.navigate(VerticalDir::Up, false, &v);
    assert_eq!(s.focused, Some(0));
}

#[test]
fn shift_navigation_extends_from_the_pivot() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection::default();
    s.select_only(2);
    s.navigate(VerticalDir::Down, true, &v);
    s.navigate(VerticalDir::Down, true, &v);
    assert_eq!(s.selected, set(&[2, 3, 4]));
    assert_eq!(s.focused, Some(4));
    assert_eq!(s.pivot, Some(2));
    s.navigate(VerticalDir::Up, true, &v);
    assert_eq!(s.selected, set(&[2, 3]));
}

#[test]
fn shift_navigation_without_pivot_anchors_at_the_current_row() {
    let g = two_groups();
    let v = visible_items(&g, &HashSet::new());
    let mut s = PlaylistSelection {
        focused: Some(4),
        ..PlaylistSelection::default()
    };
    s.navigate(VerticalDir::Up, true, &v);
    assert_eq!(s.pivot, Some(4));
    assert_eq!(s.selected, set(&[3, 4]));
    assert_eq!(s.focused, Some(3));
}

#[test]
fn navigation_in_an_empty_list_does_nothing() {
    let mut s = PlaylistSelection::default();
    s.navigate(VerticalDir::Down, false, &[]);
    assert_eq!(s, PlaylistSelection::default());
}

#[test]
fn select_all_takes_the_visible_items_and_drops_the_pivot() {
    let g = two_groups();
    let v = visible_items(&g, &collapsed(&[10]));
    let mut s = PlaylistSelection::default();
    s.select_only(3);
    s.select_all(&v);
    assert_eq!(s.selected, set(&[0, 3, 4, 5, 6]));
    assert_eq!(s.pivot, None);
    assert_eq!(s.focused, Some(3));
}

#[test]
fn clearing_drops_selection_focus_and_pivot() {
    let mut s = PlaylistSelection::default();
    s.select_only(3);
    s.clear();
    assert_eq!(s, PlaylistSelection::default());
}

#[test]
fn selected_song_ids_follow_playlist_order() {
    let g = two_groups();
    assert_eq!(selected_song_ids(&g, &set(&[6, 1, 4])), vec![1, 3, 5]);
}

#[test]
fn a_selected_separator_expands_to_its_songs_without_duplicates() {
    let g = two_groups();
    assert_eq!(selected_song_ids(&g, &set(&[3, 5, 2])), vec![2, 3, 4, 5]);
    assert_eq!(selected_song_ids(&g, &set(&[99])), Vec::<i64>::new());
}

/// [`two_groups`] searched for "keep", which matches songs 2, 4 and 5.
/// Displayed linear: 0 sep A, 1 s2, 2 sep B, 3 s4, 4 s5.
fn searched_two_groups() -> (Vec<PlaylistFolderGroup>, Vec<PlaylistFolderGroup>) {
    let mut g = two_groups();
    for (gi, si) in [(0, 1), (1, 1), (1, 2)] {
        g[gi].songs[si].title = "Keep me".into();
    }
    let filtered = filter_groups(&g, "keep").unwrap_or_default();
    (g, filtered)
}

#[test]
fn d2_an_action_under_a_search_targets_the_visible_song() {
    // Before the fix, send-to looked up the song's position in the filtered
    // group inside the full group: clicking song 2 sent song 1.
    let (_, shown) = searched_two_groups();
    let none = set(&[]);
    assert_eq!(action_target_song_ids(&shown, &none, 1), vec![2]);
    assert_eq!(action_target_song_ids(&shown, &none, 3), vec![4]);
    assert_eq!(action_target_song_ids(&shown, &none, 4), vec![5]);
}

#[test]
fn d2_a_separator_under_a_search_targets_only_its_visible_songs() {
    // Before the fix, deleting separator B removed song 3 too, which the
    // search hides.
    let (_, shown) = searched_two_groups();
    assert_eq!(action_target_song_ids(&shown, &set(&[]), 2), vec![4, 5]);
    assert_eq!(action_target_song_ids(&shown, &set(&[2]), 2), vec![4, 5]);
    assert_eq!(action_target_song_ids(&shown, &set(&[]), 0), vec![2]);
}

#[test]
fn d2_a_clicked_item_in_the_selection_targets_the_whole_visible_selection() {
    let (_, shown) = searched_two_groups();
    let selection = set(&[4, 0]);
    assert_eq!(action_target_song_ids(&shown, &selection, 4), vec![2, 5]);
    // A click outside the selection targets only the clicked item.
    assert_eq!(action_target_song_ids(&shown, &selection, 3), vec![4]);
    // A separator selected with one of its songs counts each song once.
    assert_eq!(action_target_song_ids(&shown, &set(&[2, 4]), 2), vec![4, 5]);
}

#[test]
fn d2_without_a_search_the_targets_are_unchanged() {
    let g = two_groups();
    let none = set(&[]);
    assert_eq!(action_target_song_ids(&g, &none, 1), vec![1]);
    assert_eq!(action_target_song_ids(&g, &none, 5), vec![4]);
    assert_eq!(action_target_song_ids(&g, &none, 3), vec![3, 4, 5]);
    assert_eq!(action_target_song_ids(&g, &none, 99), Vec::<i64>::new());
    let selection = set(&[6, 0]);
    assert_eq!(action_target_song_ids(&g, &selection, 6), vec![1, 2, 5]);
    assert_eq!(action_target_song_ids(&g, &selection, 4), vec![3]);
    assert_eq!(
        action_target_song_ids(&g, &selection, 6),
        selected_song_ids(&g, &selection)
    );
}

// ------------------------------------------------------------
// Collapsing
// ------------------------------------------------------------

#[test]
fn toggling_a_group_collapses_then_expands_it() {
    let mut c = HashSet::new();
    toggle_group_collapsed(&mut c, 10);
    assert!(c.contains(&10));
    toggle_group_collapsed(&mut c, 10);
    assert!(c.is_empty());
}

#[test]
fn toggle_all_collapses_everything_if_any_group_is_expanded() {
    let g = two_groups();
    let mut c = collapsed(&[10]);
    toggle_all_groups(&g, &mut c);
    assert_eq!(c, collapsed(&[10, 20]));
    toggle_all_groups(&g, &mut c);
    assert!(c.is_empty());
}

#[test]
fn left_collapses_every_touched_group_and_moves_focus_to_the_separator() {
    let g = two_groups();
    let mut c = HashSet::new();
    let mut s = PlaylistSelection {
        selected: set(&[2, 5]),
        focused: Some(5),
        pivot: Some(2),
    };
    assert!(collapse_selected_groups(&g, &mut c, &mut s));
    assert_eq!(c, collapsed(&[10, 20]));
    assert_eq!(s.selected, set(&[3]));
    assert_eq!(s.focused, Some(3));
    assert_eq!(s.pivot, Some(3));
}

#[test]
fn left_on_a_separator_keeps_the_selection() {
    let g = two_groups();
    let mut c = HashSet::new();
    let mut s = PlaylistSelection {
        selected: set(&[0, 4]),
        focused: Some(0),
        pivot: Some(0),
    };
    assert!(collapse_selected_groups(&g, &mut c, &mut s));
    assert_eq!(c, collapsed(&[10, 20]));
    assert_eq!(s.selected, set(&[0, 4]));
}

#[test]
fn left_without_a_selection_changes_nothing() {
    let g = two_groups();
    let mut c = HashSet::new();
    let mut s = PlaylistSelection {
        focused: Some(1),
        ..PlaylistSelection::default()
    };
    assert!(!collapse_selected_groups(&g, &mut c, &mut s));
    assert!(c.is_empty());
    assert_eq!(s.focused, Some(1));
}

#[test]
fn right_expands_every_touched_group() {
    let g = two_groups();
    let mut c = collapsed(&[10, 20]);
    let s = PlaylistSelection {
        selected: set(&[3]),
        ..PlaylistSelection::default()
    };
    assert!(expand_selected_groups(&g, &mut c, &s));
    assert_eq!(c, collapsed(&[10]));
    assert!(!expand_selected_groups(
        &g,
        &mut c,
        &PlaylistSelection::default()
    ));
}

// ------------------------------------------------------------
// Enabled flags
// ------------------------------------------------------------

#[test]
fn toggling_a_song_flips_it_and_returns_the_new_state() {
    let mut g = two_groups();
    assert_eq!(toggle_song_enabled(&mut g, None, 2), Some(false));
    assert!(!g[0].songs[1].enabled);
    assert_eq!(toggle_song_enabled(&mut g, None, 2), Some(true));
    assert_eq!(toggle_song_enabled(&mut g, None, 3), None);
}

#[test]
fn toggling_a_filtered_song_mirrors_it_into_the_full_list_by_id() {
    let mut full = two_groups();
    let mut filtered = vec![group(20, vec![song(4, true)])];
    assert_eq!(
        toggle_song_enabled(&mut full, Some(&mut filtered), 1),
        Some(false)
    );
    assert!(!filtered[0].songs[0].enabled);
    assert!(!full[1].songs[1].enabled);
    assert!(full[1].songs[0].enabled);
}

#[test]
fn setting_a_song_mirrors_the_first_match_of_each_full_group() {
    let mut full = two_groups();
    full[0].songs.push(song(4, true));
    let mut filtered = vec![group(20, vec![song(4, true)])];
    set_song_enabled(&mut full, Some(&mut filtered), 1, false);
    assert!(!full[0].songs[2].enabled);
    assert!(!full[1].songs[1].enabled);
    set_song_enabled(&mut full, None, 1, false);
    assert!(!full[0].songs[0].enabled);
}

#[test]
fn a_group_toggle_enables_a_fully_disabled_group_else_disables_it() {
    let mut g = two_groups();
    g[1].songs[0].enabled = false;
    toggle_group_enabled(&mut g, None, 3);
    assert!(g[1].songs.iter().all(|s| !s.enabled));
    toggle_group_enabled(&mut g, None, 3);
    assert!(g[1].songs.iter().all(|s| s.enabled));
}

#[test]
fn a_group_toggle_on_a_song_index_does_nothing() {
    let mut g = two_groups();
    toggle_group_enabled(&mut g, None, 4);
    set_group_enabled(&mut g, None, 4, false);
    assert!(g[1].songs.iter().all(|s| s.enabled));
}

#[test]
fn a_filtered_group_change_applies_to_the_whole_full_group_by_anchor() {
    let mut full = two_groups();
    let mut filtered = vec![group(20, vec![song(4, true)])];
    toggle_group_enabled(&mut full, Some(&mut filtered), 0);
    assert!(!filtered[0].songs[0].enabled);
    assert!(full[1].songs.iter().all(|s| !s.enabled));
    assert!(full[0].songs.iter().all(|s| s.enabled));
    set_group_enabled(&mut full, Some(&mut filtered), 0, true);
    assert!(full[1].songs.iter().all(|s| s.enabled));
}

#[test]
fn a_bulk_toggle_enables_when_anything_targeted_is_disabled() {
    let mut g = two_groups();
    assert!(!bulk_enable_target(&g, &g, [1, 3]));
    g[1].songs[2].enabled = false;
    assert!(bulk_enable_target(&g, &g, [1, 3]));
    assert!(!bulk_enable_target(&g, &g, [1, 4]));
    assert!(bulk_enable_target(&g, &g, [6]));
    assert!(!bulk_enable_target(&g, &g, [99]));
}

#[test]
fn a_bulk_toggle_reads_separators_from_the_full_list() {
    let mut full = two_groups();
    full[1].songs[2].enabled = false;
    let filtered = vec![group(20, vec![song(3, true)])];
    assert!(bulk_enable_target(&full, &filtered, [0]));
    assert!(!bulk_enable_target(&full, &filtered, [1]));
}

// ------------------------------------------------------------
// Search filter
// ------------------------------------------------------------

#[test]
fn an_empty_query_shows_the_full_list() {
    assert!(filter_groups(&two_groups(), "").is_none());
}

#[test]
fn the_filter_matches_case_insensitively_and_recomputes_durations() {
    let mut g = two_groups();
    g[1].songs[1].title = "Space ODDITY".into();
    let filtered = filter_groups(&g, "oddity").unwrap_or_default();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].first_item_id, 20);
    assert_eq!(filtered[0].songs.len(), 1);
    assert_eq!(filtered[0].total_duration, 104.0);
}

#[test]
fn the_filter_matches_artist_album_artist_and_album() {
    let mut g = two_groups();
    g[0].songs[0].artist_name = "Bowie".into();
    g[0].songs[1].album_artist_name = "Bowie & Friends".into();
    g[1].songs[2].album_title = "Bowie Live".into();
    let filtered = filter_groups(&g, "BOWIE").unwrap_or_default();
    let ids: Vec<i64> = filtered
        .iter()
        .flat_map(|g| g.songs.iter().map(|s| s.song_id))
        .collect();
    assert_eq!(ids, [1, 2, 5]);
}

#[test]
fn a_query_with_no_match_shows_the_full_list() {
    assert!(filter_groups(&two_groups(), "zzz").is_none());
}

// ------------------------------------------------------------
// Playback order
// ------------------------------------------------------------

#[test]
fn repeat_modes_map_to_their_persisted_numbers() {
    assert_eq!(RepeatMode::from_u8(0), RepeatMode::Off);
    assert_eq!(RepeatMode::from_u8(1), RepeatMode::All);
    assert_eq!(RepeatMode::from_u8(2), RepeatMode::One);
    assert_eq!(RepeatMode::from_u8(7), RepeatMode::Off);
    for m in [RepeatMode::Off, RepeatMode::All, RepeatMode::One] {
        assert_eq!(RepeatMode::from_u8(m.as_u8()), m);
        assert_eq!(m.cycled().as_u8(), (m.as_u8() + 1) % 3);
    }
}

#[test]
fn sequential_next_skips_disabled_songs_and_separators() {
    let mut g = two_groups();
    g[0].songs[1].enabled = false;
    assert_eq!(
        peek_next(&g, Some(1), RepeatMode::Off, false, None),
        NextStep::Sequential(4)
    );
}

#[test]
fn sequential_next_from_nothing_starts_at_the_first_enabled_song() {
    let mut g = two_groups();
    g[0].songs[0].enabled = false;
    assert_eq!(
        peek_next(&g, None, RepeatMode::Off, false, None),
        NextStep::Sequential(2)
    );
}

#[test]
fn sequential_next_stops_at_the_end_without_repeat_all() {
    let g = two_groups();
    assert_eq!(
        peek_next(&g, Some(6), RepeatMode::Off, false, None),
        NextStep::Stop
    );
}

#[test]
fn sequential_next_wraps_with_repeat_all() {
    let g = two_groups();
    assert_eq!(
        peek_next(&g, Some(6), RepeatMode::All, false, None),
        NextStep::Sequential(1)
    );
}

#[test]
fn sequential_wrap_can_return_the_only_enabled_song_itself() {
    let mut g = two_groups();
    for s in g.iter_mut().flat_map(|g| g.songs.iter_mut()) {
        s.enabled = false;
    }
    g[1].songs[1].enabled = true;
    assert_eq!(find_next_enabled(&g, Some(5), true), Some(5));
    assert_eq!(find_next_enabled(&g, Some(5), false), None);
}

#[test]
fn repeat_one_replays_the_playing_item_even_in_shuffle() {
    let g = two_groups();
    let sess = session(&[3, 4], 0, &[1]);
    assert_eq!(
        peek_next(&g, Some(2), RepeatMode::One, true, Some(&sess)),
        NextStep::Replay(2)
    );
    assert_eq!(
        peek_next(&g, None, RepeatMode::One, false, None),
        NextStep::Stop
    );
}

#[test]
fn shuffle_next_takes_the_first_enabled_song_from_the_position() {
    let mut g = two_groups();
    g[1].songs[0].enabled = false; // song 3
    let sess = session(&[1, 3, 5, 2], 1, &[1]);
    assert_eq!(
        peek_next(&g, Some(1), RepeatMode::Off, true, Some(&sess)),
        NextStep::Shuffle {
            linear_idx: 6,
            song_id: 5,
            next_position: 3
        }
    );
}

#[test]
fn shuffle_next_skips_songs_no_longer_in_the_list() {
    let g = two_groups();
    let sess = session(&[42, 4], 0, &[]);
    assert_eq!(
        peek_next(&g, Some(1), RepeatMode::Off, true, Some(&sess)),
        NextStep::Shuffle {
            linear_idx: 5,
            song_id: 4,
            next_position: 2
        }
    );
}

#[test]
fn an_exhausted_shuffle_stops_without_repeat_all_and_reshuffles_with_it() {
    let g = two_groups();
    let sess = session(&[2], 1, &[2]);
    assert_eq!(
        peek_next(&g, Some(2), RepeatMode::Off, true, Some(&sess)),
        NextStep::Stop
    );
    assert_eq!(
        peek_next(&g, Some(2), RepeatMode::All, true, Some(&sess)),
        NextStep::Reshuffle
    );
    assert_eq!(
        peek_next(&g, Some(2), RepeatMode::All, true, None),
        NextStep::Reshuffle
    );
    assert_eq!(
        peek_next(&g, Some(2), RepeatMode::Off, true, None),
        NextStep::Stop
    );
}

#[test]
fn a_position_past_the_order_is_exhausted() {
    let g = two_groups();
    let sess = session(&[2], 5, &[]);
    assert_eq!(
        peek_next(&g, Some(1), RepeatMode::Off, true, Some(&sess)),
        NextStep::Stop
    );
}

#[test]
fn the_preload_song_skips_disabled_songs_like_sequential_play() {
    // Ported from gui::playlist test_get_next_song_ref_sequential_skips_disabled.
    let g = one_group(vec![song(1, false), song(2, true)]);
    let next = next_song_ref(&g, Some(1), RepeatMode::Off, false, None);
    assert_eq!(next.map(|s| s.song_id), Some(2));
}

#[test]
fn the_preload_song_skips_disabled_songs_in_shuffle() {
    // Ported from gui::playlist test_get_next_song_ref_shuffle_skips_disabled.
    let g = one_group(vec![song(1, false), song(2, true), song(3, true)]);
    let sess = session(&[1, 3, 2], 0, &[4]);
    let next = next_song_ref(&g, Some(1), RepeatMode::Off, true, Some(&sess));
    assert_eq!(next.map(|s| s.song_id), Some(3));
}

#[test]
fn the_preload_song_is_none_at_the_end_of_the_shuffle() {
    // Ported from gui::playlist test_get_next_song_ref_shuffle_end_returns_none.
    let g = one_group(vec![song(1, true)]);
    let sess = session(&[1], 1, &[1]);
    assert!(next_song_ref(&g, Some(1), RepeatMode::Off, true, Some(&sess)).is_none());
    assert!(next_song_ref(&g, Some(1), RepeatMode::All, true, Some(&sess)).is_none());
}

#[test]
fn the_preload_song_is_none_when_nothing_plays() {
    let g = two_groups();
    assert!(next_song_ref(&g, None, RepeatMode::Off, false, None).is_none());
}

#[test]
fn the_preload_song_is_the_song_play_next_starts() {
    let mut rng = fastrand::Rng::with_seed(7);
    let mut g = two_groups();
    g[0].songs[1].enabled = false;
    let cases = [
        (Some(1), RepeatMode::Off, false, None),
        (Some(6), RepeatMode::All, false, None),
        (Some(4), RepeatMode::One, false, None),
        (
            Some(1),
            RepeatMode::Off,
            true,
            Some(session(&[2, 5, 3], 0, &[1])),
        ),
        (
            Some(6),
            RepeatMode::Off,
            true,
            Some(session(&[5, 4], 1, &[1, 5])),
        ),
    ];
    for (playing, repeat, shuffle, sess) in cases {
        let preview = next_song_ref(&g, playing, repeat, shuffle, sess.as_ref()).map(|s| s.song_id);
        let (mut p, mut sh, mut se) = (playing, shuffle, sess.clone());
        let played = advance_next(&g, &mut p, repeat, &mut sh, &mut se, &mut rng)
            .and_then(|idx| song_at(&g, idx))
            .map(|s| s.song_id);
        assert_eq!(
            preview, played,
            "case {playing:?} {repeat:?} shuffle={shuffle}"
        );
    }
}

#[test]
fn advancing_sequentially_updates_the_playing_index() {
    let mut rng = fastrand::Rng::with_seed(1);
    let g = two_groups();
    let (mut playing, mut shuffle, mut sess) = (Some(2), false, None);
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::Off,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, Some(4));
    assert_eq!(playing, Some(4));
}

#[test]
fn advancing_at_the_end_without_repeat_keeps_the_playing_index() {
    let mut rng = fastrand::Rng::with_seed(1);
    let g = two_groups();
    let (mut playing, mut shuffle, mut sess) = (Some(6), false, None);
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::Off,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, None);
    assert_eq!(playing, Some(6));
}

#[test]
fn repeat_one_replays_without_moving() {
    let mut rng = fastrand::Rng::with_seed(1);
    let g = two_groups();
    let (mut playing, mut shuffle, mut sess) = (Some(5), false, None);
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::One,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, Some(5));
    assert_eq!(playing, Some(5));
    // A repeat on a separator index plays nothing.
    let mut on_sep = Some(3);
    assert_eq!(
        advance_next(
            &g,
            &mut on_sep,
            RepeatMode::One,
            &mut shuffle,
            &mut sess,
            &mut rng
        ),
        None
    );
}

#[test]
fn a_shuffle_step_records_the_history_and_the_position() {
    let mut rng = fastrand::Rng::with_seed(1);
    let g = two_groups();
    let (mut playing, mut shuffle) = (Some(1), true);
    let mut sess = Some(session(&[4, 2], 0, &[1]));
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::Off,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, Some(5));
    assert_eq!(playing, Some(5));
    let s = sess.unwrap_or_else(|| session(&[], 0, &[]));
    assert_eq!(s.history, [1, 4]);
    assert_eq!(s.current_position, 1);
}

#[test]
fn an_exhausted_shuffle_with_repeat_all_reshuffles_and_plays() {
    let mut rng = fastrand::Rng::with_seed(3);
    let g = two_groups();
    let (mut playing, mut shuffle) = (Some(1), true);
    let mut sess = Some(session(&[1], 1, &[1]));
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::All,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    let s = sess.unwrap_or_else(|| session(&[], 0, &[]));
    assert!(shuffle);
    assert_ne!(s.session_id, "s1");
    // New session seeded with the playing song, which then played the first of the order.
    assert_eq!(s.history.len(), 2);
    assert_eq!(s.history[0], 1);
    assert_eq!(s.current_position, 1);
    assert_eq!(s.shuffle_order.len(), 4);
    assert!(!s.shuffle_order.contains(&1));
    assert_eq!(target, playing);
    assert_eq!(
        target.and_then(|i| song_at(&g, i)).map(|x| x.song_id),
        Some(s.shuffle_order[0])
    );
}

#[test]
fn an_exhausted_shuffle_without_repeat_stops() {
    let mut rng = fastrand::Rng::with_seed(3);
    let g = two_groups();
    let (mut playing, mut shuffle) = (Some(1), true);
    let mut sess = Some(session(&[1], 1, &[1]));
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::Off,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, None);
    assert_eq!(playing, Some(1));
    assert_eq!(sess.map(|s| s.session_id), Some("s1".to_string()));
}

#[test]
fn a_reshuffle_with_nothing_enabled_turns_shuffle_off_and_plays_nothing() {
    let mut rng = fastrand::Rng::with_seed(3);
    let g = one_group(vec![song(1, false)]);
    let (mut playing, mut shuffle) = (Some(1), true);
    let mut sess = Some(session(&[1], 1, &[1]));
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::All,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, None);
    assert!(!shuffle);
    // The previous session is left untouched.
    assert_eq!(sess.map(|s| s.session_id), Some("s1".to_string()));
}

/// Song 1 listed twice: its first copy (group A, item 1) is disabled and its
/// second copy (group B, item 11) is enabled. Song 2 is disabled.
/// Linear: 0 sep A, 1 s1 (off), 2 s2 (off), 3 sep B, 4 s1 copy (on).
fn duplicate_with_disabled_first_copy() -> Vec<PlaylistFolderGroup> {
    let mut copy = song(1, true);
    copy.item_id = 11;
    vec![
        group(10, vec![song(1, false), song(2, false)]),
        group(20, vec![copy]),
    ]
}

#[test]
fn the_enabled_copy_of_a_song_is_found_when_its_first_copy_is_disabled() {
    let g = duplicate_with_disabled_first_copy();
    assert_eq!(linear_index_of_song_id(&g, 1), Some(1));
    assert_eq!(enabled_linear_index_of_song_id(&g, 1), Some(4));
    assert_eq!(enabled_linear_index_of_song_id(&g, 2), None);
    // When the first copy is enabled, it is the one found.
    let g = two_groups();
    assert_eq!(enabled_linear_index_of_song_id(&g, 4), Some(5));
}

#[test]
fn d1_shuffle_plays_the_enabled_copy_of_a_song_whose_first_copy_is_disabled() {
    let g = duplicate_with_disabled_first_copy();
    let sess = session(&[1], 0, &[]);
    assert_eq!(
        peek_next(&g, None, RepeatMode::Off, true, Some(&sess)),
        NextStep::Shuffle {
            linear_idx: 4,
            song_id: 1,
            next_position: 1
        }
    );
    let next = next_song_ref(&g, Some(2), RepeatMode::Off, true, Some(&sess));
    assert_eq!(next.map(|s| s.item_id), Some(11));
}

#[test]
fn d1_repeat_all_reshuffle_plays_the_enabled_copy_instead_of_recursing() {
    // Before the fix, every re-shuffle resolved song 1 to its disabled first
    // copy, so `advance_next` re-shuffled until the stack overflowed.
    let mut rng = fastrand::Rng::with_seed(5);
    let g = duplicate_with_disabled_first_copy();
    let (mut playing, mut shuffle) = (Some(2), true);
    let mut sess = Some(session(&[1], 1, &[1]));
    let target = advance_next(
        &g,
        &mut playing,
        RepeatMode::All,
        &mut shuffle,
        &mut sess,
        &mut rng,
    );
    assert_eq!(target, Some(4));
    assert_eq!(playing, Some(4));
    assert!(shuffle);
    let s = sess.unwrap_or_else(|| session(&[], 0, &[]));
    assert_ne!(s.session_id, "s1");
    assert_eq!(s.shuffle_order, [1]);
    assert_eq!(s.current_position, 1);
}

#[test]
fn d1_repeat_all_keeps_reshuffling_and_playing_enabled_copies() {
    let mut rng = fastrand::Rng::with_seed(11);
    let mut g = duplicate_with_disabled_first_copy();
    let mut copy = song(2, true);
    copy.item_id = 12;
    g[1].songs.push(copy); // Linear 5: song 2's enabled copy.
    let (mut playing, mut shuffle, mut sess) = (None, false, None);
    activate_shuffle(&g, playing, &mut shuffle, &mut sess, &mut rng);
    for round in 0..20 {
        let preview =
            next_song_ref(&g, playing, RepeatMode::All, shuffle, sess.as_ref()).map(|s| s.item_id);
        let target = advance_next(
            &g,
            &mut playing,
            RepeatMode::All,
            &mut shuffle,
            &mut sess,
            &mut rng,
        );
        let played = target.and_then(|idx| song_at(&g, idx));
        assert!(
            played.is_some_and(|s| s.enabled),
            "round {round}: {target:?}"
        );
        // Whenever the preload knows the next song, it is the one played.
        if playing.is_some() && preview.is_some() {
            assert_eq!(preview, played.map(|s| s.item_id), "round {round}");
        }
        assert!(shuffle);
    }
}

#[test]
fn d1_nothing_enabled_returns_no_next_without_looping() {
    let mut rng = fastrand::Rng::with_seed(3);
    let mut g = duplicate_with_disabled_first_copy();
    g[1].songs[0].enabled = false;
    // An order that still names the (now disabled) songs.
    for order in [&[1, 2, 1][..], &[][..]] {
        let (mut playing, mut shuffle) = (Some(4), true);
        let mut sess = Some(session(order, 0, &[]));
        assert_eq!(
            peek_next(&g, playing, RepeatMode::All, true, sess.as_ref()),
            NextStep::Reshuffle
        );
        let target = advance_next(
            &g,
            &mut playing,
            RepeatMode::All,
            &mut shuffle,
            &mut sess,
            &mut rng,
        );
        assert_eq!(target, None, "order {order:?}");
        assert_eq!(playing, Some(4));
        assert!(!shuffle);
    }
}

#[test]
fn shuffle_prev_pops_the_history() {
    let g = two_groups();
    let mut sess = session(&[4, 5], 2, &[1, 4, 5]);
    let prev = step_prev(&g, Some(6), RepeatMode::Off, true, Some(&mut sess));
    assert_eq!(prev, Some(5));
    assert_eq!(sess.history, [1, 4]);
    assert_eq!(sess.current_position, 1);
}

#[test]
fn shuffle_prev_with_a_single_history_entry_falls_back_to_sequential() {
    let g = two_groups();
    let mut sess = session(&[4, 5], 0, &[5]);
    let prev = step_prev(&g, Some(6), RepeatMode::Off, true, Some(&mut sess));
    assert_eq!(prev, Some(5));
    assert_eq!(sess.history, [5]);
}

#[test]
fn shuffle_prev_to_a_removed_song_pops_then_falls_back_to_sequential() {
    let g = two_groups();
    let mut sess = session(&[], 0, &[42, 5]);
    let prev = step_prev(&g, Some(6), RepeatMode::Off, true, Some(&mut sess));
    assert_eq!(sess.history, [42]);
    assert_eq!(prev, Some(5));
}

#[test]
fn sequential_prev_finds_the_previous_enabled_song() {
    let mut g = two_groups();
    g[0].songs[1].enabled = false;
    assert_eq!(
        step_prev(&g, Some(4), RepeatMode::Off, false, None),
        Some(1)
    );
    assert_eq!(step_prev(&g, Some(1), RepeatMode::Off, false, None), None);
}

#[test]
fn sequential_prev_from_the_first_song_does_not_wrap_even_with_repeat_all() {
    let g = two_groups();
    assert_eq!(step_prev(&g, Some(1), RepeatMode::All, false, None), None);
}

#[test]
fn sequential_prev_from_nothing_wraps_to_the_last_enabled_song_only_with_repeat_all() {
    let mut g = two_groups();
    g[1].songs[2].enabled = false;
    assert_eq!(step_prev(&g, None, RepeatMode::All, false, None), Some(5));
    assert_eq!(step_prev(&g, None, RepeatMode::Off, false, None), None);
    assert_eq!(find_last_enabled(&g), Some(5));
    assert_eq!(find_last_enabled(&[]), None);
}

#[test]
fn d14_sequential_prev_from_the_first_song_wraps_to_the_last_with_repeat_all() {
    let mut g = two_groups();
    g[1].songs[2].enabled = false; // song 5
    assert_eq!(
        step_prev_fixed(&g, Some(1), RepeatMode::All, false, None),
        Some(5),
        "the last enabled song"
    );
    assert_eq!(
        step_prev_fixed(&g, Some(1), RepeatMode::Off, false, None),
        None
    );
    // The original keeps not wrapping (Iced).
    assert_eq!(step_prev(&g, Some(1), RepeatMode::All, false, None), None);
}

#[test]
fn d14_sequential_prev_wraps_when_only_disabled_songs_come_before() {
    let mut g = two_groups();
    g[0].songs[0].enabled = false; // song 1
    g[0].songs[1].enabled = false; // song 2
    assert_eq!(
        step_prev_fixed(&g, Some(4), RepeatMode::All, false, None),
        Some(6)
    );
    assert_eq!(
        step_prev_fixed(&g, Some(4), RepeatMode::One, false, None),
        None
    );
}

#[test]
fn d14_sequential_prev_with_a_single_enabled_song_wraps_to_itself() {
    let g = one_group(vec![song(1, true), song(2, false)]);
    assert_eq!(
        step_prev_fixed(&g, Some(1), RepeatMode::All, false, None),
        Some(1)
    );
}

#[test]
fn d14_the_fixed_prev_matches_the_original_elsewhere() {
    let mut g = two_groups();
    g[0].songs[1].enabled = false;
    for playing in [None, Some(1), Some(2), Some(4), Some(5), Some(6)] {
        for repeat in [RepeatMode::Off, RepeatMode::One] {
            assert_eq!(
                step_prev_fixed(&g, playing, repeat, false, None),
                step_prev(&g, playing, repeat, false, None),
                "{playing:?} {repeat:?}"
            );
        }
    }
    assert_eq!(
        step_prev_fixed(&g, Some(5), RepeatMode::All, false, None),
        Some(4)
    );
    assert_eq!(
        step_prev_fixed(&g, None, RepeatMode::All, false, None),
        step_prev(&g, None, RepeatMode::All, false, None)
    );
}

#[test]
fn d15_shuffle_prev_skips_history_entries_that_left_the_list() {
    let g = two_groups();
    let mut sess = session(&[3, 4], 3, &[1, 42, 43, 5]);
    let prev = step_prev_fixed(&g, Some(6), RepeatMode::Off, true, Some(&mut sess));
    assert_eq!(prev, Some(1), "song 1, not the removed 43 and 42");
    assert_eq!(sess.history, [1]);
    assert_eq!(sess.current_position, 0, "one step back per popped entry");

    // The original rewinds into the removed song, then plays the sequential
    // previous one.
    let mut original = session(&[3, 4], 3, &[1, 42, 43, 5]);
    let prev = step_prev(&g, Some(6), RepeatMode::Off, true, Some(&mut original));
    assert_eq!(prev, Some(5));
    assert_eq!(original.history, [1, 42, 43]);
}

#[test]
fn d15_without_an_earlier_song_in_the_list_the_session_is_kept_and_sequential_applies() {
    let g = two_groups();
    let mut sess = session(&[3], 2, &[42, 5]);
    let prev = step_prev_fixed(&g, Some(6), RepeatMode::Off, true, Some(&mut sess));
    assert_eq!(prev, Some(5), "the sequential previous song");
    assert_eq!(sess.history, [42, 5]);
    assert_eq!(sess.current_position, 2);
}

#[test]
fn d15_shuffle_prev_without_removed_songs_pops_one_entry_like_the_original() {
    let g = two_groups();
    let mut fixed = session(&[4, 5], 2, &[1, 4, 5]);
    let mut original = fixed.clone();
    assert_eq!(
        step_prev_fixed(&g, Some(6), RepeatMode::Off, true, Some(&mut fixed)),
        step_prev(&g, Some(6), RepeatMode::Off, true, Some(&mut original))
    );
    assert_eq!(fixed.history, original.history);
    assert_eq!(fixed.current_position, original.current_position);

    let mut single = session(&[4, 5], 0, &[5]);
    assert_eq!(
        step_prev_fixed(&g, Some(6), RepeatMode::Off, true, Some(&mut single)),
        Some(5),
        "a single entry falls back to sequential"
    );
    assert_eq!(single.history, [5]);
}

#[test]
fn d16_activating_shuffle_without_enabled_songs_drops_the_old_session() {
    let g = one_group(vec![song(1, false), song(2, false)]);
    let mut active = false;
    let mut sess = Some(session(&[1, 2], 1, &[1]));
    activate_shuffle_fixed(
        &g,
        None,
        &mut active,
        &mut sess,
        &mut fastrand::Rng::with_seed(1),
    );
    assert!(!active);
    assert!(sess.is_none(), "the stale session is gone");

    // The original keeps it (Iced).
    let mut original = Some(session(&[1, 2], 1, &[1]));
    activate_shuffle(
        &g,
        None,
        &mut active,
        &mut original,
        &mut fastrand::Rng::with_seed(1),
    );
    assert!(original.is_some());
}

#[test]
fn d16_activating_shuffle_with_enabled_songs_matches_the_original() {
    let g = two_groups();
    let (mut fixed_active, mut original_active) = (false, false);
    let (mut fixed, mut original) = (None, None);
    activate_shuffle_fixed(
        &g,
        Some(2),
        &mut fixed_active,
        &mut fixed,
        &mut fastrand::Rng::with_seed(9),
    );
    activate_shuffle(
        &g,
        Some(2),
        &mut original_active,
        &mut original,
        &mut fastrand::Rng::with_seed(9),
    );
    assert!(fixed_active && original_active);
    assert_eq!(
        fixed.map(|s| (s.session_id, s.shuffle_order, s.history)),
        original.map(|s| (s.session_id, s.shuffle_order, s.history))
    );
}

#[test]
fn a_new_shuffle_session_holds_the_enabled_songs_except_the_playing_one() {
    let mut rng = fastrand::Rng::with_seed(11);
    let mut g = two_groups();
    g[1].songs[1].enabled = false; // song 4
    let s = new_shuffle_session(&g, Some(2), &mut rng);
    let s = s.unwrap_or_else(|| session(&[], 0, &[]));
    let mut order = s.shuffle_order.clone();
    order.sort_unstable();
    assert_eq!(order, [1, 3, 5]);
    assert_eq!(s.history, [2]);
    assert_eq!(s.current_position, 0);
    assert!(s.session_id.starts_with("sess-"));
    assert!(s.session_id["sess-".len()..].parse::<u64>().is_ok());
}

#[test]
fn a_new_shuffle_session_is_deterministic_for_a_seed() {
    let g = two_groups();
    let a = new_shuffle_session(&g, None, &mut fastrand::Rng::with_seed(5));
    let b = new_shuffle_session(&g, None, &mut fastrand::Rng::with_seed(5));
    assert_eq!(
        a.as_ref().map(|s| &s.shuffle_order),
        b.as_ref().map(|s| &s.shuffle_order)
    );
    assert_eq!(a.map(|s| s.session_id), b.map(|s| s.session_id));
}

#[test]
fn a_new_shuffle_session_without_playing_song_has_an_empty_history() {
    let g = two_groups();
    let s = new_shuffle_session(&g, None, &mut fastrand::Rng::with_seed(5));
    assert_eq!(
        s.map(|s| (s.shuffle_order.len(), s.history.len())),
        Some((5, 0))
    );
}

#[test]
fn activating_shuffle_without_enabled_songs_turns_it_off() {
    let g = one_group(vec![song(1, false)]);
    let mut active = true;
    let mut sess = None;
    activate_shuffle(
        &g,
        None,
        &mut active,
        &mut sess,
        &mut fastrand::Rng::with_seed(1),
    );
    assert!(!active);
    assert!(sess.is_none());
    let g = two_groups();
    activate_shuffle(
        &g,
        None,
        &mut active,
        &mut sess,
        &mut fastrand::Rng::with_seed(1),
    );
    assert!(active);
    assert!(sess.is_some());
}

#[test]
fn a_manual_play_joins_the_history_and_leaves_the_future_order() {
    let mut s = session(&[1, 2, 3, 4], 1, &[1]);
    record_manual_play(&mut s, 3);
    assert_eq!(s.history, [1, 3]);
    assert_eq!(s.shuffle_order, [1, 2, 4]);
    // Already played (before the position): stays in the order.
    record_manual_play(&mut s, 1);
    assert_eq!(s.shuffle_order, [1, 2, 4]);
    assert_eq!(s.history, [1, 3, 1]);
}

#[test]
fn a_manual_play_at_the_end_of_the_order_only_joins_the_history() {
    let mut s = session(&[1], 1, &[1]);
    record_manual_play(&mut s, 1);
    assert_eq!(s.shuffle_order, [1]);
    assert_eq!(s.history, [1, 1]);
}

#[test]
fn a_shuffle_step_commit_pushes_history_and_sets_the_position() {
    let mut s = session(&[1, 2], 0, &[]);
    commit_shuffle_step(&mut s, 2, 2);
    assert_eq!(s.history, [2]);
    assert_eq!(s.current_position, 2);
}

#[test]
fn next_step_targets_are_known_except_for_reshuffle_and_stop() {
    assert_eq!(NextStep::Replay(3).target(), Some(3));
    assert_eq!(NextStep::Sequential(4).target(), Some(4));
    assert_eq!(
        NextStep::Shuffle {
            linear_idx: 5,
            song_id: 9,
            next_position: 1
        }
        .target(),
        Some(5)
    );
    assert_eq!(NextStep::Reshuffle.target(), None);
    assert_eq!(NextStep::Stop.target(), None);
}
