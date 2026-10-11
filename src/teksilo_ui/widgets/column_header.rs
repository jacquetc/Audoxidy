//! [`ColumnHeader`]: the library's sort bar, with resizable columns.
//!
//! Port of `build_sort_bar` (`src/gui/widgets.rs:1675-1850`) and of the
//! column-resize handling in `src/gui/app.rs` (`StartColumnResize`,
//! `ColumnHover`, `PlayerMouseMoved`, `GlobalMouseRelease`).
//!
//! The bar is 28 px high on the background colour, over a 2 px
//! `COLOR_CONTRAST` rule. Its columns ([`columns_for`]) sit in a row that
//! starts [`leading_padding`] px in and keeps 5 px at the end. Every column is
//! exactly its width from the view-model's widths ([`default_widths`]; a
//! column with no width is 100 px wide) and clips what overflows. A column
//! holds, from its leading edge:
//!
//! - the sort button, 5 px in: the label in Inter Medium 12 px, secondary,
//!   cut to `(width − 15) / 7` characters with "..." (`truncate_text`), and,
//!   on the sorted column, the 20 px up or down chevron in the secondary
//!   colour at its trailing end. The track-number column ("#") is right
//!   aligned instead, 3 px from the button's end. The album card and album
//!   thumbnail columns are inert;
//! - 3 px, the grip, 3 px. The grip is a 9 × 16 hit area whose visible part
//!   is a 1 × 8 px line at its centre (the Iced quad has a 4 px transparent
//!   border), in the secondary colour, in the accent while hovered or
//!   dragged. The album columns have 4 px of space instead.
//!
//! A click on a sort button moves the sort state one step through
//! `logic::library_sort`'s three-state cycle (another column: ascending; the
//! same column ascending: descending; descending: no sort) and hands the new
//! state to `on_sort`. A press on a grip captures the pointer; every move then
//! reports `max(30, start width + dx)` to `on_resize` until the release
//! (`app.rs:4672-4684`). The header never writes the signals itself: the
//! view-model applies the sort (and reloads) and stores the widths.
//!
//! The row scrolls sideways inside a `ScrollArea` with no visible bars: in the
//! Iced UI only the header scrolls horizontally (`widgets.rs:1911`).
//!
//! Accessibility and keyboard, which the Iced bar does not have:
//!
//! - every column is a column header named with its full label, carrying the
//!   sort direction of the sorted column; a sortable one takes the focus and
//!   sorts on Enter or Space;
//! - every grip is a focusable splitter whose value is the column width.
//!   Left and Right narrow and widen the column by 10 px (1 px with Shift),
//!   mirrored in a right-to-left layout; the Increment and Decrement actions
//!   and two named custom actions do the same for assistive technologies. Each
//!   such change is announced. This is the single-pointer alternative to the
//!   drag (WCAG 2.5.7).

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use teksilo::core::accesskit::{self, Role};
use teksilo::core::binding::BindingLevel;
use teksilo::core::environment::LayoutDirection;
use teksilo::core::event::PointerButton;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    FixedSize, HStack, IconWidget, Padding, RectWidget, ScrollArea, ScrollBarPolicy, Spacer,
    TextWidget, VStack,
};

use crate::logic::library_geometry::LibraryViewMode;
use crate::logic::library_sort::SortState;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;
use crate::utils::{SortColumn, truncate_text};

/// The view-model's column widths, in whole pixels as the Iced UI keeps them.
pub type ColumnWidths = HashMap<SortColumn, u16>;

