use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::PointerPhase;
use teksilo::core::WidgetTree;
use teksilo::core::environment::LayoutDirection;
use teksilo::tokens::Alignment;
use teksilo::widgets::Spacer;

use super::*;
use crate::teksilo_ui::app::services::AppServices;
use crate::teksilo_ui::audio_center::shell::testing::{find_by_name, services_with_fakes};
use crate::teksilo_ui::test_support::{
    VIEWPORT, access_click, first_of_type, key_down, layout, pointer_event, tap_at,
    tree_with_events, with_shipped_messages,
};

/// The panel as the app mounts it: a layer in a stack over the main layout,
/// which here is one big button counting the taps that reach it.
struct Fixture {
    services: AppServices,
    vm: AudioCenterViewModel,
    tree: WidgetTree,
    main_taps: Rc<Cell<u32>>,
}

/// A tab body: a 100 × 20 button named after the tab, at the body's
/// top-leading corner.
fn body(name: &'static str) -> Box<dyn Widget> {
    Box::new(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(Pressable::new(
                FixedSize::new()
                    .width(100.0)
                    .height(20.0)
                    .child(Spacer::new()),
                lit!(name),
            )),
    )
}

fn fixture() -> Fixture {
    let (services, _fakes) = services_with_fakes();
    let vm = AudioCenterViewModel::new(&services);
    let mut tree = tree_with_events(&services.ctx);
    let main_taps = Rc::new(Cell::new(0));
    let taps = main_taps.clone();
    let main = Pressable::new(
        FixedSize::new()
            .width(VIEWPORT.0)
            .height(VIEWPORT.1)
            .child(Spacer::new()),
        lit!("main layout"),
    )
    .on_activate(move |_| taps.set(taps.get() + 1));
    let panel = AudioCenterPanel::new(
        vm.clone(),
        body("config body"),
        body("equalizer body"),
        body("effects body"),
        body("volume body"),
    );
    tree.add(
        ZStack::new()
            .alignment(Alignment::TOP_LEADING)
            .child(Expand::new().child(main))
            .child(panel),
    );
    layout(&mut tree);
    Fixture {
        services,
        vm,
        tree,
        main_taps,
    }
}

impl Fixture {
    /// Opens the panel on `tab`.
    fn open(&mut self, tab: usize) {
        self.services.links.audio_center.toggle(Some(tab));
        layout(&mut self.tree);
    }

    /// The panel's frame.
    fn frame(&self) -> WidgetId {
        self.tree
            .find_by_role(Role::Dialog)
            .expect("the frame is the dialog")
    }

