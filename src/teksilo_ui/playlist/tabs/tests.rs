//! The tab strip over the mock library ("Audoxidy", "Oxidy Drift", "David
//! Bowie"), in the 400 px column.

use teksilo::canvas::Point;
use teksilo::core::{Key, Modifiers, PointerButton, WidgetTree};

use super::*;
use crate::frontend::commands::playlist;
use crate::teksilo_ui::playlist::testing::{Env, menus, mount_sized, open_menu, restored};
use crate::teksilo_ui::test_support::{
    access_click, find_by_name, layout, pointer_click, with_shipped_messages,
};

fn mounted(env: &Env) -> (WidgetTree, WidgetId) {
    let (tree, id, _, _) = mount_sized(env, TabsStrip::new(env.vm.clone()), (400.0, HEIGHT));
    (tree, id)
}

/// The tab named `name` (the overflow list, built under the strip, has an
/// entry of the same name).
fn tab(tree: &WidgetTree, root: WidgetId, name: &str) -> WidgetId {
    find_by_name(tree, root, name)
        .into_iter()
        .find(|id| tree.accessibility_node(*id).role() == Role::Tab)
        .unwrap()
}

fn tab_bounds(tree: &WidgetTree, root: WidgetId, name: &str) -> Rect {
    tree.bounds(tab(tree, root, name))
}

/// Adds playlists until the tabs overflow the strip.
fn overflowing(env: &Env) {
    for name in ["Una lista bastante larga", "Otra lista más larga"] {
        playlist::create_playlist(&env.services.ctx, name, Vec::new())
            .recv_blocking()
            .unwrap();
    }
    env.reload_tabs();
}

#[test]
fn the_tabs_start_12_pixels_in_5_apart_and_centred_in_40() {
    let env = restored();
    let (tree, root) = mounted(&env);
    assert_eq!(tree.bounds(root).height, HEIGHT);
    let audoxidy = tab_bounds(&tree, root, "Audoxidy");
    let drift = tab_bounds(&tree, root, "Oxidy Drift");
    let bowie = tab_bounds(&tree, root, "David Bowie");
    assert_eq!(audoxidy.x, LEADING);
    assert_eq!(drift.x - (audoxidy.x + audoxidy.width), TAB_SPACING);
    assert_eq!(bowie.x - (drift.x + drift.width), TAB_SPACING);
    assert_eq!(audoxidy.height, TAB_HEIGHT);
    assert_eq!(audoxidy.y + audoxidy.height / 2.0, HEIGHT / 2.0);
    assert!(tree.find_by_label("All playlists").is_none(), "no chevron");
}

#[test]
fn the_active_playlists_tab_is_the_selected_one_and_a_click_switches() {
    let env = restored();
    let (mut tree, root) = mounted(&env);
    let bowie = tab(&tree, root, "David Bowie");
    let audoxidy = tab(&tree, root, "Audoxidy");
    assert_eq!(tree.accessibility_node(bowie).role(), Role::Tab);
    assert!(tree.accessibility_node(bowie).is_selected());
    assert!(!tree.accessibility_node(audoxidy).is_selected());
    access_click(&mut tree, audoxidy);
    let id = env.playlist_id("Audoxidy");
    env.wait(|| env.vm.active_playlist().get() == Some(id));
    layout(&mut tree);
    let audoxidy = tab(&tree, root, "Audoxidy");
    assert!(tree.accessibility_node(audoxidy).is_selected());
}

#[test]
fn a_right_click_on_a_tab_opens_its_menu_and_on_the_strip_the_bar_menu() {
    let env = restored();
    let (mut tree, root) = mounted(&env);
    let bowie = tab_bounds(&tree, root, "David Bowie");
    with_shipped_messages("es-ES", || {
        pointer_click(&mut tree, bowie.center(), PointerButton::Secondary, 1_000);
        let labels: Vec<String> = open_menu(&tree).into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels[0], "Reproducir");
        assert_eq!(labels.len(), 7);
        tree.press_key(Key::Escape, Modifiers::NONE);
        layout(&mut tree);
        assert!(tree.active_overlays().is_empty(), "Escape closes it");
        pointer_click(
            &mut tree,
            Point::new(380.0, HEIGHT / 2.0),
            PointerButton::Secondary,
            2_000,
        );
        let labels: Vec<String> = open_menu(&tree).into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, vec!["Nueva lista", "Importar lista"]);
    });
}