/// The bar's height (`widgets.rs:1910`).
pub const HEIGHT: f32 = 28.0;
/// The rule under the bar (`widgets.rs:1914`).
pub const RULE_HEIGHT: f32 = 2.0;
/// The width of a column with no stored width (`widgets.rs:1700`).
pub const DEFAULT_WIDTH: u16 = 100;
/// The narrowest a drag makes a column (`app.rs:4682`).
pub const MIN_WIDTH: u16 = 30;
/// The keyboard step of a grip, and its step with Shift.
pub const KEY_STEP: u16 = 10;
/// The fine keyboard step of a grip (with Shift).
pub const FINE_STEP: u16 = 1;
/// The label size (`widgets.rs:1705`).
pub const LABEL_SIZE: f32 = 12.0;
/// The sort chevron's size (`widgets.rs:1722`).
pub const ARROW_SIZE: f32 = 20.0;
/// The grip's hit area (`widgets.rs:1737`).
pub const GRIP_WIDTH: f32 = 9.0;
/// The grip's hit area height (`widgets.rs:1738`).
pub const GRIP_HEIGHT: f32 = 16.0;
/// The grip's transparent border, around the visible line
/// (`widgets.rs:1748`).
pub const GRIP_INSET: f32 = 4.0;
/// The space on each side of the grip (`widgets.rs:1832`).
pub const GRIP_SPACE: f32 = 3.0;
/// What an album column has instead of a grip (`widgets.rs:1757`).
pub const INERT_GRIP_SPACE: f32 = 4.0;
/// The sort button's leading padding (`widgets.rs:1795`).
pub const LABEL_LEADING: f32 = 5.0;
/// The space after the "#" column's content (`widgets.rs:1775`).
pub const TRACK_TRAILING: f32 = 3.0;
/// The space after the last column (`widgets.rs:1690`).
pub const ROW_TRAILING: f32 = 5.0;
/// The width per character the label is cut to (`widgets.rs:1702`).
const CHAR_WIDTH: f32 = 7.0;

/// The width of `column` in `widths`, or [`DEFAULT_WIDTH`].
pub fn width_of(widths: &ColumnWidths, column: SortColumn) -> u16 {
    widths.get(&column).copied().unwrap_or(DEFAULT_WIDTH)
}

/// The Iced default widths (`library.rs:166-182`).
pub fn default_widths() -> ColumnWidths {
    use SortColumn::*;
    [
        (TrackNumber, 47),
        (Title, 275),
        (Artist, 155),
        (AlbumArtist, 135),
        (Album, 165),
        (Genre, 135),
        (Year, 50),
        (Duration, 53),
        (Format, 53),
        (SampleRate, 80),
        (Channels, 43),
        (BitDepth, 60),
        (Bitrate, 90),
        (Size, 85),
        (AlbumCard, 250),
        (AlbumThumbnail, 40),
    ]
    .into_iter()
    .collect()
}

/// The columns of a view mode, in order (`library.rs:1401-1452`).
pub fn columns_for(mode: LibraryViewMode) -> Vec<SortColumn> {
    use SortColumn::*;
    let tail = [
        Duration, Format, SampleRate, BitDepth, Bitrate, Channels, Size,
    ];
    let mut columns = match mode {
        LibraryViewMode::DetailedList => vec![AlbumCard, TrackNumber, Title, Artist, AlbumArtist],
        LibraryViewMode::ThumbnailList => vec![
            AlbumThumbnail,
            TrackNumber,
            Title,
            Artist,
            AlbumArtist,
            Album,
            Genre,
            Year,
        ],
        LibraryViewMode::Grid | LibraryViewMode::SimpleList => {
            vec![TrackNumber, Title, Artist, AlbumArtist, Album, Genre, Year]
        }
    };
    columns.extend(tail);
    columns
}

/// The space before the first column in a view mode (`library.rs:1454`).
pub fn leading_padding(mode: LibraryViewMode) -> f32 {
    match mode {
        LibraryViewMode::Grid => 19.0,
        LibraryViewMode::DetailedList | LibraryViewMode::SimpleList => 29.0,
        LibraryViewMode::ThumbnailList => 31.0,
    }
}

/// Whether a column sorts (the album card and thumbnail columns do not).
pub fn is_sortable(column: SortColumn) -> bool {
    !matches!(column, SortColumn::AlbumCard | SortColumn::AlbumThumbnail)
}

