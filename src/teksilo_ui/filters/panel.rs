//! [`FiltersPanel`]: the filter column, 202 px wide.
//!
//! Port of `src/gui/library_filters.rs`, everything under its 40 px top bar
//! (the logo strip is the title bar's), top to bottom:
//!
//! - the **selector** (`:178-216`): a 28 px strip in `COLOR_BG` showing the
//!   current type in Inter Medium 14 px, secondary, 15 px from the leading
//!   edge, and a 28 px up / down chevron 7 px from the trailing one, with a
//!   2 px `COLOR_CONTRAST` line under it. It is a popover trigger: the menu
//!   ([`filter_menu`]) opens under it, 172 px wide, the five types with their
//!   icons, the current one in the accent (`:219-345`); Escape and a click
//!   outside close it (quirk fix 4), and it is anchored at the selector
//!   (quirk fix 7; Iced puts it at a fixed 70 px from the window top);
//! - the **letter grid** (`:348-468`), shown once the index holds the type:
//!   the restore icon, then `#`, `•` and the letters that exist, in 16 × 16
//!   cells with no spacing, 4 px between lines, 10 px above and below and
//!   15 px on the sides, so ten cells to a line. The selected letter is
//!   primary on an accent disc (radius 18), the others secondary on nothing;
//!   the restore icon turns primary when hovered and fills with the accent
//!   while pressed. The grid is a group: the arrows move the focus between
//!   its cells (Left / Right by one, Up / Down by a line) and Enter or Space
//!   chooses the focused one;
//! - the **tree** (`:470-553`), shown once the index holds the type, in the
//!   remaining height: 32 px rows, a 22 px chevron (or 16 px of space for a
//!   leaf) after 12 px of indent per level, the label in Inter Medium 13 px,
//!   secondary, 15 px from the trailing edge; the selected and the hovered
//!   row are filled with `COLOR_CONTRAST`. The chevron toggles the node; a
//!   click or Enter on the row selects it (the library is then filtered);
//!   the arrows, Home, End, `*`, `+` and `-` are the tree view's own. Long
//!   labels end in an ellipsis where Iced cuts them with "..." by an
//!   estimate of the character width;
//! - the **search field** (`:556-570`): 180 px in a 40 px `COLOR_CONTRAST`
//!   bar, "Buscar...". The search prunes the tree; Escape clears it.
//!
//! While the index builds (and the type's tree is empty) the Iced column
//! shows the selector and the search field and nothing between them; so does
//! this one.

use std::rc::Rc;

use teksilo::core::accesskit::Role;
use teksilo::core::{Key, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight, HAlignment, VAlignment};
use teksilo::widgets::ActivateOn;
use teksilo::widgets::scroll_area::ScrollBarMode;
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, IconWidget, OverlayTrigger, Padding, Panel, PopoverCustom,
    RectWidget, Spacer, TextWidget, TreeView, VStack, Wrap, ZStack,
};

use super::filters_vm::FiltersViewModel;
use crate::logic::filter_index::{FilterType, TREE_ROW_HEIGHT, VisibleTreeItem};
use crate::teksilo_ui::icons;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::menu::{MenuEntry, filter_menu};
use crate::teksilo_ui::widgets::{Pressable, SearchField};

