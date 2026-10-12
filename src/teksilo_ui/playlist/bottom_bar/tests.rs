//! The bottom bar over the mock library, in the 400 px column.

use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;

use super::*;
use crate::teksilo_ui::app::audio_center_tab;
use crate::teksilo_ui::playlist::testing::{Env, mount_sized, restored};
use crate::teksilo_ui::test_support::{access_click, layout, type_text, with_shipped_messages};

fn mounted(env: &Env) -> (WidgetTree, WidgetId) {
    let (tree, id, _, _) = mount_sized(env, BottomBar::new(env.vm.clone()), (400.0, HEIGHT));
    (tree, id)
}

fn click(tree: &mut WidgetTree, label: &str) {
    let id = tree.find_by_label(label).unwrap();
    access_click(tree, id);
}

fn bounds(tree: &WidgetTree, label: &str) -> Rect {
    tree.bounds(tree.find_by_label(label).unwrap())
}

#[test]
fn the_field_and_the_icons_sit_where_iced_puts_them() {
    let env = restored();
    let (tree, root) = mounted(&env);
    let bar = tree.bounds(root);
    assert_eq!(bar.height, HEIGHT);
    let field = tree.find_by_role(Role::TextInput).unwrap();
    let field = tree.bounds(field);
    assert_eq!(field.x, SIDE_PADDING);
    let side = ICON_SIZE + 2.0 * ICON_PADDING;
    let equalizer = bounds(&tree, "Equalizer");
    let shuffle = bounds(&tree, "Shuffle");
    let repeat = bounds(&tree, "Repeat: off");
    let lyrics = bounds(&tree, "Lyrics");
    assert_eq!((equalizer.width, equalizer.height), (side, side));
    assert_eq!(equalizer.x + equalizer.width, bar.width - SIDE_PADDING);
    assert_eq!(equalizer.x - (shuffle.x + shuffle.width), SPACING);
    assert_eq!(shuffle.x - (repeat.x + repeat.width), SPACING);
    assert_eq!(repeat.x - (lyrics.x + lyrics.width), SPACING);
    assert_eq!(repeat.y + side / 2.0, HEIGHT / 2.0);
    assert_eq!(
        lyrics.y - repeat.y,
        LYRICS_DROP / 2.0,
        "3 px lower, centred"
    );
}

#[test]
fn the_lyrics_icon_is_a_disabled_stub() {
    let env = restored();
    let (mut tree, _) = mounted(&env);
    let lyrics = tree.find_by_label("Lyrics").unwrap();
    assert!(tree.accessibility_node(lyrics).is_disabled());
    access_click(&mut tree, lyrics);
}

#[test]
fn repeat_cycles_its_mode_and_its_name() {
    let env = restored();
    let (mut tree, _) = mounted(&env);
    click(&mut tree, "Repeat: off");
    assert_eq!(env.vm.repeat().get(), RepeatMode::All);
    click(&mut tree, "Repeat: all");
    assert_eq!(env.vm.repeat().get(), RepeatMode::One);
    click(&mut tree, "Repeat: one");
    assert_eq!(env.vm.repeat().get(), RepeatMode::Off);
}

#[test]
fn shuffle_and_the_equalizer_do_what_iced_does() {
    let env = restored();
    let (mut tree, _) = mounted(&env);
    click(&mut tree, "Shuffle");
    assert!(env.vm.shuffle().get());
    click(&mut tree, "Equalizer");
    let link = &env.services.links.audio_center;
    assert!(link.open.get());
    assert_eq!(link.tab.get(), audio_center_tab::EQUALIZER);
}

#[test]
fn an_icon_is_lit_while_its_mode_is_on() {
    assert_eq!(icon_role(true), TextRole::Accent);
    assert_eq!(icon_role(false), TextRole::Secondary);
}

#[test]
fn typing_in_the_field_filters_the_rows() {
    let env = restored();
    let (mut tree, _) = mounted(&env);
    let field = tree.find_by_role(Role::TextInput).unwrap();
    tree.focus(field);
    type_text(&mut tree, "2017");
    layout(&mut tree);
    assert_eq!(env.rows(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn the_bar_reads_in_spanish() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        let (tree, _) = mounted(&env);
        for name in [
            "Buscar en la lista",
            "Lyrics",
            "Repetir: desactivado",
            "Aleatorio",
            "Ecualizador",
        ] {
            assert!(tree.find_by_label(name).is_some(), "{name}");
        }
    });
}

#[test]
fn the_search_field_has_its_own_name_not_the_filter_fields() {
    // Review 13a: named by its placeholder, it read "Search..." like the
    // filter column's field.
    let env = restored();
    let (tree, _) = mounted(&env);
    let field = tree.find_by_role(Role::TextInput).unwrap();
    assert_eq!(
        tree.accessibility_node(field).name(),
        Some("Search the playlist")
    );
    with_shipped_messages("es-ES", || {
        let (tree, _) = mounted(&env);
        let field = tree.find_by_role(Role::TextInput).unwrap();
        assert_eq!(
            tree.accessibility_node(field).name(),
            Some("Buscar en la lista")
        );
    });
}