/// A column's label (`utils/mod.rs:71`). The track-number column is "#" and
/// the thumbnail column shows nothing.
pub fn column_label(column: SortColumn) -> LocalizedString {
    match column {
        SortColumn::TrackNumber => lit!("#"),
        SortColumn::Title => tr!(chrome_column_title()),
        SortColumn::Artist => tr!(chrome_column_artist()),
        SortColumn::AlbumArtist => tr!(chrome_column_album_artist()),
        SortColumn::Album => tr!(chrome_column_album()),
        SortColumn::Genre => tr!(chrome_column_genre()),
        SortColumn::Year => tr!(chrome_column_year()),
        SortColumn::Duration => tr!(chrome_column_duration()),
        SortColumn::Format => tr!(chrome_column_format()),
        SortColumn::SampleRate => tr!(chrome_column_sample_rate()),
        SortColumn::Channels => tr!(chrome_column_channels()),
        SortColumn::BitDepth => tr!(chrome_column_bit_depth()),
        SortColumn::Bitrate => tr!(chrome_column_bitrate()),
        SortColumn::Size => tr!(chrome_column_size()),
        SortColumn::AlbumCard => tr!(chrome_column_album_card()),
        SortColumn::AlbumThumbnail => lit!(""),
    }
}

/// A column's accessible name: its label, or "Cover" for the thumbnail
/// column, which shows none.
pub fn column_name(column: SortColumn) -> LocalizedString {
    match column {
        SortColumn::AlbumThumbnail => tr!(chrome_column_cover()),
        _ => column_label(column),
    }
}

/// `label` cut for a column `width` px wide (`widgets.rs:1701-1703`).
pub fn truncated_label(label: &str, width: f32) -> String {
    let available = width - (2.0 * GRIP_SPACE + GRIP_WIDTH);
    let max_chars = (available / CHAR_WIDTH).max(1.0) as usize;
    truncate_text(label, max_chars)
}

/// The width a drag that started at `start` and moved `dx` px gives
/// (`app.rs:4681-4682`): at least [`MIN_WIDTH`], cut to whole pixels.
pub fn resized_width(start: u16, dx: f32) -> u16 {
    (f32::from(start) + dx).max(f32::from(MIN_WIDTH)) as u16
}

/// The width a keyboard or assistive-technology step of `delta` px gives.
pub fn stepped_width(width: u16, delta: i32) -> u16 {
    let next = (i32::from(width) + delta).max(i32::from(MIN_WIDTH));
    u16::try_from(next).unwrap_or(u16::MAX)
}

/// The sort direction a screen reader is told for `column` under `sort`.
pub fn sort_direction(sort: &SortState, column: SortColumn) -> Option<accesskit::SortDirection> {
    if sort.column != Some(column) {
        return None;
    }
    match sort.ascending {
        Some(false) => Some(accesskit::SortDirection::Descending),
        _ => Some(accesskit::SortDirection::Ascending),
    }
}

/// The two chevrons the sorted column shows (`arrow-up-chevron.svg`,
/// `arrow-down-chevron.svg`), as `icons.rs` factories.
#[derive(Debug, Clone, Copy)]
pub struct SortArrows {
    /// Ascending.
    pub up: fn() -> IconWidget,
    /// Descending.
    pub down: fn() -> IconWidget,
}

/// The sort callback: the clicked column and the state after the click.
pub type SortFn = Rc<dyn Fn(SortColumn, SortState, &mut EventContext)>;
/// The resize callback: the column and its new width.
pub type ResizeFn = Rc<dyn Fn(SortColumn, u16, &mut EventContext)>;

/// The sort bar; see the module documentation.
pub struct ColumnHeader {
    columns: Vec<SortColumn>,
    widths: Signal<ColumnWidths>,
    sort: Signal<SortState>,
    arrows: SortArrows,
    leading: f32,
    on_sort: Option<SortFn>,
    on_resize: Option<ResizeFn>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ColumnHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnHeader")
            .field("columns", &self.columns)
            .field("leading", &self.leading)
            .finish_non_exhaustive()
    }
}

impl ColumnHeader {
    /// A bar of `columns`, sized from `widths`, showing `sort`.
    pub fn new(
        columns: Vec<SortColumn>,
        widths: Signal<ColumnWidths>,
        sort: Signal<SortState>,
        arrows: SortArrows,
    ) -> Self {
        Self {
            columns,
            widths,
            sort,
            arrows,
            leading: 0.0,
            on_sort: None,
            on_resize: None,
            root: None,
        }
    }

    /// The space before the first column ([`leading_padding`] of the mode).
    pub fn leading_padding(mut self, padding: f32) -> Self {
        self.leading = padding;
        self
    }

    /// Called with the clicked column and the sort state after the click.
    pub fn on_sort(
        mut self,
        f: impl Fn(SortColumn, SortState, &mut EventContext) + 'static,
    ) -> Self {
        self.on_sort = Some(Rc::new(f));
        self
    }

