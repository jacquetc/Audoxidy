//! The Audoxidy menus: context menus, the filter-type menu and the player menu.
//!
//! The Iced UI draws three kinds of menu:
//!
//! - **context menus** (`build_context_menu_content`, `widgets.rs:1944`): the
//!   playlist's row, tab, bar and empty-area menus, the tab overflow menu and
//!   the "Enviar a otra lista" list (`playlist.rs:470-592`, `1030-1140`,
//!   `app.rs:2675`, `app.rs:4000`). Rows are 32 px high: a 24 px icon (or 24 px
//!   of space) 10 px in, 15 px of space, the label in Inter 13 px, all primary;
//!   a hovered row turns `COLOR_CONTRAST`. A divider is a 1 px line in the
//!   secondary text colour. The menu is as wide as its longest label allows
//!   (`7.5 px × characters + 52`, at least 80), filled with `COLOR_BG`, with a
//!   1 px secondary border, an 8 px radius, a shadow 4 px below, and 5 px of
//!   padding above and below, 1 px on the sides;
//! - the **filter-type menu** (`library_filters.rs:237-345`): the same frame,
//!   172 px wide; 32 px rows of an 18 px icon 15 px in, 15 px of space and the
//!   label in Inter 14 px; the current filter's label is in the accent;
//! - the **player menu** (`player.rs:455-520`): text rows only (Inter Medium
//!   13 px, primary, 4 px above and below), dividers with 2 px of space on
//!   each side, in a 180 px panel with 15 px of padding, `COLOR_BG`, a 2 px
//!   accent border and an 8 px radius, and no hover feedback.
//!
//! Context and filter menus are stock `MenuList`s of `MenuItem`s, so they keep
//! the framework's keyboard handling (arrows, Home / End, Enter, type-ahead,
//! Escape), its submenus and its accessibility. Their rows are drawn by
//! [`MenuRowStyle`] and their frame by [`MenuSurfaceStyle`], the theme's
//! `PopoverStyle` for menu surfaces. Dividers are drawn rows that the keyboard
//! skips. Disabled entries ([`MenuEntry::enabled`]) are the Iced stubs, kept
//! visible and greyed.
//!
//! Two consequences of building on `MenuList`, both small:
//!
//! - `MenuList` keeps 4 px between its frame and its rows on every side, so
//!   the frame adds 1 px above and below (5 px in all, as in Iced) and nothing
//!   on the sides: a hovered row stops 4 px from the side borders where the
//!   Iced one stops 1 px from them. The icons and labels sit where Iced puts
//!   them.
//! - The "Enviar a otra lista" entry is a real submenu: it opens beside the
//!   menu, with a chevron, where Iced replaced the menu with the playlist list
//!   and an "Atrás" entry. It opens on a click (or → / Enter), as Iced's
//!   does, never by resting the pointer on it ([`SUBMENU_HOVER_DELAY`]).
//!
//! The player menu is a small custom list ([`PlayerMenu`]): a `MenuList`'s
//! frame is the theme's menu surface, shared by every menu, so it cannot also
//! take the player menu's accent frame. It has the same keyboard model: Up and
//! Down move between the enabled entries, Enter and Space activate, and the
//! host popover closes on Escape.

use std::rc::Rc;

use teksilo::core::accesskit::Role;
use teksilo::core::styles::{
    MenuItemMetrics, MenuItemStyle, MenuItemStyleConfig, PopoverStyle, PopoverStyleConfig,
    PopoverVariant,
};
use teksilo::core::{Key, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, Shadow, VAlignment};
use teksilo::widgets::styles::RecipePopoverStyle;
use teksilo::widgets::{
    FixedSize, HStack, IconWidget, MenuItem, MenuList, Padding, RectWidget, Spacer, TextWidget,
    VStack, ZStack,
};

