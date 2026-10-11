use teksilo::core::WidgetTree;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::style;
use crate::teksilo_ui::test_support::{first_of_type, layout};

/// A `side`-pixel opaque grey cover.
fn cover(side: u32) -> SharedCover {
    Rc::new(RgbaCover {
        pixels: vec![128_u8; (side * side * 4) as usize].into(),
        width: side,
        height: side,
    })
}

fn mounted(art: AlbumArt) -> (WidgetTree, WidgetId) {
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let id = tree.add(art);
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

fn size_of(tree: &WidgetTree, id: WidgetId) -> (f32, f32) {
    let bounds = tree.bounds(id);
    (bounds.width, bounds.height)
}

#[test]
fn a_cover_fills_the_square_and_shows_no_placeholder() {
    let (tree, id) = mounted(
        AlbumArt::new(
            Some(cover(64)),
            Placeholder::Large(icons::window::close),
            152.0,
        )
        .radius(8.0),
    );
    assert_eq!(size_of(&tree, id), (152.0, 152.0));
    let image = first_of_type(&tree, id, "ImageWidget").unwrap();
    assert_eq!(size_of(&tree, image), (152.0, 152.0));
    assert!(first_of_type(&tree, id, "IconWidget").is_none());
}

#[test]
fn the_large_placeholder_centres_the_96_pixel_icon_over_the_logo() {
    let (tree, id) = mounted(AlbumArt::new(
        None::<SharedCover>,
        Placeholder::Large(icons::window::close),
        152.0,
    ));
    assert_eq!(size_of(&tree, id), (152.0, 152.0));
    let icon = first_of_type(&tree, id, "IconWidget").unwrap();
    assert_eq!(size_of(&tree, icon), (LARGE_ICON_SIZE, LARGE_ICON_SIZE));
    let art = tree.bounds(id);
    let icon = tree.bounds(icon);
    assert_eq!(icon.x - art.x, (152.0 - LARGE_ICON_SIZE) / 2.0);
    let logo = first_of_type(&tree, id, "BrandLogo").unwrap();
    assert_eq!(tree.bounds(logo).y - (icon.y + icon.height), 5.0);
    assert!(first_of_type(&tree, id, "ImageWidget").is_none());
}

#[test]
fn the_small_placeholder_centres_the_20_pixel_icon() {
    let (tree, id) = mounted(AlbumArt::new(
        None::<SharedCover>,
        Placeholder::Small(icons::window::close),
        30.0,
    ));
    let icon = first_of_type(&tree, id, "IconWidget").unwrap();
    let art = tree.bounds(id);
    let icon = tree.bounds(icon);
    assert_eq!(
        (icon.width, icon.height),
        (SMALL_ICON_SIZE, SMALL_ICON_SIZE)
    );
    assert_eq!(icon.x - art.x, 5.0);
    assert_eq!(icon.y - art.y, 5.0);
}

#[test]
fn the_player_placeholder_is_the_logo_alone() {
    let (tree, id) = mounted(AlbumArt::new(
        None::<SharedCover>,
        Placeholder::Player,
        400.0,
    ));
    assert_eq!(size_of(&tree, id), (400.0, 400.0));
    assert!(first_of_type(&tree, id, "BrandLogo").is_some());
    assert!(first_of_type(&tree, id, "IconWidget").is_none());
}

#[test]
fn a_bound_cover_replaces_the_placeholder_when_it_arrives() {
    let source: Signal<Option<SharedCover>> = Signal::new(None);
    let (mut tree, id) = mounted(AlbumArt::new(
        source.clone(),
        Placeholder::Small(icons::window::close),
        30.0,
    ));
    assert!(first_of_type(&tree, id, "ImageWidget").is_none());
    source.set(Some(cover(30)));
    layout(&mut tree);
    let _ = tree.render();
    assert!(first_of_type(&tree, id, "ImageWidget").is_some());
    assert_eq!(size_of(&tree, id), (30.0, 30.0));
    source.set(None);
    layout(&mut tree);
    assert!(first_of_type(&tree, id, "ImageWidget").is_none());
}

#[test]
fn the_radius_becomes_a_fraction_of_the_side() {
    assert_eq!(mask_fraction(8.0, 160.0), 0.05);
    assert_eq!(mask_fraction(0.0, 152.0), 0.0);
    assert_eq!(mask_fraction(100.0, 40.0), 0.5);
    assert_eq!(mask_fraction(8.0, 0.0), 0.0);
}

#[test]
fn the_rgba_cover_reports_its_pixels() {
    let art = cover(2);
    assert_eq!((art.width(), art.height()), (2, 2));
    assert_eq!(art.rgba().len(), 16);
}