    /// Called with a column and its new width while a grip is dragged or
    /// stepped.
    pub fn on_resize(mut self, f: impl Fn(SortColumn, u16, &mut EventContext) + 'static) -> Self {
        self.on_resize = Some(Rc::new(f));
        self
    }

    /// One column: the sort button and the grip, `width` px wide.
    fn column(&self, column: SortColumn, ctx: &mut BuildContext) -> WidgetId {
        let width = self
            .widths
            .map(move |widths| f32::from(width_of(widths, column)));
        let sortable = is_sortable(column);
        let reserve = if sortable {
            2.0 * GRIP_SPACE + GRIP_WIDTH
        } else {
            2.0 * GRIP_SPACE + INERT_GRIP_SPACE
        };
        let button_width = width.map(move |width| (width - reserve).max(0.0));
        let button = self.sort_button(column, button_width);
        let grip: Box<dyn Widget> = if sortable {
            Box::new(ColumnGrip::new(
                column,
                self.widths.clone(),
                self.on_resize.clone(),
            ))
        } else {
            Box::new(gap(INERT_GRIP_SPACE))
        };
        let grip = ctx.add_boxed(grip);
        ctx.add(
            FixedSize::new()
                .width(width)
                .height(HEIGHT)
                .child(
                    HStack::new()
                        .spacing(0.0)
                        .alignment(VAlignment::Center)
                        .child(button)
                        .child(gap(GRIP_SPACE))
                        .child(grip)
                        .child(gap(GRIP_SPACE)),
                )
                .clips_children_on(true),
        )
    }

    /// The label and chevron of `column`, `width` px wide.
    fn sort_button(&self, column: SortColumn, width: Signal<f32>) -> Box<dyn Widget> {
        let label = column_label(column);
        let text = match Prop::<String>::from(label) {
            Prop::Bound(signal) => signal,
            Prop::Static(text) => Signal::new(text),
        };
        let cell_width = self
            .widths
            .map(move |widths| f32::from(width_of(widths, column)));
        let shown = text
            .zip(&cell_width)
            .map(|(text, width)| truncated_label(text, *width));
        let label = TextWidget::new(lit!(""))
            .text(shown)
            .style(style::inter(LABEL_SIZE, FontWeight::MEDIUM))
            .color(TextRole::Secondary)
            .single_line()
            .no_shrink()
            .a11y_hidden();
        let up = self
            .sort
            .map(move |sort| sort.column == Some(column) && sort.ascending == Some(true));
        let down = self
            .sort
            .map(move |sort| sort.column == Some(column) && sort.ascending == Some(false));
        let arrow = |icon: fn() -> IconWidget, shown: Signal<bool>| {
            (icon)()
                .icon_size(ARROW_SIZE)
                .color(TextRole::Secondary)
                .visible_when(shown)
        };
        let (up, down) = (arrow(self.arrows.up, up), arrow(self.arrows.down, down));
        let content = if column == SortColumn::TrackNumber {
            HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Center)
                .child(Spacer::new())
                .child(label)
                .child(up)
                .child(down)
                .child(gap(TRACK_TRAILING))
        } else {
            HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Center)
                .child(label)
                .child(Spacer::new())
                .child(up)
                .child(down)
        };
        let body = FixedSize::new()
            .width(width)
            .child(Padding::new(0.0, 0.0, 0.0, LABEL_LEADING).child(content));
        let name = column_name(column);
        if !is_sortable(column) {
            return Box::new(body.access_role(Role::ColumnHeader).access_label(name));
        }
        let sort = self.sort.clone();
        let on_sort = self.on_sort.clone();
        let direction = self.sort.clone();
        Box::new(
            Pressable::new(body, name)
                .on_activate(move |ctx| {
                    if let Some(on_sort) = &on_sort {
                        on_sort(column, sort.get().after_header_click(column), ctx);
                    }
                })
                .access_role(Role::ColumnHeader)
                .access_customize(move |builder| {
                    if let Some(direction) = sort_direction(&direction.get(), column) {
                        builder.inner_mut().set_sort_direction(direction);
                    }
                }),
        )
    }
}