use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;
use crate::teksilo_ui::widgets::dialog_frame::Frame;
use crate::teksilo_ui::widgets::pressable::ActivateFn;

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// What one menu entry is.
enum EntryKind {
    /// A command.
    Item(Option<ActivateFn>),
    /// A submenu, its entries built each time it opens.
    Submenu(Rc<dyn Fn() -> Vec<MenuEntry>>),
    /// A divider line.
    Divider,
}

/// One entry of an Audoxidy menu: a command, a submenu or a divider.
pub struct MenuEntry {
    kind: EntryKind,
    label: LocalizedString,
    icon: Option<fn() -> IconWidget>,
    enabled: Prop<bool>,
    selected: bool,
    icon_shift: f32,
}

impl std::fmt::Debug for MenuEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            EntryKind::Item(_) => "item",
            EntryKind::Submenu(_) => "submenu",
            EntryKind::Divider => "divider",
        };
        f.debug_struct("MenuEntry")
            .field("kind", &kind)
            .field("label", &self.label.resolve_now())
            .finish_non_exhaustive()
    }
}

impl MenuEntry {
    fn new(kind: EntryKind, label: LocalizedString) -> Self {
        Self {
            kind,
            label,
            icon: None,
            enabled: Prop::Static(true),
            selected: false,
            icon_shift: 0.0,
        }
    }

    /// A command labelled `label`; give it an action with
    /// [`on_activate`](Self::on_activate).
    pub fn item(label: impl Into<LocalizedString>) -> Self {
        Self::new(EntryKind::Item(None), label.into())
    }

    /// A submenu labelled `label` whose entries `entries` builds when it opens
    /// (context and filter menus only).
    pub fn submenu(
        label: impl Into<LocalizedString>,
        entries: impl Fn() -> Vec<MenuEntry> + 'static,
    ) -> Self {
        Self::new(EntryKind::Submenu(Rc::new(entries)), label.into())
    }

    /// A divider line.
    pub fn divider() -> Self {
        Self::new(EntryKind::Divider, lit!(""))
    }

    /// What activating the command does. The menu closes after it.
    pub fn on_activate(mut self, f: impl Fn(&mut EventContext) + 'static) -> Self {
        if let EntryKind::Item(action) = &mut self.kind {
            *action = Some(Rc::new(f));
        }
        self
    }

    /// The entry's icon (an `icons.rs` factory); ignored by the player menu,
    /// which has none.
    pub fn icon(mut self, icon: fn() -> IconWidget) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether the entry can be activated (default `true`). The Iced stubs
    /// are disabled entries.
    pub fn enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    /// Marks the entry as the current choice: its label is drawn in the
    /// accent (the filter-type menu's current filter).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Moves the icon `shift` px towards the leading edge and the label not
    /// at all. The Iced context menu does this for "Renombrar lista"
    /// (`widgets.rs:1971`: 9 px in and 16 px of space instead of 10 and 15),
    /// whose icon is drawn off-centre.
    pub fn icon_shift(mut self, shift: f32) -> Self {
        self.icon_shift = shift;
        self
    }

    /// The entry's label.
    pub fn label(&self) -> &LocalizedString {
        &self.label
    }

    /// Whether this is a divider.
    pub fn is_divider(&self) -> bool {
        matches!(self.kind, EntryKind::Divider)
    }
}

// ---------------------------------------------------------------------------
// Looks
// ---------------------------------------------------------------------------

