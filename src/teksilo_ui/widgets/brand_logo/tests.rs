use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::teksilo_ui::test_support::layout;

fn mounted(logo: BrandLogo) -> (WidgetTree, WidgetId) {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let id = tree.add(logo);
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(id)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    (tree, id)
}

/// The type names of every node under `id`.
fn texts_under(tree: &WidgetTree, id: WidgetId) -> usize {
    let mut count = 0;
    let mut stack = vec![id];
    while let Some(node) = stack.pop() {
        if tree
            .widget_type_name(node)
            .is_some_and(|name| name.ends_with("TextWidget"))
        {
            count += 1;
        }
        stack.extend(tree.children(node));
    }
    count
}

#[test]
fn the_two_tone_runs_spell_the_logo_with_the_mark_on_a_o_and_y() {
    let text: String = TWO_TONE_RUNS.iter().map(|(run, _)| *run).collect();
    assert_eq!(text, LOGO_TEXT);
    let marked: String = TWO_TONE_RUNS
        .iter()
        .filter(|(_, mark)| *mark)
        .map(|(run, _)| *run)
        .collect();
    assert_eq!(marked, "AoY");
}

#[test]
fn a_one_colour_logo_is_one_text_and_a_two_tone_logo_five() {
    let (tree, id) = mounted(BrandLogo::new(19.0));
    assert_eq!(texts_under(&tree, id), 1);
    let (tree, id) = mounted(BrandLogo::new(18.0).two_tone(TextRole::Accent));
    assert_eq!(texts_under(&tree, id), 5);
}

#[test]
fn it_is_one_image_named_audoxidy() {
    let (mut tree, _) = mounted(BrandLogo::new(18.0).two_tone(TextRole::Accent));
    let image = tree.find_by_role(Role::Image).unwrap();
    assert_eq!(tree.accessibility_node(image).name(), Some("Audoxidy"));
    // The runs are pruned from what a screen reader gets.
    let update = tree.sync_accessibility();
    let labels: Vec<String> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().map(|label| label.to_string()))
        .collect();
    assert!(labels.iter().any(|label| label == "Audoxidy"), "{labels:?}");
    assert!(!labels.iter().any(|label| label == "uD"), "{labels:?}");
}