    fn named(&mut self, name: &str) -> WidgetId {
        let frame = self.frame();
        let found = find_by_name(&mut self.tree, frame, name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    }

    fn find(&mut self, name: &str) -> Vec<WidgetId> {
        let frame = self.frame();
        find_by_name(&mut self.tree, frame, name)
    }

    fn bounds_of(&mut self, name: &str) -> Rect {
        let id = self.named(name);
        self.tree.bounds(id)
    }

    fn bounds(&self, id: WidgetId) -> Rect {
        self.tree.bounds(id)
    }
}

fn centre(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

// ---- geometry ----

#[test]
fn the_panel_is_940_by_480_and_centred_in_the_window() {
    let mut f = fixture();
    f.open(0);
    let frame = f.bounds(f.frame());
    assert_eq!((frame.width, frame.height), (940.0, 480.0));
    assert_eq!(
        (frame.x, frame.y),
        ((VIEWPORT.0 - 940.0) / 2.0, (VIEWPORT.1 - 480.0) / 2.0)
    );
    assert!(f.services.ctx.shutdown());
}

#[test]
fn the_header_is_40_high_with_the_logo_title_and_close_icon() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let frame = f.bounds(f.frame());
        let close = f.bounds_of("Close");
        assert_eq!(
            (close.width, close.height),
            (
                CLOSE_SIZE + 2.0 * CLOSE_PADDING,
                CLOSE_SIZE + 2.0 * CLOSE_PADDING
            ),
            "a 24 px icon with 4 px around it"
        );
        assert_eq!(
            close.x + close.width,
            frame.x + frame.width - BORDER - HEADER_TRAILING,
            "6 px from the trailing edge, inside the 2 px frame"
        );
        assert_eq!(
            close.y,
            frame.y + BORDER + (HEADER_HEIGHT - close.height) / 2.0,
            "centred in the 40 px header"
        );
        let logo = f.bounds(first_of_type(&f.tree, f.frame(), "BrandLogo").unwrap());
        assert_eq!(logo.x, frame.x + BORDER + HEADER_LEADING);
        assert!(
            logo.y >= frame.y + BORDER + LOGO_TOP,
            "9 px of space above the logo"
        );
        // The title is centred on the panel.
        let title = f.bounds(first_of_type(&f.tree, f.frame(), "TextWidget").unwrap());
        let middle = title.x + title.width / 2.0;
        assert!(
            (middle - (frame.x + frame.width / 2.0)).abs() < 1.0,
            "{title:?}"
        );
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_four_tabs_are_10_pixels_apart_under_the_header() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let frame = f.bounds(f.frame());
        let tabs: Vec<Rect> = [
            "Audio Configuration",
            "Equalizer",
            "Audio Effects",
            "Volume and Mix",
        ]
        .into_iter()
        .map(|name| f.bounds_of(name))
        .collect();
        assert_eq!(tabs[0].x, frame.x + BORDER + BODY_PADDING, "15 px in");
        assert_eq!(tabs[0].y, frame.y + BORDER + HEADER_HEIGHT);
        for pair in tabs.windows(2) {
            assert_eq!(pair[1].x, pair[0].x + pair[0].width + TAB_SPACING);
            assert_eq!(pair[1].y, pair[0].y);
        }
        // Padding of 5 px above and below and 15 px at the sides around a 14 px label.
        let equalizer = f.named("Equalizer");
        let label = f.bounds(first_of_type(&f.tree, equalizer, "TextWidget").unwrap());
        assert_eq!(tabs[1].width, label.width + 2.0 * TAB_PADDING.1);
        assert_eq!(tabs[1].height, label.height + 2.0 * TAB_PADDING.0);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_body_starts_below_the_two_pixel_line_under_the_tabs() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let frame = f.bounds(f.frame());
        let tab = f.bounds_of("Audio Configuration");
        let page = f.bounds_of("config body");
        assert_eq!(page.x, tab.x);
        assert_eq!(page.y, tab.y + tab.height + DIVIDER_HEIGHT);
        // The switcher fills what is left: 15 px at the sides and below.
        let switcher = f.bounds(first_of_type(&f.tree, f.frame(), "Switcher").unwrap());
        assert_eq!(switcher.x, frame.x + BORDER + BODY_PADDING);
        assert_eq!(switcher.width, 940.0 - 2.0 * BORDER - 2.0 * BODY_PADDING);
        assert_eq!(
            switcher.y + switcher.height,
            frame.y + 480.0 - BORDER - BODY_PADDING
        );
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_dragged_panel_is_drawn_at_its_position() {
    let mut f = fixture();
    f.open(0);
    f.vm.position().set(Some((100.0, 50.0)));
    layout(&mut f.tree);
    let frame = f.bounds(f.frame());
    assert_eq!((frame.x, frame.y), (100.0, 50.0));
    assert!(f.services.ctx.shutdown());
}

#[test]
fn a_right_to_left_layout_counts_the_position_from_the_trailing_edge() {
    let mut f = fixture();
    f.tree.set_layout_direction(LayoutDirection::RightToLeft);
    f.open(0);
    f.vm.position().set(Some((100.0, 50.0)));
    layout(&mut f.tree);
    let frame = f.bounds(f.frame());
    assert_eq!(frame.x, VIEWPORT.0 - 100.0 - 940.0);
    assert_eq!(frame.y, 50.0);
    assert!(f.services.ctx.shutdown());
}

// ---- showing and hiding ----

#[test]
fn the_panel_is_not_in_the_accessibility_tree_while_closed() {
    let mut f = fixture();
    assert!(f.tree.find_by_role(Role::Dialog).is_none());
    f.open(0);
    assert!(f.tree.find_by_role(Role::Dialog).is_some());
    f.vm.close();
    layout(&mut f.tree);
    assert!(f.tree.find_by_role(Role::Dialog).is_none());
    assert!(f.services.ctx.shutdown());
}

#[test]
fn the_selected_tab_shows_its_body_and_only_that_one() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        assert_eq!(f.find("config body").len(), 1);
        assert!(f.find("equalizer body").is_empty());
        let tab = f.named("Audio Effects");
        access_click(&mut f.tree, tab);
        assert_eq!(f.vm.tab().get(), audio_center_tab::EFFECTS);
        assert!(f.find("config body").is_empty());
        assert_eq!(f.find("effects body").len(), 1);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_player_menu_opens_the_panel_on_a_tab_and_toggles_it_closed() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(audio_center_tab::VOLUME_MIX);
        assert_eq!(f.find("volume body").len(), 1);
        f.services
            .links
            .audio_center
            .toggle(Some(audio_center_tab::VOLUME_MIX));
        layout(&mut f.tree);
        assert!(f.tree.find_by_role(Role::Dialog).is_none());
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_tabs_are_announced_as_tabs_with_the_selected_one_marked() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(audio_center_tab::EQUALIZER);
        let equalizer = f.named("Equalizer");
        let selected = f.tree.accessibility_node(equalizer);
        assert_eq!(selected.role(), Role::Tab);
        assert!(selected.is_selected());
        let effects = f.named("Audio Effects");
        let other = f.tree.accessibility_node(effects);
        assert_eq!(other.role(), Role::Tab);
        assert!(!other.is_selected());
        assert!(f.services.ctx.shutdown());
    });
}