/// A menu row's height in the context and filter menus (`widgets.rs:1995`).
pub const ROW_HEIGHT: f32 = 32.0;
/// The padding `MenuList` keeps around its rows.
pub const MENU_LIST_INSET: f32 = 4.0;
/// The context menu's horizontal padding (`widgets.rs:2066`).
const CONTEXT_SIDE_PADDING: f32 = 1.0;
/// The context menu's vertical padding (`widgets.rs:2064`).
const CONTEXT_VERTICAL_PADDING: f32 = 5.0;
/// The menu frame's radius (`widgets.rs:2054`).
pub const MENU_RADIUS: f32 = 8.0;
/// The filter menu's width (`library_filters.rs:248`).
pub const FILTER_MENU_WIDTH: f32 = 172.0;
/// The player menu's width (`player.rs:500`).
pub const PLAYER_MENU_WIDTH: f32 = 180.0;
/// The player menu's padding (`player.rs:501`).
pub const PLAYER_MENU_PADDING: f32 = 15.0;
/// The player menu's accent border width (`player.rs:507`).
pub const PLAYER_MENU_BORDER: f32 = 2.0;
/// The space above and below a player menu label (`player.rs:463`).
pub const PLAYER_ROW_PADDING: f32 = 4.0;
/// The space on each side of a player menu divider (`player.rs:470`).
pub const PLAYER_DIVIDER_SPACE: f32 = 2.0;

/// The shadow under the context and filter menus: 4 px down, 10 px blur,
/// black at 50 % (`widgets.rs:2056`). A literal colour: a drop shadow is black
/// whatever the theme.
pub fn menu_shadow() -> Shadow {
    Shadow {
        offset_x: 0.0,
        offset_y: 4.0,
        blur: 10.0,
        spread: 0.0,
        color: Color::new(0.0, 0.0, 0.0, 0.5),
    }
}

/// The width of an Iced context menu for labels of `chars` characters at most
/// (`widgets.rs:1955`).
pub fn context_menu_width(chars: usize) -> f32 {
    (chars as f32 * 7.5 + 52.0).max(80.0)
}

/// The geometry of the rows of one menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowMetrics {
    /// The row's width.
    pub width: f32,
    /// Space before the icon column, from the row's leading edge.
    pub leading: f32,
    /// The icon column's width (reserved even with no icon).
    pub icon_column: f32,
    /// Space between the icon column and the label.
    pub gap: f32,
    /// The label's size, in Inter regular.
    pub text_size: f32,
}

impl RowMetrics {
    /// The rows of a context menu `menu_width` wide: the Iced row is 1 px in
    /// from the frame and puts its icon 10 px further (`widgets.rs:1972-2001`);
    /// a `MenuList` row is [`MENU_LIST_INSET`] in, so it starts its icon
    /// that much less into the row.
    pub fn context(menu_width: f32) -> Self {
        Self {
            width: menu_width - 2.0 * MENU_LIST_INSET,
            leading: 10.0 + CONTEXT_SIDE_PADDING - MENU_LIST_INSET,
            icon_column: 24.0,
            gap: 15.0,
            text_size: 13.0,
        }
    }

    /// The rows of the filter-type menu (`library_filters.rs:262-292`): the
    /// 18 px icon 15 px into a row that starts 1 px in.
    pub fn filter() -> Self {
        Self {
            width: FILTER_MENU_WIDTH - 2.0 * MENU_LIST_INSET,
            leading: 15.0 + CONTEXT_SIDE_PADDING - MENU_LIST_INSET,
            icon_column: 18.0,
            gap: 15.0,
            text_size: 14.0,
        }
    }
}

/// The row of a context or filter menu as a `MenuItemStyle`: on the
/// background colour, `COLOR_CONTRAST` while hovered or reached with the
/// keyboard, and back to the background while pressed, as the Iced button
/// style reads only the hovered status (`widgets.rs:2007`).
#[derive(Debug, Clone, Copy)]
pub struct MenuRowStyle {
    metrics: RowMetrics,
    icon_shift: f32,
}

impl MenuRowStyle {
    /// Rows with `metrics`.
    pub fn new(metrics: RowMetrics) -> Self {
        Self {
            metrics,
            icon_shift: 0.0,
        }
    }
}

/// A row's fill: `COLOR_CONTRAST` while lit and not pressed, the background
/// otherwise.
pub fn row_fill(lit: bool, pressed: bool) -> SurfaceRole {
    if lit && !pressed {
        SurfaceRole::Raised
    } else {
        SurfaceRole::Main
    }
}

