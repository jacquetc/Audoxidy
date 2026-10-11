//! The list over the mock "David Bowie" playlist: separator 0 ("FLAC
//! (16bit-44.1kHz)", songs 1 to 10), separator 11 ("CD 02", songs 12 to 15),
//! in a 400 × 480 region.

use teksilo::canvas::Point;
use teksilo::core::{PointerButton, WidgetTree};

use super::*;
use crate::db::database::PlaylistSongRef;
use crate::frontend::ports::PlayerPort;
use crate::teksilo_ui::playlist::testing::{Env, all_of_type, mount, open_menu, restored};
use crate::teksilo_ui::test_support::{
    key_down, key_down_with, layout, pointer_click, with_shipped_messages,
};

fn mounted(env: &Env) -> (WidgetTree, WidgetId, Rc<Cell<Option<WidgetId>>>) {
    let list = PlaylistList::new(env.vm.clone());
    let list_id = list.list_id();
    let (tree, id, _, _) = mount(env, list);
    (tree, id, list_id)
}

/// The realized rows, top to bottom, with their bounds.
fn rows(tree: &WidgetTree, root: WidgetId) -> Vec<(WidgetId, Rect)> {
    let mut rows: Vec<(WidgetId, Rect)> = all_of_type(tree, root, "PlaylistRow")
        .into_iter()
        .map(|id| (id, tree.bounds(id)))
        .collect();
    rows.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    rows
}

/// The first node of type `suffix` under `root`, with its bounds.
fn first(tree: &WidgetTree, root: WidgetId, suffix: &str) -> Rect {
    tree.bounds(all_of_type(tree, root, suffix)[0])
}

/// The centre of the row at `row` (from the top of the list, unscrolled).
fn row_point(row: usize, x: f32) -> Point {
    Point::new(x, row as f32 * ROW_HEIGHT + ROW_HEIGHT / 2.0)
}

fn click(tree: &mut WidgetTree, at: Point, ms: u64) {
    pointer_click(tree, at, PointerButton::Primary, ms);
}