impl Widget for ColumnHeader {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let self_id = ctx.self_id();
        self.sort.bind_to(
            self_id,
            ctx.binding_registry(),
            BindingLevel::AccessibilityOnly,
        );
        let mut row = HStack::new().spacing(0.0).alignment(VAlignment::Center);
        for column in self.columns.clone() {
            row = row.child(self.column(column, ctx));
        }
        let bar = ScrollArea::new()
            .horizontal_scroll_bar_policy(ScrollBarPolicy::AlwaysOff)
            .vertical_scroll_bar_policy(ScrollBarPolicy::AlwaysOff)
            .child(Padding::new(0.0, ROW_TRAILING, 0.0, self.leading).child(row));
        let root = ctx.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(
                    FixedSize::new().height(HEIGHT).child(
                        teksilo::widgets::ZStack::new()
                            .child(RectWidget::new().background(SurfaceRole::Main))
                            .child(bar),
                    ),
                )
                .child(
                    FixedSize::new()
                        .height(RULE_HEIGHT)
                        .child(RectWidget::new().background(SurfaceRole::Raised)),
                )
                .access_role(Role::Row)
                .access_label(tr!(chrome_column_headers())),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, HEIGHT + RULE_HEIGHT))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// A fixed horizontal gap.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

// ---------------------------------------------------------------------------
// The grip
// ---------------------------------------------------------------------------

/// A column's resize grip; see the module documentation.
struct ColumnGrip {
    column: SortColumn,
    widths: Signal<ColumnWidths>,
    on_resize: Option<ResizeFn>,
    hovered: Signal<bool>,
    dragging: Signal<bool>,
    focused: Signal<bool>,
    focus_visible: Option<Signal<bool>>,
    /// The pointer's x and the column width when the drag started.
    drag: Rc<Cell<Option<(f32, u16)>>>,
}

impl std::fmt::Debug for ColumnGrip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnGrip")
            .field("column", &self.column)
            .finish_non_exhaustive()
    }
}

/// Everything a grip handler needs.
#[derive(Clone)]
struct GripTarget {
    column: SortColumn,
    widths: Signal<ColumnWidths>,
    on_resize: Option<ResizeFn>,
}

impl GripTarget {
    fn width(&self) -> u16 {
        width_of(&self.widths.get(), self.column)
    }

    fn resize(&self, width: u16, ctx: &mut EventContext) {
        if let Some(on_resize) = &self.on_resize {
            on_resize(self.column, width, ctx);
        }
    }

    /// A keyboard or assistive-technology step, announced.
    fn step(&self, delta: i32, ctx: &mut EventContext) {
        let width = stepped_width(self.width(), delta);
        if width == self.width() {
            return;
        }
        self.resize(width, ctx);
        let column = column_name(self.column).resolve_now();
        ctx.announce(tr!(chrome_column_width(
            column = column,
            width = i64::from(width)
        )));
    }
}

impl ColumnGrip {
    fn new(column: SortColumn, widths: Signal<ColumnWidths>, on_resize: Option<ResizeFn>) -> Self {
        Self {
            column,
            widths,
            on_resize,
            hovered: Signal::new(false),
            dragging: Signal::new(false),
            focused: Signal::new(false),
            focus_visible: None,
            drag: Rc::new(Cell::new(None)),
        }
    }

    fn target(&self) -> GripTarget {
        GripTarget {
            column: self.column,
            widths: self.widths.clone(),
            on_resize: self.on_resize.clone(),
        }
    }

    /// Whether the line is lit: hovered, dragged, or focused from the
    /// keyboard.
    fn lit(&self) -> bool {
        let focus_visible = self.focus_visible.as_ref().is_some_and(Signal::get);
        self.hovered.get() || self.dragging.get() || (self.focused.get() && focus_visible)
    }
}

/// The key step of `key` with `shift`, in a layout running `direction`.
pub fn key_delta(key: &Key, shift: bool, direction: LayoutDirection) -> Option<i32> {
    let step = i32::from(if shift { FINE_STEP } else { KEY_STEP });
    let wider = match key {
        Key::ArrowRight => true,
        Key::ArrowLeft => false,
        _ => return None,
    };
    let wider = wider != (direction == LayoutDirection::RightToLeft);
    Some(if wider { step } else { -step })
}