// ---- the pointer ----

#[test]
fn the_main_layout_stays_usable_beside_the_panel() {
    let mut f = fixture();
    f.open(0);
    tap_at(&mut f.tree, Point::new(20.0, 20.0));
    assert_eq!(
        f.main_taps.get(),
        1,
        "a tap beside the panel reaches the layout"
    );
    let frame = f.bounds(f.frame());
    // A point of the panel that holds no control: well below the body's button.
    tap_at(&mut f.tree, Point::new(frame.x + 600.0, frame.y + 400.0));
    assert_eq!(f.main_taps.get(), 1, "the panel does not let taps through");
    assert!(f.services.ctx.shutdown());
}

#[test]
fn dragging_the_header_moves_the_panel_and_a_release_ends_it() {
    let mut f = fixture();
    f.open(0);
    let frame = f.bounds(f.frame());
    // The middle of the header's left half: nothing but the header there.
    let grab = Point::new(frame.x + 300.0, frame.y + 20.0);
    pointer_event(
        &mut f.tree,
        PointerPhase::Down,
        grab,
        PointerButton::Primary,
        0,
    );
    assert!(f.vm.is_dragging());
    pointer_event(
        &mut f.tree,
        PointerPhase::Move,
        Point::new(grab.x + 50.0, grab.y + 30.0),
        PointerButton::Primary,
        20,
    );
    // Iced centres the first drag on a height of 474 (see the view-model).
    assert_eq!(
        f.vm.position().get(),
        Some((
            (VIEWPORT.0 - 940.0) / 2.0 + 50.0,
            (VIEWPORT.1 - 474.0) / 2.0 + 30.0
        ))
    );
    let moved = f.bounds(f.frame());
    assert_eq!(moved.x, frame.x + 50.0);
    pointer_event(
        &mut f.tree,
        PointerPhase::Up,
        Point::new(grab.x + 50.0, grab.y + 30.0),
        PointerButton::Primary,
        40,
    );
    assert!(!f.vm.is_dragging());
    pointer_event(
        &mut f.tree,
        PointerPhase::Move,
        Point::new(grab.x + 300.0, grab.y + 300.0),
        PointerButton::Primary,
        60,
    );
    assert_eq!(
        f.bounds(f.frame()).x,
        frame.x + 50.0,
        "no drag after the release"
    );
    assert!(f.services.ctx.shutdown());
}

