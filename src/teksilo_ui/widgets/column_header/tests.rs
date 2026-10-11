use std::cell::RefCell;

use teksilo::core::{Modifiers, WidgetTree};
use teksilo::widgets::{Spacer, VStack};

use super::*;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::test_support::{
    access_click, find_by_name, key_down, key_down_with, layout, with_shipped_messages,
};

const ARROWS: SortArrows = SortArrows {
    up: icons::window::maximize,
    down: icons::window::minimize,
};

/// A mounted header and what its callbacks received.
struct Harness {
    tree: WidgetTree,
    header: WidgetId,
    widths: Signal<ColumnWidths>,
    sort: Signal<SortState>,
    sorts: Rc<RefCell<Vec<(SortColumn, SortState)>>>,
}

/// The header of `mode` at the top of the window. Its callbacks store what
/// they get into the signals, as the view-model does.
fn harness(mode: LibraryViewMode) -> Harness {
    let widths = Signal::new(default_widths());
    let sort = Signal::new(SortState::default());
    let sorts = Rc::new(RefCell::new(Vec::new()));
    let (store_widths, store_sort, log) = (widths.clone(), sort.clone(), sorts.clone());
    let mut tree = WidgetTree::new();
    tree.set_theme(style::audoxidy_dark());
    let header = tree.add(
        ColumnHeader::new(columns_for(mode), widths.clone(), sort.clone(), ARROWS)
            .leading_padding(leading_padding(mode))
            .on_sort(move |column, next, _| {
                log.borrow_mut().push((column, next));
                store_sort.set(next);
            })
            .on_resize(move |column, width, _| {
                let mut all = store_widths.get();
                all.insert(column, width);
                store_widths.set(all);
            }),
    );
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(header)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    Harness {
        tree,
        header,
        widths,
        sort,
        sorts,
    }
}

impl Harness {
    /// The column header named `name` (its full label).
    fn cell(&self, name: &str) -> WidgetId {
        find_by_name(&self.tree, self.header, name)
            .into_iter()
            .find(|id| self.tree.accessibility_node(*id).role() == Role::ColumnHeader)
            .unwrap()
    }

    /// The grip of the column named `column`.
    fn grip(&self, column: &str) -> WidgetId {
        find_by_name(&self.tree, self.header, &format!("Resize {column}"))
            .into_iter()
            .chain(find_by_name(
                &self.tree,
                self.header,
                &format!("Resize \u{2068}{column}\u{2069}"),
            ))
            .next()
            .unwrap()
    }

    fn relayout(&mut self) {
        layout(&mut self.tree);
        let _ = self.tree.render();
    }
}

#[test]
fn the_default_widths_are_the_iced_ones_and_cover_every_column() {
    let widths = default_widths();
    assert_eq!(widths.len(), 16);
    assert_eq!(widths[&SortColumn::Title], 275);
    assert_eq!(widths[&SortColumn::TrackNumber], 47);
    assert_eq!(widths[&SortColumn::AlbumThumbnail], 40);
    assert_eq!(
        width_of(&ColumnWidths::new(), SortColumn::Year),
        DEFAULT_WIDTH
    );
}

#[test]
fn each_mode_has_the_iced_columns_and_leading_padding() {
    let detailed = columns_for(LibraryViewMode::DetailedList);
    assert_eq!(detailed.first(), Some(&SortColumn::AlbumCard));
    assert_eq!(detailed.len(), 12);
    let thumbnail = columns_for(LibraryViewMode::ThumbnailList);
    assert_eq!(thumbnail.first(), Some(&SortColumn::AlbumThumbnail));
    assert_eq!(thumbnail.len(), 15);
    let simple = columns_for(LibraryViewMode::SimpleList);
    assert_eq!(simple, columns_for(LibraryViewMode::Grid));
    assert_eq!(simple.len(), 14);
    for columns in [detailed, thumbnail, simple] {
        assert_eq!(columns.last(), Some(&SortColumn::Size));
    }
    assert_eq!(leading_padding(LibraryViewMode::Grid), 19.0);
    assert_eq!(leading_padding(LibraryViewMode::DetailedList), 29.0);
    assert_eq!(leading_padding(LibraryViewMode::ThumbnailList), 31.0);
    assert_eq!(leading_padding(LibraryViewMode::SimpleList), 29.0);
}

#[test]
fn labels_are_cut_to_the_characters_the_column_fits() {
    assert_eq!(truncated_label("Duración", 53.0), "Du...");
    assert_eq!(truncated_label("Formato", 53.0), "Fo...");
    assert_eq!(truncated_label("Profundidad", 60.0), "Pro...");
    assert_eq!(truncated_label("Título", 275.0), "Título");
    assert_eq!(truncated_label("#", 47.0), "#");
}

#[test]
fn a_drag_never_makes_a_column_narrower_than_30() {
    assert_eq!(resized_width(275, 10.6), 285);
    assert_eq!(resized_width(100, -100.0), MIN_WIDTH);
    assert_eq!(stepped_width(275, 10), 285);
    assert_eq!(stepped_width(35, -10), MIN_WIDTH);
}