impl MenuItemStyle for MenuRowStyle {
    fn make_body(&self, cfg: &MenuItemStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        let m = self.metrics;
        let fill = cfg
            .is_hovered
            .zip3(&cfg.is_highlighted, &cfg.is_pressed)
            .map(|(hovered, highlighted, pressed)| row_fill(*hovered || *highlighted, *pressed));
        let leading = (m.leading - self.icon_shift).max(0.0);
        let mut row = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(gap(leading));
        row = match cfg.leading {
            Some(icon) => row.child(icon),
            None => row.child(gap(m.icon_column)),
        };
        row = row
            .child(gap(m.gap + self.icon_shift))
            .child(cfg.label)
            .child(Spacer::new());
        if let Some(trailing) = cfg.trailing {
            row = row.child(trailing);
        }
        ctx.add(
            FixedSize::new().width(m.width).height(ROW_HEIGHT).child(
                ZStack::new()
                    .child(RectWidget::new().background(fill))
                    .child(row),
            ),
        )
    }

    fn metrics(&self) -> MenuItemMetrics {
        MenuItemMetrics {
            icon_column_width: self.metrics.icon_column,
            trailing_column_width: TRAILING_COLUMN,
            separator_height: 1.0,
            item_height: ROW_HEIGHT,
        }
    }
}

/// The submenu chevron's column.
const TRAILING_COLUMN: f32 = 16.0;

/// A fixed horizontal gap.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

/// The theme's menu surface (`PopoverVariant::Menu`): the Iced context-menu
/// frame (`widgets.rs:2048-2067`). The other popover variants keep the stock
/// look. Install it theme-wide with [`install`].
#[derive(Debug, Clone, Copy, Default)]
pub struct MenuSurfaceStyle;

impl PopoverStyle for MenuSurfaceStyle {
    fn make_body(&self, cfg: &PopoverStyleConfig, ctx: &mut BuildContext) -> WidgetId {
        if cfg.variant != PopoverVariant::Menu {
            return RecipePopoverStyle::for_tokens(&ctx.theme().input).make_body(cfg, ctx);
        }
        let vertical = CONTEXT_VERTICAL_PADDING - MENU_LIST_INSET;
        ctx.add(
            Frame::around(cfg.content)
                .fill(SurfaceRole::Main)
                .border(TextRole::Secondary, 1.0)
                .radius(MENU_RADIUS)
                .shadow(menu_shadow())
                .padding(vertical, 0.0, vertical, 0.0),
        )
    }
}

/// Installs the menu surface theme-wide. Called by `style::audoxidy_dark`.
pub fn install(slots: &mut teksilo::core::styles::ComponentStyleSlots) {
    slots.popover = Some(Rc::new(MenuSurfaceStyle));
}

// ---------------------------------------------------------------------------
// Context and filter menus
// ---------------------------------------------------------------------------

/// Which menu rows to build.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Look {
    Context,
    Filter,
}

/// An Iced context menu of `entries`, for a `.context_menu(…)` factory or a
/// popover. Its width follows the longest label (see [`context_menu_width`]).
pub fn context_menu(entries: Vec<MenuEntry>) -> MenuList {
    let chars = entries
        .iter()
        .map(|entry| entry.label.resolve_now().chars().count())
        .max()
        .unwrap_or(0);
    build_list(
        entries,
        RowMetrics::context(context_menu_width(chars)),
        Look::Context,
    )
}

/// The filter-type menu of `entries`, 172 px wide.
pub fn filter_menu(entries: Vec<MenuEntry>) -> MenuList {
    build_list(entries, RowMetrics::filter(), Look::Filter)
}