/// The filter menu's width (`:223`).
pub const MENU_WIDTH: f32 = 172.0;
/// The column's width (`library_filters.rs:549`).
pub const WIDTH: f32 = 202.0;
/// The selector's height (`:211`).
pub const SELECTOR_HEIGHT: f32 = 28.0;
/// The line under the selector (`:225`).
pub const SELECTOR_LINE: f32 = 2.0;
/// The selector's chevron (`:200`).
pub const SELECTOR_ICON: f32 = 28.0;
/// The distance of the selector's label from the leading edge (`:194`).
pub const SELECTOR_LEADING: f32 = 15.0;
/// The distance of the selector's chevron from the trailing edge (`:195`).
pub const SELECTOR_TRAILING: f32 = 7.0;
/// The side of a letter cell and of the restore cell (`:359`, `:385`).
pub const CELL: f32 = 16.0;
/// The restore icon's size (`:359`).
pub const RESTORE_ICON: f32 = 14.0;
/// The letter text size (`:432`).
pub const LETTER_SIZE: f32 = 12.0;
/// The space between lines of cells (`:351`).
pub const LINE_SPACING: f32 = 4.0;
/// The grid's padding above and below, and on the sides (`:352-357`).
pub const GRID_PADDING: (f32, f32) = (10.0, 15.0);
/// The cells to a line: the grid is `202 - 30 = 172` px wide, 16 px a cell.
pub const CELLS_PER_LINE: usize = 10;
/// The corner radius of a selected letter's fill (`:454`), which is larger
/// than the cell, so a disc.
pub const LETTER_RADIUS: f32 = 18.0;
/// A tree row's height (`:538`).
pub const ROW_HEIGHT: f32 = TREE_ROW_HEIGHT;
/// The indent per level (`:490`).
pub const INDENT: f32 = 12.0;
/// The chevron button and icon (`:508`).
pub const CHEVRON: f32 = 22.0;
/// The space a leaf keeps where a chevron would be (`:511`).
pub const LEAF_SPACE: f32 = 16.0;
/// The distance of a row's label from the trailing edge (`:540`).
pub const ROW_TRAILING: f32 = 15.0;
/// The tree label's size (`:497`).
pub const LABEL_SIZE: f32 = 13.0;
/// The search bar's height (`:557`).
pub const SEARCH_BAR_HEIGHT: f32 = 40.0;
/// The search field's width (`:556`).
pub const SEARCH_WIDTH: f32 = 180.0;

/// The label of a filter type, in the Iced UI's Spanish (`label`, `:13`).
fn type_label(filter_type: FilterType) -> LocalizedString {
    match filter_type {
        FilterType::Folder => tr!(filters_type_folder()),
        FilterType::Artist => tr!(filters_type_artist()),
        FilterType::Album => tr!(filters_type_album()),
        FilterType::Genre => tr!(filters_type_genre()),
        FilterType::Year => tr!(filters_type_year()),
    }
}

/// The menu icon of a filter type (`:238-245`).
fn type_icon(filter_type: FilterType) -> fn() -> IconWidget {
    match filter_type {
        FilterType::Folder => icons::shared::folder_open_outlined,
        FilterType::Artist => icons::filters::artist_outlined,
        FilterType::Album => icons::shared::album,
        FilterType::Genre => icons::filters::music_note_2,
        FilterType::Year => icons::filters::calendar_rounded,
    }
}

/// A column as Iced lays one out by default: no spacing, children at the
/// leading edge.
fn column() -> VStack {
    VStack::new().spacing(0.0).alignment(HAlignment::Leading)
}

// ---------------------------------------------------------------------------
// The selector and its menu
// ---------------------------------------------------------------------------

/// The five types as menu entries, the current one marked.
fn type_entries(vm: &FiltersViewModel) -> Vec<MenuEntry> {
    let current = vm.filter_type().get();
    FilterType::ALL
        .into_iter()
        .map(|filter_type| {
            let vm = vm.clone();
            MenuEntry::item(type_label(filter_type))
                .icon(type_icon(filter_type))
                .selected(current == filter_type)
                .on_activate(move |_| vm.set_filter_type(filter_type))
        })
        .collect()
}

/// The popover's content: the filter menu, rebuilt when the type changes so
/// the accent follows the current one.
struct TypeMenu {
    vm: FiltersViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for TypeMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeMenu").finish_non_exhaustive()
    }
}