#[test]
fn the_chevron_appears_once_the_tabs_overflow() {
    let env = restored();
    overflowing(&env);
    let (tree, root) = mounted(&env);
    let chevron = tree.bounds(tree.find_by_label("All playlists").unwrap());
    let side = CHEVRON_SIZE + 2.0 * CHEVRON_PADDING;
    assert_eq!((chevron.width, chevron.height), (side, side));
    assert_eq!(chevron.x + chevron.width, tree.bounds(root).width);
    assert_eq!(chevron.y + chevron.height / 2.0, HEIGHT / 2.0);
}

#[test]
fn a_new_playlist_rebuilds_the_strip() {
    let env = restored();
    let (mut tree, root) = mounted(&env);
    assert!(tree.find_by_label("All playlists").is_none());
    overflowing(&env);
    layout(&mut tree);
    assert!(!find_by_name(&tree, root, "Otra lista más larga").is_empty());
    assert!(tree.find_by_label("All playlists").is_some());
}

#[test]
fn the_chevron_opens_every_playlist_under_itself() {
    // Quirk fix 7: under the chevron, not at a fixed window position.
    let env = restored();
    overflowing(&env);
    let (mut tree, _) = mounted(&env);
    let chevron_id = tree.find_by_label("All playlists").unwrap();
    let chevron = tree.bounds(chevron_id);
    with_shipped_messages("es-ES", || {
        access_click(&mut tree, chevron_id);
        let overlay = *tree.active_overlays().last().unwrap();
        let bounds = tree.overlay_content_bounds(overlay).unwrap();
        let below = bounds.y - (chevron.y + chevron.height);
        assert!(
            (0.0..=8.0).contains(&below) && (bounds.x - chevron.x).abs() < 0.5,
            "anchored under the chevron: {bounds:?} {chevron:?}"
        );
        let labels: Vec<String> = open_menu(&tree).into_iter().map(|(l, _)| l).collect();
        assert_eq!(
            labels,
            vec![
                "Nueva lista",
                "Audoxidy",
                "Oxidy Drift",
                "David Bowie",
                "Una lista bastante larga",
                "Otra lista más larga"
            ]
        );
        tree.press_key(Key::Escape, Modifiers::NONE);
        layout(&mut tree);
        assert!(tree.active_overlays().is_empty(), "Escape closes it");
    });
}

#[test]
fn a_tab_from_the_overflow_list_switches_and_scrolls_into_view() {
    let env = restored();
    overflowing(&env);
    let (mut tree, root) = mounted(&env);
    let chevron = tree.find_by_label("All playlists").unwrap();
    access_click(&mut tree, chevron);
    let menu = *menus(&tree).last().unwrap();
    let entry = find_by_name(&tree, menu, "Otra lista más larga")[0];
    tree.click(entry);
    layout(&mut tree);
    let id = env.vm.tab_list().last().unwrap().id;
    env.wait(|| env.vm.active_playlist().get() == Some(id));
    layout(&mut tree);
    let _ = tree.render();
    layout(&mut tree);
    let tab = tab_bounds(&tree, root, "Otra lista más larga");
    let chevron = tree.bounds(chevron);
    assert!(
        tab.x + tab.width <= chevron.x,
        "the active tab is in view: {tab:?}"
    );
    assert!(tab.x >= 0.0);
}

#[test]
fn the_strip_reads_in_spanish() {
    let env = restored();
    overflowing(&env);
    with_shipped_messages("es-ES", || {
        let (tree, root) = mounted(&env);
        assert!(tree.find_by_label("Todas las listas").is_some());
        let tabs = tree.find_by_role(Role::TabList).unwrap();
        assert!(tree.accessibility_node(root).role() != Role::TabList);
        assert_eq!(
            tree.accessibility_node(tabs).name(),
            Some("Listas de reproducción")
        );
    });
}