fn build_list(entries: Vec<MenuEntry>, metrics: RowMetrics, look: Look) -> MenuList {
    entries
        .into_iter()
        .fold(MenuList::new(), |list, entry| match entry.kind {
            EntryKind::Divider => list.header(MenuDivider {
                width: metrics.width,
            }),
            _ => list.item(menu_item(entry, metrics, look)),
        })
}

/// How long the pointer rests on a submenu entry before the submenu opens by
/// itself: never, in practice. A click (or → / Enter) opens it, as the Iced
/// "Enviar a otra lista" opens on a click only. Teksilo 0.16.0's
/// `MenuItem::submenu` opens a second copy of a submenu a click already
/// opened once its hover delay elapses; leaving the trigger then closes
/// both, and the click meant for an entry falls through to nothing (probe
/// F1). With no hover opening there is no second copy.
pub const SUBMENU_HOVER_DELAY: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// One `MenuItem` in the Iced look.
fn menu_item(entry: MenuEntry, metrics: RowMetrics, look: Look) -> MenuItem {
    let row_style = MenuRowStyle {
        metrics,
        icon_shift: entry.icon_shift,
    };
    let mut item = match entry.kind {
        EntryKind::Submenu(entries) => MenuItem::submenu(entry.label.clone(), move || {
            let list: Box<dyn Widget> = match look {
                Look::Context => Box::new(context_menu(entries())),
                Look::Filter => Box::new(filter_menu(entries())),
            };
            list
        })
        .submenu_delay(SUBMENU_HOVER_DELAY),
        EntryKind::Item(action) => {
            let item = MenuItem::new(entry.label.clone());
            match action {
                Some(action) => item.on_activate_fn(move |ctx| action(ctx)),
                None => item,
            }
        }
        EntryKind::Divider => MenuItem::new(entry.label.clone()),
    };
    item = item
        .style(row_style)
        .text_style(style::inter(metrics.text_size, FontWeight::REGULAR))
        .enabled(entry.enabled);
    if let Some(icon) = entry.icon {
        let icon = icon().icon_size(metrics.icon_column);
        item = if entry.selected {
            // The Iced filter icon stays primary on the current filter.
            item.icon(icon.color(TextRole::Primary)).icon_keeps_color()
        } else {
            item.icon(icon)
        };
    }
    if entry.selected {
        item = item.text_role(TextRole::Accent);
    }
    item
}

/// A 1 px divider line in the secondary text colour (`widgets.rs:2031`).
#[derive(Debug)]
pub struct MenuDivider {
    width: f32,
}

impl MenuDivider {
    /// A divider `width` px long.
    pub fn new(width: f32) -> Self {
        Self { width }
    }
}

impl Widget for MenuDivider {
    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(self.width, 1.0).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let color = ColorProp::from(TextRole::Secondary).resolve(ctx.theme, true);
        canvas.fill_rect(bounds, color);
    }
}

// ---------------------------------------------------------------------------
// Player menu
// ---------------------------------------------------------------------------

/// The player menu; see the module documentation. Host it in an overlay (a
/// `PopoverCustom` on `menu.svg`): activating an entry runs its action, then
/// closes the overlay it is in.
pub struct PlayerMenu {
    entries: Vec<MenuEntry>,
    label: LocalizedString,
    rows: PlayerRows,
    root: Option<WidgetId>,
}

/// The player menu's rows, as the key handler reads them: each row's
/// `Pressable` and whether it is enabled.
type PlayerRows = Rc<std::cell::RefCell<Vec<(WidgetId, Signal<bool>)>>>;