#[test]
fn the_bar_is_28_pixels_over_a_2_pixel_rule() {
    let h = harness(LibraryViewMode::SimpleList);
    assert_eq!(h.tree.bounds(h.header).height, HEIGHT + RULE_HEIGHT);
}

#[test]
fn columns_start_after_the_leading_padding_at_their_widths() {
    with_shipped_messages("en-US", || {
        let h = harness(LibraryViewMode::SimpleList);
        let origin = h.tree.bounds(h.header).x;
        let track = h.cell("#");
        let title = h.cell("Title");
        // The sort button starts the column.
        assert_eq!(h.tree.bounds(track).x - origin, 29.0);
        assert_eq!(h.tree.bounds(title).x - origin, 29.0 + 47.0);
        assert_eq!(h.tree.bounds(title).width, 275.0 - 15.0);
        let grip = h.tree.bounds(h.grip("Title"));
        assert_eq!((grip.width, grip.height), (GRIP_WIDTH, GRIP_HEIGHT));
        assert_eq!(
            grip.x - origin,
            29.0 + 47.0 + 275.0 - GRIP_SPACE - GRIP_WIDTH
        );
        let bar_center = h.tree.bounds(h.header).y + HEIGHT / 2.0;
        assert_eq!(grip.center().y, bar_center);
    });
}

#[test]
fn the_album_columns_are_inert_and_have_no_grip() {
    with_shipped_messages("en-US", || {
        let h = harness(LibraryViewMode::DetailedList);
        let card = h.cell("Album Card");
        assert!(
            !h.tree
                .accessibility_node(card)
                .actions()
                .contains(&accesskit::Action::Click)
        );
        assert!(find_by_name(&h.tree, h.header, "Resize Album Card").is_empty());
        let h = harness(LibraryViewMode::ThumbnailList);
        let cover = h.cell("Cover");
        assert_eq!(h.tree.bounds(cover).width, 40.0 - 10.0);
    });
}

#[test]
fn a_click_steps_the_sort_through_its_three_states() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let title = h.cell("Title");
        access_click(&mut h.tree, title);
        access_click(&mut h.tree, title);
        access_click(&mut h.tree, title);
        let sorts = h.sorts.borrow().clone();
        assert_eq!(
            sorts,
            vec![
                (
                    SortColumn::Title,
                    SortState {
                        column: Some(SortColumn::Title),
                        ascending: Some(true)
                    }
                ),
                (
                    SortColumn::Title,
                    SortState {
                        column: Some(SortColumn::Title),
                        ascending: Some(false)
                    }
                ),
                (
                    SortColumn::Title,
                    SortState {
                        column: None,
                        ascending: None
                    }
                ),
            ]
        );
    });
}

#[test]
fn enter_and_space_sort_a_focused_column() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let artist = h.cell("Artist");
        key_down(&mut h.tree, artist, Key::Enter);
        key_down(&mut h.tree, artist, Key::Space);
        assert_eq!(h.sorts.borrow().len(), 2);
        assert_eq!(h.sort.get().ascending, Some(false));
    });
}

#[test]
fn only_the_sorted_column_shows_its_direction() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let title = h.cell("Title");
        let arrows = |h: &Harness, cell: WidgetId| -> Vec<bool> {
            let mut shown = Vec::new();
            let mut stack = vec![cell];
            while let Some(node) = stack.pop() {
                if h.tree
                    .widget_type_name(node)
                    .is_some_and(|name| name.ends_with("IconWidget"))
                {
                    shown.push(h.tree.is_visible(node));
                }
                stack.extend(h.tree.children(node));
            }
            shown
        };
        assert_eq!(arrows(&h, title), vec![false, false]);
        h.sort.set(SortState {
            column: Some(SortColumn::Title),
            ascending: Some(false),
        });
        h.relayout();
        assert_eq!(arrows(&h, title).iter().filter(|shown| **shown).count(), 1);
        assert_eq!(arrows(&h, h.cell("Artist")), vec![false, false]);
    });
}

#[test]
fn the_sorted_column_tells_its_direction_to_screen_readers() {
    assert_eq!(
        sort_direction(&SortState::default(), SortColumn::Title),
        None
    );
    let sorted = SortState {
        column: Some(SortColumn::Title),
        ascending: Some(false),
    };
    assert_eq!(
        sort_direction(&sorted, SortColumn::Title),
        Some(accesskit::SortDirection::Descending)
    );
    assert_eq!(sort_direction(&sorted, SortColumn::Artist), None);
}

#[test]
fn dragging_a_grip_resizes_its_column_from_where_the_press_started() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let grip = h.grip("Title");
        let start = h.tree.bounds(grip).center();
        h.tree.pointer_move(start);
        h.tree.pointer_down_button(start, PointerButton::Primary);
        h.tree
            .pointer_move(Point::new(start.x + 40.0, start.y + 30.0));
        assert_eq!(h.widths.get()[&SortColumn::Title], 315);
        h.tree.pointer_move(Point::new(start.x - 400.0, start.y));
        assert_eq!(h.widths.get()[&SortColumn::Title], MIN_WIDTH);
        h.tree
            .pointer_up_button(Point::new(start.x - 400.0, start.y), PointerButton::Primary);
        h.relayout();
        h.tree.pointer_move(Point::new(start.x + 100.0, start.y));
        assert_eq!(
            h.widths.get()[&SortColumn::Title],
            MIN_WIDTH,
            "the drag ended"
        );
        assert_eq!(h.tree.bounds(h.cell("Title")).width, 30.0 - 15.0);
    });
}