fn right_click(tree: &mut WidgetTree, at: Point, ms: u64) {
    pointer_click(tree, at, PointerButton::Secondary, ms);
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[test]
fn a_press_lands_on_a_row_its_dot_or_the_background() {
    let env = restored();
    let rows = env.vm.rows().get();
    // Separator 0: its dot is 10..22 px in, on its 35 px line.
    assert_eq!(hit(&rows, 15.0, 10.0, 0.0), ListHit::GroupDot(0));
    assert_eq!(
        hit(&rows, 15.0, 40.0, 0.0),
        ListHit::Row(0),
        "under the line"
    );
    assert_eq!(hit(&rows, 200.0, 10.0, 0.0), ListHit::Row(0));
    // Song 1: its dot is 15..27 px in, the row's height.
    assert_eq!(hit(&rows, 20.0, 50.0, 0.0), ListHit::SongDot(1));
    assert_eq!(hit(&rows, 30.0, 50.0, 0.0), ListHit::Row(1));
    // Scrolled by 92 px: the top row is song 2.
    assert_eq!(hit(&rows, 200.0, 1.0, 92.0), ListHit::Row(2));
    assert_eq!(
        hit(&rows, 200.0, 16.0 * ROW_HEIGHT + 1.0, 0.0),
        ListHit::Background
    );
}

#[test]
fn smart_text_cuts_by_the_iced_estimate() {
    // 13 px: 7.15 px a character.
    assert_eq!(smart_truncate("Time", 13.0, 100.0), "Time");
    assert_eq!(
        smart_truncate("Lady Grinning Soul", 13.0, 100.0),
        "Lady Grinn..."
    );
    assert_eq!(smart_truncate("abc", 13.0, 0.0), "...");
}

fn song(track: Option<&str>, year: Option<&str>) -> PlaylistSongRef {
    PlaylistSongRef {
        item_id: 1,
        song_id: 1,
        sequence_order: 1.0,
        enabled: true,
        title: "Time".into(),
        artist_name: "David Bowie".into(),
        album_title: "Aladdin Sane".into(),
        album_artist_name: "David Bowie".into(),
        year: year.map(Into::into),
        genre: None,
        track_number: track.map(Into::into),
        duration: 315.4,
        file_path: "/a.flac".into(),
        folder_path: "/".into(),
        folder_name: "/".into(),
        cover_path: None,
        cover_override: None,
    }
}

#[test]
fn a_song_shows_its_track_number_or_its_position_in_its_group() {
    assert_eq!(track_prefix(&song(Some("6"), None), 6), "6.");
    assert_eq!(track_prefix(&song(None, None), 7), "07.");
    assert_eq!(
        song_details(&song(None, Some("1973"))),
        "David Bowie - Aladdin Sane :: 1973"
    );
    assert_eq!(
        song_details(&song(None, None)),
        "David Bowie - Aladdin Sane :: -"
    );
}

#[test]
fn a_song_row_is_lit_when_it_plays_is_selected_or_its_group_is() {
    let item = SongItem {
        linear_idx: 3,
        separator_idx: 0,
        number: 3,
        song: song(None, None),
    };
    let mut selection = PlaylistSelection::default();
    assert!(!song_lit(&selection, None, &item));
    assert!(song_lit(&selection, Some(3), &item));
    selection.select_only(0);
    assert!(song_lit(&selection, None, &item));
    selection.select_only(3);
    assert!(song_lit(&selection, Some(5), &item));
    assert_eq!(row_fill(true), SurfaceRole::Raised);
    assert_eq!(row_fill(false), SurfaceRole::Transparent);
    assert_eq!(text_role(true), TextRole::Primary);
    assert_eq!(text_role(false), TextRole::Secondary);
}

#[test]
fn the_accelerator_toggles_and_shift_extends() {
    assert_eq!(
        select_modifiers(Modifiers::COMMAND),
        SelectModifiers {
            toggle: true,
            range: false
        }
    );
    assert_eq!(
        select_modifiers(Modifiers::SHIFT),
        SelectModifiers {
            toggle: false,
            range: true
        }
    );
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[test]
fn the_rows_are_46_pixels_tall_from_the_top_of_the_list() {
    let env = restored();
    let (tree, root, _) = mounted(&env);
    let rows = rows(&tree, root);
    assert!(rows.len() >= 11, "the viewport's rows are realized");
    for (n, (_, bounds)) in rows.iter().enumerate().take(11) {
        assert_eq!(bounds.y, n as f32 * ROW_HEIGHT);
        assert_eq!(bounds.height, ROW_HEIGHT);
        assert_eq!(bounds.width, 400.0);
    }
}

#[test]
fn a_separator_is_drawn_where_iced_draws_it() {
    let env = restored();
    let (tree, root, _) = mounted(&env);
    let (separator, row) = rows(&tree, root)[0];
    let name = first(&tree, separator, "SmartText");
    assert_eq!(name.x - row.x, SEPARATOR_LEADING + INDICATOR_WIDTH + 5.0);
    assert_eq!(name.height, 16.0, "14 px and 2 of line spacing");
    // The rule: 1 px at 35 px, 4 + 9 px short of the trailing edge.
    let rule = all_of_type(&tree, separator, "RectWidget")
        .into_iter()
        .map(|id| tree.bounds(id))
        .find(|b| b.height == 1.0)
        .unwrap();
    assert_eq!(rule.y - row.y, SEPARATOR_LINE);
    assert_eq!(rule.x - row.x, SEPARATOR_LEADING);
    assert_eq!(
        rule.x + rule.width,
        row.x + 400.0 - RIGHT_GUTTER - ROW_TRAILING
    );
    let stats = all_of_type(&tree, separator, "TextWidget")
        .into_iter()
        .map(|id| tree.bounds(id))
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap();
    assert_eq!(
        stats.x + stats.width,
        rule.x + rule.width,
        "stats at the trailing end"
    );
    assert_eq!(name.x + name.width, stats.x, "the name takes the rest");
}

#[test]
fn a_song_row_puts_its_number_title_and_duration_where_iced_does() {
    let env = restored();
    let (tree, root, _) = mounted(&env);
    let (song, row) = rows(&tree, root)[1];
    let texts: Vec<Rect> = all_of_type(&tree, song, "SmartText")
        .into_iter()
        .map(|id| tree.bounds(id))
        .collect();
    let (title, details) = (texts[0], texts[1]);
    let text_x = SONG_LEADING + INDICATOR_WIDTH + PREFIX_WIDTH + PREFIX_SPACING;
    assert_eq!(title.x - row.x, text_x);
    assert_eq!(details.x - row.x, text_x);
    assert_eq!(title.height, 15.0);
    assert!(details.y > title.y, "the details are under the title");
    let trailing = row.x + 400.0 - RIGHT_GUTTER - ROW_TRAILING;
    let duration = all_of_type(&tree, song, "TextWidget")
        .into_iter()
        .map(|id| tree.bounds(id))
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap();
    assert_eq!(duration.x + duration.width, trailing);
    assert_eq!(title.x + title.width + PREFIX_SPACING, duration.x);
    assert_eq!(details.x + details.width, trailing);
    let lines_top = title.y;
    let lines_bottom = details.y + details.height;
    let middle = (lines_top + lines_bottom) / 2.0;
    assert!(
        (middle - (row.y + ROW_HEIGHT / 2.0)).abs() <= 1.0,
        "centred: {title:?} {details:?} {row:?}"
    );
}

#[test]
fn the_rows_announce_what_they_show() {
    let env = restored();
    let (tree, root, _) = mounted(&env);
    let rows = rows(&tree, root);
    let separator = tree.accessibility_node(rows[0].0);
    assert_eq!(separator.role(), Role::ListItem);
    assert_eq!(separator.name(), Some("FLAC (16bit-44.1kHz), 10 | 41:50"));
    let song = env.vm.song_at(1).unwrap();
    let node = tree.accessibility_node(rows[1].0);
    let name = node.name().unwrap().to_string();
    assert!(name.contains(song.title.as_ref()), "{name}");
    assert!(name.ends_with(&format_duration(song.duration)), "{name}");
}

// ---------------------------------------------------------------------------
// Pointer
// ---------------------------------------------------------------------------

#[test]
fn a_click_selects_a_row_and_a_quick_second_click_plays_it() {
    let env = restored();
    let (mut tree, _, list) = mounted(&env);
    click(&mut tree, row_point(1, 200.0), 1_000);
    assert_eq!(env.selected(), vec![1]);
    assert_eq!(tree.focused(), list.get(), "the list takes the keyboard");
    click(&mut tree, row_point(1, 200.0), 1_100);
    assert_eq!(env.vm.playing_index().get(), Some(1));
    assert_eq!(env.loads(), 1);
}

#[test]
fn a_quick_second_click_on_a_separator_folds_its_group() {
    let env = restored();
    let (mut tree, root, _) = mounted(&env);
    click(&mut tree, row_point(0, 200.0), 1_000);
    click(&mut tree, row_point(0, 200.0), 1_100);
    assert_eq!(env.rows(), vec![0, 11, 12, 13, 14, 15]);
    assert_eq!(rows(&tree, root).len(), 6);
}

#[test]
fn the_dot_toggles_the_song_and_leaves_the_selection() {
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    let dot = Point::new(SONG_LEADING + INDICATOR_WIDTH / 2.0, ROW_HEIGHT * 1.5);
    click(&mut tree, dot, 1_000);
    assert!(!env.vm.song_at(1).unwrap().enabled);
    assert!(env.selected().is_empty());
    // A separator's dot: the whole group.
    let dot = Point::new(
        SEPARATOR_LEADING + INDICATOR_WIDTH / 2.0,
        SEPARATOR_LINE / 2.0,
    );
    click(&mut tree, dot, 2_000);
    assert!((1..=10).all(|idx| !env.vm.song_at(idx).unwrap().enabled));
}

#[test]
fn a_click_on_the_background_clears_the_selection() {
    let env = restored();
    let (mut tree, _, list) = mounted(&env);
    env.vm.toggle_all_groups();
    layout(&mut tree);
    click(&mut tree, row_point(1, 200.0), 1_000);
    assert_eq!(env.selected(), vec![11]);
    click(&mut tree, Point::new(200.0, 300.0), 2_000);
    assert!(env.selected().is_empty());
    assert_eq!(tree.focused(), list.get());
}

#[test]
fn a_right_click_opens_the_row_menu_with_the_three_stubs_disabled() {
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    with_shipped_messages("es-ES", || {
        right_click(&mut tree, row_point(3, 200.0), 1_000);
        assert_eq!(env.selected(), vec![3], "the row became the selection");
        let entries = open_menu(&tree);
        let labels: Vec<&str> = entries.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Reproducir",
                "Agregar archivo",
                "Agregar carpeta",
                "Información",
                "Ubicación del archivo",
                "Mostrar en la biblioteca",
                "Enviar a otra lista",
                "Editor de Etiquetas",
                "Convertidor de audio",
                "Activar | Desactivar",
                "Plegar | Desplegar",
                "Eliminar de la lista",
            ]
        );
        let disabled: Vec<&str> = entries
            .iter()
            .filter(|(_, enabled)| !enabled)
            .map(|(label, _)| label.as_str())
            .collect();
        assert_eq!(
            disabled,
            vec!["Información", "Editor de Etiquetas", "Convertidor de audio"]
        );
    });
}