impl std::fmt::Debug for PlayerMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerMenu")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl PlayerMenu {
    /// A player menu of `entries` (items and dividers; submenus are not
    /// supported), announced as `label`.
    pub fn new(entries: Vec<MenuEntry>, label: impl Into<LocalizedString>) -> Self {
        Self {
            entries,
            label: label.into(),
            rows: Rc::default(),
            root: None,
        }
    }

    /// One text row (`player.rs:456-466`).
    fn row(entry: MenuEntry, enabled: Signal<bool>) -> Pressable {
        let action = match entry.kind {
            EntryKind::Item(action) => action,
            EntryKind::Submenu(_) | EntryKind::Divider => None,
        };
        let body = Padding::symmetric(PLAYER_ROW_PADDING, 0.0).child(
            TextWidget::new(entry.label.clone())
                .style(style::inter(13.0, FontWeight::MEDIUM))
                .color(TextRole::Primary)
                .single_line()
                .a11y_hidden(),
        );
        Pressable::new(body, entry.label)
            .enabled(enabled)
            .on_activate(move |ctx| {
                if let Some(action) = &action {
                    action(ctx);
                }
                ctx.dismiss_self_overlay_chain();
            })
    }
}

/// The player divider: 2 px, a 1 px secondary line, 2 px (`player.rs:470-474`).
fn player_divider(width: f32) -> Padding {
    Padding::symmetric(PLAYER_DIVIDER_SPACE, 0.0).child(MenuDivider::new(width))
}

/// The entry `from` moves to with `key` among `enabled` rows (Up and Down,
/// wrapping as a menu does), or `None` for another key.
pub fn next_row(enabled: &[bool], from: Option<usize>, key: &Key) -> Option<usize> {
    let count = enabled.len();
    let step = match key {
        Key::ArrowDown => 1,
        Key::ArrowUp => count.saturating_sub(1),
        _ => return None,
    };
    if count == 0 {
        return None;
    }
    let mut index = match from {
        Some(index) => (index + step) % count,
        None if step == 1 => 0,
        None => count - 1,
    };
    for _ in 0..count {
        if enabled[index] {
            return Some(index);
        }
        index = (index + step) % count;
    }
    None
}

impl Widget for PlayerMenu {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let inner = PLAYER_MENU_WIDTH - 2.0 * PLAYER_MENU_PADDING;
        let mut column = VStack::new().spacing(0.0).alignment(HAlignment::Leading);
        let mut rows = Vec::new();
        for entry in std::mem::take(&mut self.entries) {
            if entry.is_divider() {
                column = column.child(player_divider(inner));
                continue;
            }
            let enabled = match &entry.enabled {
                Prop::Static(value) => ctx.signal(*value),
                Prop::Bound(signal) => signal.clone(),
            };
            let id = ctx.add(Self::row(entry, enabled.clone()).access_role(Role::MenuItem));
            rows.push((id, enabled));
            column = column.child(FixedSize::new().width(inner).child(id));
        }
        *self.rows.borrow_mut() = rows;
        let rows = self.rows.clone();
        let root = ctx.add(
            FixedSize::new()
                .width(PLAYER_MENU_WIDTH)
                .child(
                    Frame::new(column)
                        .fill(SurfaceRole::Main)
                        .border(BorderRole::Accent, PLAYER_MENU_BORDER)
                        .radius(MENU_RADIUS)
                        .padding(
                            PLAYER_MENU_PADDING,
                            PLAYER_MENU_PADDING,
                            PLAYER_MENU_PADDING,
                            PLAYER_MENU_PADDING,
                        ),
                )
                .on_key_preview(move |event, ctx| {
                    let WidgetEvent::KeyDown { key, .. } = event else {
                        return EventResponse::Ignored;
                    };
                    let rows = rows.borrow();
                    let focused = ctx.focused();
                    let from = rows.iter().position(|(id, _)| Some(*id) == focused);
                    let enabled: Vec<bool> = rows.iter().map(|(_, on)| on.get()).collect();
                    match next_row(&enabled, from, key) {
                        Some(next) => {
                            ctx.request_focus(rows[next].0);
                            EventResponse::Handled
                        }
                        None => EventResponse::Ignored,
                    }
                }),
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

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::Menu);
        builder.set_name(self.label.resolve_now());
    }
}

#[cfg(test)]
mod tests;