impl Widget for ColumnGrip {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let id = ctx.self_id();
        let registry = ctx.binding_registry();
        for signal in [&self.hovered, &self.dragging, &self.focused] {
            signal.bind_to(id, registry, BindingLevel::RepaintOnly);
        }
        self.widths
            .bind_to(id, registry, BindingLevel::AccessibilityOnly);
        let focus_visible = ctx.focus_visible();
        focus_visible.bind_to(id, ctx.binding_registry(), BindingLevel::RepaintOnly);
        self.focus_visible = Some(focus_visible);

        let (pointer, key, access) = (self.target(), self.target(), self.target());
        let (widen, narrow) = (self.target(), self.target());
        let drag = self.drag.clone();
        let cancel_drag = self.drag.clone();
        let dragging = self.dragging.clone();
        let cancel_dragging = self.dragging.clone();
        let hovered = self.hovered.clone();
        let focused = self.focused.clone();
        ctx.apply_self_handlers(
            HandlerSet::new()
                .focusable(true)
                .cursor(CursorIcon::ColResize)
                .on_hover(move |entered, _| hovered.set(entered))
                .on_focus(move |gained, _| focused.set(gained))
                .on_pointer_event(move |event, ctx| match event {
                    WidgetEvent::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        ..
                    } => {
                        ctx.capture_pointer();
                        drag.set(Some((position.x, pointer.width())));
                        dragging.set(true);
                        EventResponse::Handled
                    }
                    WidgetEvent::PointerMove { position, .. } => match drag.get() {
                        Some((start_x, start_width)) if ctx.owns_pointer() => {
                            pointer.resize(resized_width(start_width, position.x - start_x), ctx);
                            EventResponse::Handled
                        }
                        _ => EventResponse::Ignored,
                    },
                    WidgetEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    } if drag.get().is_some() => {
                        drag.set(None);
                        dragging.set(false);
                        ctx.release_pointer();
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                })
                .on_pointer_cancel(move |_, _, _| {
                    cancel_drag.set(None);
                    cancel_dragging.set(false);
                })
                .on_key(move |event, ctx| match event {
                    WidgetEvent::KeyDown {
                        key: k, modifiers, ..
                    } => match key_delta(k, modifiers.shift(), ctx.layout_direction()) {
                        Some(delta) => {
                            key.step(delta, ctx);
                            EventResponse::Handled
                        }
                        None => EventResponse::Ignored,
                    },
                    _ => EventResponse::Ignored,
                })
                .on_access_action(move |action, ctx| match action {
                    accesskit::Action::Increment => {
                        access.step(i32::from(KEY_STEP), ctx);
                        EventResponse::Handled
                    }
                    accesskit::Action::Decrement => {
                        access.step(-i32::from(KEY_STEP), ctx);
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                })
                .access_custom_action(tr!(chrome_column_widen()), move |ctx| {
                    widen.step(i32::from(KEY_STEP), ctx)
                })
                .access_custom_action(tr!(chrome_column_narrow()), move |ctx| {
                    narrow.step(-i32::from(KEY_STEP), ctx)
                }),
        );
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(GRIP_WIDTH, GRIP_HEIGHT).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let color: ColorProp = if self.lit() {
            SurfaceRole::Accent.into()
        } else {
            TextRole::Secondary.into()
        };
        canvas.fill_rect(
            Rect::new(
                bounds.x + GRIP_INSET,
                bounds.y + GRIP_INSET,
                (bounds.width - 2.0 * GRIP_INSET).max(0.0),
                (bounds.height - 2.0 * GRIP_INSET).max(0.0),
            ),
            color.resolve(ctx.theme, ctx.effective_enabled),
        );
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::Splitter);
        let column = column_name(self.column).resolve_now();
        builder.set_name(tr!(chrome_column_resize(column = column)).resolve_now());
        builder.set_orientation(accesskit::Orientation::Vertical);
        builder.set_numeric_value(f64::from(width_of(&self.widths.get(), self.column)));
        builder.set_min_numeric_value(f64::from(MIN_WIDTH));
        builder.set_numeric_value_step(f64::from(KEY_STEP));
        builder.add_action(accesskit::Action::Increment);
        builder.add_action(accesskit::Action::Decrement);
    }
}

#[cfg(test)]
mod tests;