#[test]
fn the_row_menu_opens_at_the_pointer() {
    // Quirk fix 7: a context menu opens where it was asked for.
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    let at = row_point(2, 150.0);
    right_click(&mut tree, at, 1_000);
    let overlay = *tree.active_overlays().last().unwrap();
    let bounds = tree.overlay_content_bounds(overlay).unwrap();
    assert_eq!((bounds.x, bounds.y), (at.x, at.y));
}

#[test]
fn the_row_menu_plays_the_row() {
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    right_click(&mut tree, row_point(0, 200.0), 1_000);
    let play = tree.find_by_label("Play").unwrap();
    tree.click(play);
    layout(&mut tree);
    assert_eq!(
        env.vm.playing_index().get(),
        Some(1),
        "the group's first song"
    );
    assert!(tree.active_overlays().is_empty(), "the menu closed");
}

#[test]
fn escape_closes_the_row_menu() {
    // Quirk fix 4.
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    right_click(&mut tree, row_point(2, 200.0), 1_000);
    assert!(!tree.active_overlays().is_empty());
    tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut tree);
    assert!(tree.active_overlays().is_empty());
}

#[test]
fn a_right_click_on_the_background_opens_the_add_menu() {
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    env.vm.toggle_all_groups();
    layout(&mut tree);
    with_shipped_messages("es-ES", || {
        right_click(&mut tree, Point::new(200.0, 300.0), 1_000);
        let labels: Vec<String> = open_menu(&tree).into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec!["Agregar archivo", "Agregar carpeta"]);
    });
}