impl Widget for TypeMenu {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.filter_type().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            teksilo::core::binding::BindingLevel::Rebuild,
        );
        let root = ctx.add(
            // The popover stretches its content to the trigger's 202 px; the
            // menu keeps its 172 px at the leading edge.
            ZStack::new().alignment(Alignment::TOP_LEADING).child(
                FixedSize::new()
                    .width(MENU_WIDTH)
                    .child(filter_menu(type_entries(&self.vm))),
            ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// The selector strip and the line under it.
fn selector(vm: &FiltersViewModel) -> impl Widget + use<> {
    let open = Signal::new(false);
    let label = vm
        .filter_type()
        .map(|filter_type| type_label(*filter_type).resolve_now());
    let strip = FixedSize::new().width(WIDTH).height(SELECTOR_HEIGHT).child(
        ZStack::new()
            .child(RectWidget::new().background(SurfaceRole::Main))
            .child(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Center)
                    .child(
                        Padding::new(0.0, 0.0, 0.0, SELECTOR_LEADING).child(
                            TextWidget::new(lit!(""))
                                .text(label)
                                .style(style::inter(14.0, FontWeight::MEDIUM))
                                .color(TextRole::Secondary)
                                .single_line()
                                .a11y_hidden(),
                        ),
                    )
                    .child(Spacer::new())
                    .child(
                        Padding::new(0.0, SELECTOR_TRAILING, 0.0, 0.0).child(
                            ZStack::new()
                                .child(
                                    icons::shared::arrow_down_chevron()
                                        .icon_size(SELECTOR_ICON)
                                        .color(style::ICON_ROLE)
                                        .visible_when(open.map(|open| !*open)),
                                )
                                .child(
                                    icons::shared::arrow_up_chevron()
                                        .icon_size(SELECTOR_ICON)
                                        .color(style::ICON_ROLE)
                                        .visible_when(open.clone()),
                                ),
                        ),
                    ),
            ),
    );
    let line = FixedSize::new()
        .width(WIDTH)
        .height(SELECTOR_LINE)
        .child(RectWidget::new().background(SurfaceRole::Raised));
    let name = tr!(filters_selector()).resolve_now();
    let on_open = open.clone();
    let on_close = open;
    PopoverCustom::new(OverlayTrigger::around(column().child(strip).child(line)).named(name))
        .content(TypeMenu {
            vm: vm.clone(),
            root: None,
        })
        .bare()
        .on_open(move || on_open.set(true))
        .on_close(move || on_close.set(false))
}

// ---------------------------------------------------------------------------
// The letter grid
// ---------------------------------------------------------------------------

/// The cell the arrow `key` moves to from cell `from` of `count` cells (cell 0
/// is the restore icon), or `None` when the key is not an arrow or the move
/// would leave the grid. Left and Right move by one, Up and Down by a line of
/// [`CELLS_PER_LINE`].
pub fn next_cell(from: usize, count: usize, key: &Key) -> Option<usize> {
    let to = match key {
        Key::ArrowLeft => from.checked_sub(1)?,
        Key::ArrowRight => from + 1,
        Key::ArrowUp => from.checked_sub(CELLS_PER_LINE)?,
        Key::ArrowDown => from + CELLS_PER_LINE,
        _ => return None,
    };
    (to < count).then_some(to)
}

/// The restore cell: the icon in secondary, primary while hovered, on an
/// accent disc while pressed (`:360-381`).
fn restore_cell(vm: &FiltersViewModel) -> Pressable {
    let hovered = Signal::new(false);
    let pressed = Signal::new(false);
    let down = pressed.clone();
    let up = pressed.clone();
    let fill = pressed.map(|pressed| {
        if *pressed {
            SurfaceRole::Accent
        } else {
            SurfaceRole::Transparent
        }
    });
    let tint = hovered.map(|hovered| {
        if *hovered {
            TextRole::Primary
        } else {
            TextRole::Secondary
        }
    });
    let vm = vm.clone();
    Pressable::new(
        FixedSize::new()
            .width(CELL)
            .height(CELL)
            .child(
                ZStack::new()
                    .child(
                        RectWidget::new()
                            .background(fill)
                            .corner_radius(CornerRadius::uniform(LETTER_RADIUS)),
                    )
                    .child(
                        Center::new()
                            .child(icons::shared::restore().icon_size(RESTORE_ICON).color(tint)),
                    ),
            )
            .on_pointer_event(move |event, _| {
                match event {
                    WidgetEvent::PointerDown { .. } => down.set(true),
                    WidgetEvent::PointerUp { .. } => up.set(false),
                    _ => {}
                }
                EventResponse::Ignored
            })
            .on_pointer_cancel({
                let pressed = pressed.clone();
                move |_, _, _| pressed.set(false)
            }),
        tr!(filters_letter_all()),
    )
    .hovered(hovered)
    .corner_radius(LETTER_RADIUS)
    .on_activate(move |_| vm.select_subfilter(None))
}

/// One letter's cell (`:408-465`).
fn letter_cell(vm: &FiltersViewModel, letter: &str) -> impl Widget + use<> {
    let selected = {
        let letter = letter.to_string();
        vm.subfilter()
            .map(move |current| current.as_deref() == Some(letter.as_str()))
    };
    let fill = selected.map(|selected| {
        if *selected {
            SurfaceRole::Accent
        } else {
            SurfaceRole::Transparent
        }
    });
    let tint = selected.map(|selected| {
        if *selected {
            TextRole::Primary
        } else {
            TextRole::Secondary
        }
    });
    let picked = selected.clone();
    let vm = vm.clone();
    let chosen = letter.to_string();
    Pressable::new(
        FixedSize::new().width(CELL).height(CELL).child(
            ZStack::new()
                .child(
                    RectWidget::new()
                        .background(fill)
                        .corner_radius(CornerRadius::uniform(LETTER_RADIUS)),
                )
                .child(
                    Center::new().child(
                        TextWidget::new(lit!(letter))
                            .style(style::inter(LETTER_SIZE, FontWeight::MEDIUM))
                            .color(tint)
                            .single_line()
                            .no_shrink()
                            .a11y_hidden(),
                    ),
                ),
        ),
        lit!(letter),
    )
    .corner_radius(LETTER_RADIUS)
    .on_activate(move |_| vm.select_subfilter(Some(&chosen)))
    .access_customize(move |builder| builder.set_selected(picked.get()))
}

/// The letter grid; see the module documentation. It is empty until the
/// index holds letters for the shown type, and it is rebuilt when they change.
struct LetterGrid {
    vm: FiltersViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for LetterGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LetterGrid").finish_non_exhaustive()
    }
}