#[test]
fn the_arrow_keys_step_a_focused_grip_and_shift_steps_one_pixel() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let grip = h.grip("Title");
        key_down(&mut h.tree, grip, Key::ArrowRight);
        assert_eq!(h.widths.get()[&SortColumn::Title], 285);
        key_down_with(&mut h.tree, grip, Key::ArrowLeft, Modifiers::SHIFT);
        assert_eq!(h.widths.get()[&SortColumn::Title], 284);
    });
}

#[test]
fn right_to_left_swaps_the_arrow_keys() {
    assert_eq!(
        key_delta(&Key::ArrowRight, false, LayoutDirection::LeftToRight),
        Some(10)
    );
    assert_eq!(
        key_delta(&Key::ArrowRight, true, LayoutDirection::RightToLeft),
        Some(-1)
    );
    assert_eq!(
        key_delta(&Key::Enter, false, LayoutDirection::LeftToRight),
        None
    );
}

#[test]
fn a_grip_is_a_splitter_valued_at_the_width_with_step_actions() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let grip = h.grip("Year");
        assert_eq!(h.tree.accessibility_node(grip).role(), Role::Splitter);
        let node_id = teksilo::core::accessibility::widget_id_to_node_id(grip);
        let update = h.tree.sync_accessibility();
        let node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == node_id)
            .map(|(_, node)| node.clone())
            .unwrap();
        assert_eq!(node.numeric_value(), Some(50.0));
        assert_eq!(node.min_numeric_value(), Some(f64::from(MIN_WIDTH)));
        h.tree.dispatch_event(WidgetEvent::AccessAction {
            action: accesskit::Action::Increment,
            target: Some(grip),
            target_node: teksilo::core::accessibility::widget_id_to_node_id(grip),
            data: None,
        });
        h.relayout();
        assert_eq!(h.widths.get()[&SortColumn::Year], 60);
    });
}

#[test]
fn a_new_width_resizes_the_column_in_place() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let grip = h.grip("Genre");
        let mut widths = h.widths.get();
        widths.insert(SortColumn::Genre, 200);
        h.widths.set(widths);
        h.relayout();
        assert_eq!(
            h.grip("Genre"),
            grip,
            "no rebuild: a drag in progress survives"
        );
        assert_eq!(h.tree.bounds(h.cell("Genre")).width, 200.0 - 15.0);
    });
}

#[test]
fn the_spanish_header_shows_the_iced_labels() {
    with_shipped_messages("es-ES", || {
        let h = harness(LibraryViewMode::SimpleList);
        let duration = h.cell("Duración");
        let shown: Vec<String> = {
            let mut texts = Vec::new();
            let mut stack = vec![duration];
            while let Some(node) = stack.pop() {
                if let Some(text) = h
                    .tree
                    .widget_as_any(node)
                    .and_then(|any| any.downcast_ref::<TextWidget>())
                {
                    texts.push(text.resolved_text());
                }
                stack.extend(h.tree.children(node));
            }
            texts
        };
        assert_eq!(shown, vec!["Du...".to_string()]);
        assert!(!find_by_name(&h.tree, h.header, "Título").is_empty());
    });
}

#[test]
fn a_keyboard_resize_is_announced_with_the_new_width() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        let _ = h.tree.sync_accessibility();
        let grip = h.grip("Year");
        key_down(&mut h.tree, grip, Key::ArrowRight);
        h.relayout();
        let _ = h.tree.sync_accessibility();
        h.relayout();
        let _ = h.tree.sync_accessibility();
        let said: Vec<String> = h
            .tree
            .announcements_since(0)
            .into_iter()
            .map(|announcement| announcement.text)
            .collect();
        assert!(
            said.iter()
                .any(|text| text.contains("Year") && text.contains("60")),
            "{said:?}"
        );
    });
}

#[test]
fn the_sorted_column_header_carries_its_sort_direction() {
    with_shipped_messages("en-US", || {
        let mut h = harness(LibraryViewMode::SimpleList);
        h.sort.set(SortState {
            column: Some(SortColumn::Title),
            ascending: Some(true),
        });
        h.relayout();
        let title = teksilo::core::accessibility::widget_id_to_node_id(h.cell("Title"));
        let artist = teksilo::core::accessibility::widget_id_to_node_id(h.cell("Artist"));
        let update = h.tree.sync_accessibility();
        let direction = |id| {
            update
                .nodes
                .iter()
                .find(|(node_id, _)| *node_id == id)
                .and_then(|(_, node)| node.sort_direction())
        };
        assert_eq!(direction(title), Some(accesskit::SortDirection::Ascending));
        assert_eq!(direction(artist), None);
    });
}