#[test]
fn dragging_far_outside_the_window_stops_at_the_clamp() {
    let mut f = fixture();
    f.open(0);
    let frame = f.bounds(f.frame());
    let grab = Point::new(frame.x + 300.0, frame.y + 20.0);
    pointer_event(
        &mut f.tree,
        PointerPhase::Down,
        grab,
        PointerButton::Primary,
        0,
    );
    pointer_event(
        &mut f.tree,
        PointerPhase::Move,
        Point::new(grab.x + 5000.0, grab.y + 5000.0),
        PointerButton::Primary,
        20,
    );
    assert_eq!(
        f.vm.position().get(),
        Some((VIEWPORT.0 - 100.0, VIEWPORT.1 - 40.0))
    );
    assert!(f.services.ctx.shutdown());
}

#[test]
fn a_right_to_left_drag_follows_the_pointer() {
    let mut f = fixture();
    f.tree.set_layout_direction(LayoutDirection::RightToLeft);
    f.open(0);
    let frame = f.bounds(f.frame());
    let grab = Point::new(frame.x + 300.0, frame.y + 20.0);
    pointer_event(
        &mut f.tree,
        PointerPhase::Down,
        grab,
        PointerButton::Primary,
        0,
    );
    pointer_event(
        &mut f.tree,
        PointerPhase::Move,
        Point::new(grab.x + 40.0, grab.y),
        PointerButton::Primary,
        20,
    );
    assert_eq!(
        f.bounds(f.frame()).x,
        frame.x + 40.0,
        "the panel moves right with the pointer in a right-to-left layout"
    );
    assert!(f.services.ctx.shutdown());
}

#[test]
fn a_press_on_the_close_icon_closes_instead_of_dragging() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let close = f.bounds_of("Close");
        tap_at(&mut f.tree, centre(close));
        assert!(!f.vm.open().get());
        assert!(!f.vm.is_dragging());
        assert!(f.services.ctx.shutdown());
    });
}

// ---- the keyboard ----

#[test]
fn the_close_button_takes_the_focus_when_the_panel_opens() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let close = f.named("Close");
        assert_eq!(f.tree.focused(), Some(close));
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn escape_closes_the_panel() {
    // Quirk fix 4 (Audio Center part): Iced's Audio Center ignores Escape.
    let mut f = fixture();
    f.open(0);
    f.vm.position().set(Some((40.0, 40.0)));
    let frame = f.frame();
    key_down(&mut f.tree, frame, Key::Escape);
    assert!(!f.vm.open().get());
    assert_eq!(f.vm.position().get(), None);
    assert!(f.services.ctx.shutdown());
}

#[test]
fn escape_from_a_control_inside_a_tab_closes_the_panel_too() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let button = f.named("config body");
        key_down(&mut f.tree, button, Key::Escape);
        assert!(!f.vm.open().get());
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn escape_does_nothing_while_the_panel_is_closed() {
    let mut f = fixture();
    f.open(0);
    f.vm.close();
    layout(&mut f.tree);
    f.tree.dispatch_event(WidgetEvent::KeyDown {
        key: Key::Escape,
        modifiers: teksilo::core::Modifiers::NONE,
        text: None,
    });
    assert!(!f.vm.open().get());
    assert!(f.services.ctx.shutdown());
}

#[test]
fn the_close_button_closes_with_enter_and_the_click_action() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.open(0);
        let close = f.named("Close");
        access_click(&mut f.tree, close);
        assert!(!f.vm.open().get());
        f.open(0);
        let close = f.named("Close");
        key_down(&mut f.tree, close, Key::Enter);
        assert!(!f.vm.open().get());
        assert!(f.services.ctx.shutdown());
    });
}

// ---- Spanish ----

#[test]
fn the_spanish_strings_are_the_iced_ones() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture();
        f.open(0);
        let names: Vec<&str> = vec![
            "Configuración de Audio",
            "Ecualizador",
            "Efectos de Audio",
            "Volumen y Mezcla",
            "Cerrar",
        ];
        for name in names {
            assert_eq!(f.find(name).len(), 1, "{name}");
        }
        let frame = f.tree.accessibility_node(f.frame());
        assert_eq!(frame.name(), Some("Centro de Audio Avanzado"));
        assert!(f.services.ctx.shutdown());
    });
}