impl Widget for LetterGrid {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let letters = self.vm.subfilters();
        letters.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            teksilo::core::binding::BindingLevel::Rebuild,
        );
        let letters = letters.get();
        if letters.is_empty() {
            let root = ctx.add(FixedSize::new().width(0.0).height(0.0).child(Spacer::new()));
            self.root = Some(root);
            return vec![root];
        }
        let mut cells = vec![ctx.add(restore_cell(&self.vm))];
        cells.extend(
            letters
                .iter()
                .map(|letter| ctx.add(letter_cell(&self.vm, letter))),
        );
        let cells = Rc::new(cells);
        let keys = cells.clone();
        let (vertical, horizontal) = GRID_PADDING;
        let root = ctx.add(
            Padding::new(vertical, horizontal, vertical, horizontal)
                .child(
                    Wrap::new()
                        .spacing(0.0)
                        .line_spacing(LINE_SPACING)
                        .children(cells.iter().copied()),
                )
                .on_key_preview(move |event, ctx| {
                    let WidgetEvent::KeyDown { key, .. } = event else {
                        return EventResponse::Ignored;
                    };
                    let Some(from) = keys.iter().position(|id| Some(*id) == ctx.focused()) else {
                        return EventResponse::Ignored;
                    };
                    match next_cell(from, keys.len(), key) {
                        Some(to) => {
                            ctx.request_focus(keys[to]);
                            EventResponse::Handled
                        }
                        None => EventResponse::Ignored,
                    }
                })
                .access_role(Role::Group)
                .access_label(tr!(filters_letters())),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// The tree
// ---------------------------------------------------------------------------

/// One tree row (`:470-545`).
fn tree_row(
    item: &VisibleTreeItem,
    toggle: Rc<dyn Fn(&mut EventContext)>,
    selected: bool,
) -> Box<dyn Widget> {
    let hovered = Signal::new(false);
    let fill = hovered.map(move |hovered| {
        if selected || *hovered {
            SurfaceRole::Raised
        } else {
            SurfaceRole::Transparent
        }
    });
    let leading: Box<dyn Widget> = if item.has_children {
        let icon = if item.is_expanded {
            icons::shared::arrow_down_chevron()
        } else {
            icons::filters::arrow_right_chevron()
        };
        Box::new(
            FixedSize::new()
                .width(CHEVRON)
                .height(CHEVRON)
                .child(icon.icon_size(CHEVRON).color(style::ICON_ROLE))
                .on_tap(move |_, ctx| toggle(ctx))
                .cursor(CursorIcon::Pointer),
        )
    } else {
        Box::new(
            FixedSize::new()
                .width(LEAF_SPACE)
                .height(1.0)
                .child(Spacer::new()),
        )
    };
    Box::new(
        FixedSize::new()
            .height(ROW_HEIGHT)
            .child(
                ZStack::new()
                    .alignment(Alignment::CENTER_LEADING)
                    .child(RectWidget::new().background(fill))
                    .child(
                        Padding::new(0.0, ROW_TRAILING, 0.0, item.depth as f32 * INDENT).child(
                            HStack::new()
                                .spacing(0.0)
                                .alignment(VAlignment::Center)
                                .child(leading)
                                .child(
                                    TextWidget::new(lit!(&item.label))
                                        .style(style::inter(LABEL_SIZE, FontWeight::MEDIUM))
                                        .color(TextRole::Secondary)
                                        .single_line()
                                        .overflow(TextOverflow::Ellipsis(EllipsisMode::Trailing))
                                        .a11y_hidden(),
                                ),
                        ),
                    ),
            )
            .on_hover(move |entered, _| hovered.set(entered)),
    )
}

/// The tree view over the view-model's source.
fn tree(vm: &FiltersViewModel) -> impl Widget + use<> {
    let source = vm.tree_source();
    let activate = vm.clone();
    let shown = vm.subfilters().map(|letters| !letters.is_empty());
    TreeView::from_source_keyed(
        source,
        vm.selection(),
        |item: &VisibleTreeItem, row, selected| tree_row(item, row.toggle_callback(), selected),
    )
    .item_height(ROW_HEIGHT)
    // Iced's bars are 8 px overlays that take no width from the rows, and the
    // wheel does not glide (`standard_scrollable`, `widgets.rs:1648`).
    .scroll_bar_style(ScrollBarMode::Overlay)
    .smooth_scrolling(false)
    .row_click_expands(false)
    .activate_on(ActivateOn::SingleClick)
    .on_activate(move |index, _| {
        if let Some(id) = activate.tree_source().id_at(index) {
            activate.select_node(&id);
        }
    })
    .access_label(tr!(filters_tree()))
    .visible_when(shown)
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

/// The filter column; see the module documentation. Mount it in the 202 px
/// region under the title bar's logo cell.
pub struct FiltersPanel {
    vm: FiltersViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for FiltersPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FiltersPanel").finish_non_exhaustive()
    }
}

impl FiltersPanel {
    /// The column over `vm`.
    pub fn new(vm: FiltersViewModel) -> Self {
        Self { vm, root: None }
    }
}

impl Widget for FiltersPanel {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.wire(ctx);
        let search = FixedSize::new()
            .width(WIDTH)
            .height(SEARCH_BAR_HEIGHT)
            .child(
                ZStack::new()
                    .child(RectWidget::new().background(SurfaceRole::Raised))
                    .child(Center::new().child(SearchField::new(
                        self.vm.search(),
                        tr!(filters_search()),
                        SEARCH_WIDTH,
                    ))),
            );
        let root = ctx.add(
            FixedSize::new().width(WIDTH).child(
                Panel::new()
                    .padding(0.0)
                    .border_width(0.0)
                    .corner_radius(0.0)
                    .background(SurfaceRole::Main)
                    .child(
                        column()
                            .child(selector(&self.vm))
                            .child(LetterGrid {
                                vm: self.vm.clone(),
                                root: None,
                            })
                            .child(Expand::new().child(tree(&self.vm)))
                            .child(search),
                    ),
            ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(WIDTH, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