#[test]
fn hovering_the_list_is_seen() {
    let env = restored();
    let (mut tree, _, _) = mounted(&env);
    tree.pointer_move(Point::new(200.0, 100.0));
    layout(&mut tree);
    assert!(env.vm.hovered().get());
    tree.pointer_move(Point::new(900.0, 100.0));
    layout(&mut tree);
    assert!(!env.vm.hovered().get());
}

// ---------------------------------------------------------------------------
// Keys (quirk fix 2)
// ---------------------------------------------------------------------------

#[test]
fn the_keys_act_on_the_focused_list() {
    let env = restored();
    let (mut tree, _, list) = mounted(&env);
    let list = list.get().unwrap();
    key_down(&mut tree, list, Key::ArrowDown);
    key_down(&mut tree, list, Key::ArrowDown);
    assert_eq!(env.selected(), vec![1]);
    key_down_with(&mut tree, list, Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!(env.selected(), vec![1, 2]);
    key_down(&mut tree, list, Key::Space);
    assert!(!env.vm.song_at(1).unwrap().enabled && !env.vm.song_at(2).unwrap().enabled);
    key_down(&mut tree, list, Key::Enter);
    assert_eq!(env.vm.playing_index().get(), Some(2), "the focused row");
    key_down(&mut tree, list, Key::ArrowLeft);
    assert_eq!(env.rows(), vec![0, 11, 12, 13, 14, 15]);
    key_down(&mut tree, list, Key::ArrowRight);
    assert_eq!(env.rows().len(), 16);
    key_down(&mut tree, list, Key::ArrowDown);
    key_down(&mut tree, list, Key::Delete);
    env.wait(|| {
        crate::frontend::commands::playlist::load_playlist(
            &env.services.ctx,
            env.playlist_id("David Bowie"),
        )
        .recv_blocking()
        .is_ok_and(|loaded| loaded.groups[0].songs.len() == 9)
    });
    assert!(env.fakes.player.snapshot().is_playing);
}

#[test]
fn the_keys_are_left_to_an_open_menu() {
    let env = restored();
    let (mut tree, _, list) = mounted(&env);
    tree.focus(list.get().unwrap());
    right_click(&mut tree, row_point(3, 200.0), 1_000);
    tree.press_key(Key::ArrowDown, Modifiers::NONE);
    tree.press_key(Key::ArrowDown, Modifiers::NONE);
    layout(&mut tree);
    assert_eq!(env.selected(), vec![3], "the menu took the arrows");
}

#[test]
fn the_focused_row_scrolls_into_view() {
    let env = restored();
    let (mut tree, root, list) = mounted(&env);
    let list = list.get().unwrap();
    for _ in 0..16 {
        key_down(&mut tree, list, Key::ArrowDown);
    }
    assert_eq!(env.vm.selection().get().focused, Some(15));
    let _ = tree.render();
    layout(&mut tree);
    // 16 rows are 736 px: the last row's bottom meets the viewport's.
    let last = rows(&tree, root).last().unwrap().1;
    assert_eq!(last.y + last.height, 480.0);
    for _ in 0..16 {
        key_down(&mut tree, list, Key::ArrowUp);
    }
    let _ = tree.render();
    layout(&mut tree);
    let first = rows(&tree, root)
        .into_iter()
        .find(|(id, _)| {
            tree.accessibility_node(*id)
                .name()
                .is_some_and(|name| name.starts_with("FLAC (16bit-44.1kHz)"))
        })
        .unwrap();
    assert_eq!(first.1.y, 0.0, "back at the top");
}

// ---------------------------------------------------------------------------
// Empty
// ---------------------------------------------------------------------------

#[test]
fn an_empty_playlist_says_so() {
    let env = restored();
    let drift = env.playlist_id("Oxidy Drift");
    env.vm.switch_playlist(drift, false);
    env.wait(|| env.vm.active_playlist().get() == Some(drift));
    with_shipped_messages("es-ES", || {
        let (tree, root, _) = mounted(&env);
        assert!(rows(&tree, root).is_empty());
        assert!(
            tree.find_by_label("No hay canciones en esta lista")
                .is_some()
        );
    });
}
