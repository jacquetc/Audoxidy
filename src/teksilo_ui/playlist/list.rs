//! The list: the folder separators and the songs of the active playlist.
//!
//! Port of `build_song_list` (`src/gui/playlist.rs:674`) and its rows:
//!
//! - a `ListView` over [`PlaylistRowsSource`], 46 px rows, its scroll bar
//!   floating over the rows (Iced's `standard_scrollbar`), whose own
//!   selection is off: the view-model's selection lights the rows up;
//! - the rows are 9 px narrower than the list (the column's right padding,
//!   `src/gui/playlist.rs:735`), so the bar does not cover them;
//! - a **separator** (`build_folder_separator`, `:813`): 10 px in, a "•"
//!   (Inter 19 px, secondary while a song of the group is enabled) in a 12 px
//!   column that enables or disables the group, 5 px, the folder's name
//!   (Inter Medium 14 px, cut with "..." to the room it has), then
//!   "count | duration" (Inter Medium 13 px), all secondary, on 35 px; then a
//!   1 px line, primary while selected, secondary otherwise, and 10 px of
//!   space. `COLOR_CONTRAST` behind it while selected;
//! - a **song** (`build_song_row`, `:900`): 15 px in, a "•" (Inter 17 px) in
//!   a 12 px column that enables or disables the song, invisible while it is
//!   disabled; then two lines 2 px apart: the track number (or, without one,
//!   the song's position in its group, two digits, quirk fix 57) and a dot
//!   right-aligned in 20 px, 5 px, the title, 5 px, the duration; under it,
//!   25 px in, "artist - album :: year".
//!   Inter Medium 13 px, primary while the song plays, secondary otherwise.
//!   `COLOR_CONTRAST` behind it while it plays, is selected, or its
//!   separator is selected;
//! - with no song at all, "No hay canciones en esta lista" (Inter Medium
//!   16 px, secondary) centred.
//!
//! A click on a row selects it (Ctrl adds, Shift extends) and a second click
//! within 400 ms plays the song or folds the group; a click on a row's "•"
//! enables or disables it and leaves the selection; a right-click opens the
//! row's menu, selecting the row first if it is not in the selection. A
//! click on the background clears the selection, a right-click there opens
//! the "add" menu. The list's container decides what a press lands on
//! ([`hit`]), from the press's position and the list's scroll offset: the
//! `ListView` keeps the pointer events of its rows to itself.
//!
//! The keys act on the list while it has the keyboard focus (quirk fix 2):
//! Up and Down move (Shift extends), Left folds and Right unfolds the
//! selected groups, Enter plays, Delete removes, Space enables or disables.
//! They are taken in a preview handler on the list's container, so they
//! never reach the `ListView`'s own navigation, and never a text field, a
//! menu or a dialog (which are not inside the list). Ctrl+A is the app's
//! (`PlaylistViewModel::select_all`).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use teksilo::core::accesskit::{Action, Role};
use teksilo::core::binding::BindingLevel;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, Modifiers, WidgetEvent};
use teksilo::data::{SelectionMode, SelectionModel};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, ListView, Padding, RectWidget, ScrollBarMode, Spacer,
    TextWidget, VStack, ZStack,
};

use super::menus;
use super::playlist_vm::PlaylistViewModel;
use super::rows::{
    PlaylistRowsSource, ROW_HEIGHT, RowItem, SeparatorItem, SongItem, scroll_offset_for,
};
use crate::logic::playlist_nav::PlaylistSelection;
use crate::logic::selection_modifiers::SelectModifiers;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::MenuEntry;
use crate::teksilo_ui::widgets::menu::context_menu;
use crate::utils::{format_duration, truncate_text};

/// The list column's right padding, which keeps the rows clear of the
/// scroll bar (`src/gui/playlist.rs:735`).
pub const RIGHT_GUTTER: f32 = 9.0;
/// A separator's leading padding (`src/gui/playlist.rs:884`).
pub const SEPARATOR_LEADING: f32 = 10.0;
/// A song row's leading padding (`src/gui/playlist.rs:1018`).
pub const SONG_LEADING: f32 = 15.0;
/// The trailing padding of both rows (`src/gui/playlist.rs:882`, `:1016`).
pub const ROW_TRAILING: f32 = 4.0;
/// The width of the "•" column (`src/gui/playlist.rs:850`, `:1001`).
pub const INDICATOR_WIDTH: f32 = 12.0;
/// The height of a separator's text line (`FOLDER_SEPARATOR_HEIGHT - 11`,
/// `src/gui/playlist.rs:867`).
pub const SEPARATOR_LINE: f32 = 35.0;
/// The space under a separator's rule (`src/gui/playlist.rs:878`).
pub const SEPARATOR_BOTTOM: f32 = 10.0;
/// The width of a song's number (`prefix_width`, `src/gui/playlist.rs:938`).
pub const PREFIX_WIDTH: f32 = 20.0;
/// The space after it (`prefix_spacing`, `src/gui/playlist.rs:939`).
pub const PREFIX_SPACING: f32 = 5.0;
/// The space between a song's two lines (`src/gui/playlist.rs:986`).
pub const LINE_SPACING: f32 = 2.0;

/// The text a smart-truncated label shows in `width` px at `size` px: whole
/// when it fits by Iced's estimate of 0.55 × size per character, else cut to
/// the characters that fit with "..." (`smart_truncate_text_advanced`,
/// `src/gui/widgets.rs:926`, and `utils::truncate_text`).
pub fn smart_truncate(text: &str, size: f32, width: f32) -> String {
    let char_width = size * 0.55;
    let needed = text.chars().count() as f32 * char_width;
    if needed <= width {
        text.to_string()
    } else {
        let max_chars = (width.max(0.0) / char_width).floor() as usize;
        truncate_text(text, max_chars)
    }
}

/// The track prefix of a song row: its track number and a dot, or, when it
/// has none, `number` on two digits: its position among the songs of its
/// group in the whole playlist ([`SongItem::number`], quirk fix 57). Iced
/// shows the row's linear index there, which counts the separators
/// (`src/gui/playlist.rs:931-935`).
pub fn track_prefix(song: &crate::db::database::PlaylistSongRef, number: usize) -> String {
    match &song.track_number {
        Some(track) => format!("{track}."),
        None => format!("{number:02}."),
    }
}

/// The second line of a song row: "artist - album :: year", "-" for no year
/// (`src/gui/playlist.rs:969`).
pub fn song_details(song: &crate::db::database::PlaylistSongRef) -> String {
    let year = song.year.as_deref().unwrap_or("-");
    format!("{} - {} :: {}", song.artist_name, song.album_title, year)
}

/// A separator's statistics: "count | duration" (`src/gui/playlist.rs:836`).
pub fn separator_stats(item: &SeparatorItem) -> String {
    format!(
        "{} | {}",
        item.song_count,
        format_duration(item.total_duration)
    )
}

// ---------------------------------------------------------------------------
// SmartText
// ---------------------------------------------------------------------------

/// Iced's `smart_truncate_text`: a single line in Inter Medium that takes the
/// room its row leaves it and is cut with "..." by Iced's estimate
/// ([`smart_truncate`]), drawn in the primary colour while `lit` holds and in
/// the secondary colour otherwise. Its line is `size + 2` px high.
pub struct SmartText {
    text: String,
    size: f32,
    lit: Option<Signal<bool>>,
}

impl std::fmt::Debug for SmartText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmartText")
            .field("text", &self.text)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl SmartText {
    /// `text` at `size` px, secondary.
    pub fn new(text: impl Into<String>, size: f32) -> Self {
        Self {
            text: text.into(),
            size,
            lit: None,
        }
    }

    /// Primary while `lit` holds.
    pub fn lit(mut self, lit: Signal<bool>) -> Self {
        self.lit = Some(lit);
        self
    }

    /// The full text.
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Widget for SmartText {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        if let Some(lit) = &self.lit {
            lit.bind_to(
                ctx.self_id(),
                ctx.binding_registry(),
                BindingLevel::RepaintOnly,
            );
        }
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        // `Length::Fill`: the line takes the room the row leaves.
        LayoutResponse::flexible(Size::new(0.0, self.size + 2.0), 1.0)
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let role = if self.lit.as_ref().is_some_and(Signal::get) {
            TextRole::Primary
        } else {
            TextRole::Secondary
        };
        let color = ColorProp::from(role).resolve(ctx.theme, ctx.effective_enabled);
        let shown = smart_truncate(&self.text, self.size, bounds.width);
        canvas.draw_text(
            &shown,
            bounds,
            &style::inter(self.size, FontWeight::MEDIUM),
            color,
        );
    }
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/// The row fill: `COLOR_CONTRAST` while `lit`, nothing otherwise.
pub fn row_fill(lit: bool) -> SurfaceRole {
    if lit {
        SurfaceRole::Raised
    } else {
        SurfaceRole::Transparent
    }
}

/// The text role of a line that is primary while `lit`.
pub fn text_role(lit: bool) -> TextRole {
    if lit {
        TextRole::Primary
    } else {
        TextRole::Secondary
    }
}

/// Whether a song row is lit: it plays, is selected, or its separator is
/// selected (`src/gui/playlist.rs:990-996`).
pub fn song_lit(selection: &PlaylistSelection, playing: Option<usize>, song: &SongItem) -> bool {
    selection.selected.contains(&song.linear_idx)
        || playing == Some(song.linear_idx)
        || selection.selected.contains(&song.separator_idx)
}

/// The selection modifiers of a click with `modifiers`: the platform
/// accelerator toggles, Shift extends (`select_modifiers`,
/// `src/gui/playlist.rs:132`).
pub fn select_modifiers(modifiers: Modifiers) -> SelectModifiers {
    SelectModifiers {
        toggle: modifiers.command(),
        range: modifiers.shift(),
    }
}

/// A "•" column 12 px wide (the press on it is the list's, see [`hit`]).
fn indicator(height: f32, size: f32, color: Option<Signal<TextRole>>) -> impl Widget {
    let center = match color {
        Some(color) => Center::new().child(
            TextWidget::new(lit!("•"))
                .style(style::inter(size, FontWeight::REGULAR))
                .color(color)
                .single_line()
                .no_shrink()
                .a11y_hidden(),
        ),
        None => Center::new().child(Spacer::new()),
    };
    FixedSize::new()
        .width(INDICATOR_WIDTH)
        .height(height)
        .child(center)
}

/// What a press at `x`, `y` in the list (its own coordinates, `scroll_y`
/// scrolled) lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListHit {
    /// The row of this linear index.
    Row(usize),
    /// The "•" of the song at this linear index
    /// (`ToggleSongEnabled`).
    SongDot(usize),
    /// The "•" of the separator at this linear index
    /// (`ToggleGroupEnabled`).
    GroupDot(usize),
    /// Below the last row.
    Background,
}

/// What a press at `x`, `y` in a list of `rows` scrolled to `scroll_y`
/// lands on. A separator's "•" is 12 px wide, 10 px in, on its 35 px line;
/// a song's is 12 px wide, 15 px in, the row's height.
pub fn hit(rows: &[RowItem], x: f32, y: f32, scroll_y: f32) -> ListHit {
    let content_y = y + scroll_y;
    if content_y < 0.0 {
        return ListHit::Background;
    }
    let index = (content_y / ROW_HEIGHT).floor() as usize;
    let Some(row) = rows.get(index) else {
        return ListHit::Background;
    };
    let in_row = content_y - index as f32 * ROW_HEIGHT;
    match row {
        RowItem::Separator(item) => {
            let on_dot = (SEPARATOR_LEADING..SEPARATOR_LEADING + INDICATOR_WIDTH).contains(&x)
                && in_row < SEPARATOR_LINE;
            if on_dot {
                ListHit::GroupDot(item.linear_idx)
            } else {
                ListHit::Row(item.linear_idx)
            }
        }
        RowItem::Song(item) => {
            if (SONG_LEADING..SONG_LEADING + INDICATOR_WIDTH).contains(&x) {
                ListHit::SongDot(item.linear_idx)
            } else {
                ListHit::Row(item.linear_idx)
            }
        }
    }
}

/// One row of the list; see the module documentation.
pub struct PlaylistRow {
    item: RowItem,
    vm: PlaylistViewModel,
    selected: Option<Signal<bool>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for PlaylistRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistRow")
            .field("linear_idx", &self.item.linear_idx())
            .finish_non_exhaustive()
    }
}

impl PlaylistRow {
    /// The row of `item`.
    pub fn new(item: RowItem, vm: PlaylistViewModel) -> Self {
        Self {
            item,
            vm,
            selected: None,
            root: None,
        }
    }

    /// The row's accessible name: what it shows.
    pub fn label(&self) -> String {
        match &self.item {
            RowItem::Separator(item) => {
                format!("{}, {}", item.folder_name, separator_stats(item))
            }
            RowItem::Song(item) => format!(
                "{} {}, {}, {}",
                track_prefix(&item.song, item.number),
                item.song.title,
                song_details(&item.song),
                format_duration(item.song.duration)
            ),
        }
    }

    fn separator(&mut self, item: &SeparatorItem, ctx: &mut BuildContext) -> WidgetId {
        let linear_idx = item.linear_idx;
        let selected = self
            .vm
            .selection()
            .map(move |selection| selection.selected.contains(&linear_idx));
        let dot = item.any_enabled.then(|| ctx.signal(TextRole::Secondary));
        let line = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(indicator(SEPARATOR_LINE, 19.0, dot))
            .child(gap(5.0))
            .child(SmartText::new(item.folder_name.to_string(), 14.0))
            .child(
                TextWidget::new(lit!(separator_stats(item)))
                    .style(style::inter(13.0, FontWeight::MEDIUM))
                    .color(TextRole::Secondary)
                    .single_line()
                    .no_shrink()
                    .a11y_hidden(),
            );
        let column = VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(Expand::horizontal().child(FixedSize::new().height(SEPARATOR_LINE).child(line)))
            .child(
                Expand::horizontal().child(
                    FixedSize::new()
                        .height(1.0)
                        .child(RectWidget::new().background(selected.map(|s| text_role(*s)))),
                ),
            )
            .child(gap_v(SEPARATOR_BOTTOM));
        self.selected = Some(selected.clone());
        ctx.add(
            ZStack::new()
                .child(RectWidget::new().background(selected.map(|s| row_fill(*s))))
                .child(
                    Padding::new(0.0, ROW_TRAILING, 0.0, SEPARATOR_LEADING)
                        .child(Expand::horizontal().child(column)),
                ),
        )
    }

    fn song(&mut self, item: &SongItem, ctx: &mut BuildContext) -> WidgetId {
        let linear_idx = item.linear_idx;
        let lit_item = item.clone();
        let selection = self.vm.selection();
        let playing_index = self.vm.playing_index();
        let playing = playing_index.map(move |playing| *playing == Some(linear_idx));
        let lit = selection
            .zip(&playing_index)
            .map(move |(selection, playing)| song_lit(selection, *playing, &lit_item));
        let selected = selection.map(move |selection| selection.selected.contains(&linear_idx));
        let dot = item.song.enabled.then(|| playing.map(|p| text_role(*p)));
        let song = &item.song;
        let line_text = |text: String| {
            TextWidget::new(lit!(text))
                .style(style::inter(13.0, FontWeight::MEDIUM))
                .color(playing.map(|p| text_role(*p)))
                .single_line()
                .no_shrink()
                .a11y_hidden()
        };
        let first = HStack::new()
            .spacing(PREFIX_SPACING)
            .alignment(VAlignment::Center)
            .child(
                FixedSize::new().width(PREFIX_WIDTH).child(
                    HStack::new()
                        .spacing(0.0)
                        .alignment(VAlignment::Center)
                        .child(Spacer::new())
                        .child(line_text(track_prefix(song, item.number))),
                ),
            )
            .child(SmartText::new(song.title.to_string(), 13.0).lit(playing.clone()))
            .child(line_text(format_duration(song.duration)));
        let second = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(gap(PREFIX_WIDTH + PREFIX_SPACING))
            .child(SmartText::new(song_details(song), 13.0).lit(playing.clone()));
        // Centred in the row: the spacers share the slack (and one spacing
        // each), the two lines stay 2 px apart.
        let lines = VStack::new()
            .spacing(LINE_SPACING)
            .alignment(HAlignment::Leading)
            .child(Spacer::new())
            .child(Expand::horizontal().child(first))
            .child(Expand::horizontal().child(second))
            .child(Spacer::new());
        let content = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(indicator(ROW_HEIGHT, 17.0, dot))
            .child(Expand::horizontal().child(lines));
        self.selected = Some(selected);
        ctx.add(
            ZStack::new()
                .child(RectWidget::new().background(lit.map(|lit| row_fill(*lit))))
                .child(Padding::new(0.0, ROW_TRAILING, 0.0, SONG_LEADING).child(content)),
        )
    }
}

/// A fixed horizontal gap.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

/// A fixed vertical gap.
fn gap_v(height: f32) -> FixedSize {
    FixedSize::new()
        .width(0.0)
        .height(height)
        .child(Spacer::new())
}

impl Widget for PlaylistRow {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let item = self.item.clone();
        let body = match &item {
            RowItem::Separator(separator) => self.separator(separator, ctx),
            RowItem::Song(song) => self.song(song, ctx),
        };
        if let Some(selected) = &self.selected {
            selected.bind_to(
                ctx.self_id(),
                ctx.binding_registry(),
                BindingLevel::RepaintOnly,
            );
            // The row reports its selection to assistive technology: a change
            // that no event marks (Ctrl+A, a search remapping the selection)
            // must still reach the accessibility tree.
            selected.bind_to(
                ctx.self_id(),
                ctx.binding_registry(),
                BindingLevel::AccessibilityOnly,
            );
        }
        // Its description says whether it plays.
        self.vm.playing_index().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::AccessibilityOnly,
        );
        let linear_idx = item.linear_idx();
        let click = self.vm.clone();
        // The assistive-technology click selects the row, as a click does.
        ctx.apply_self_handlers(HandlerSet::new().on_access_action(move |action, _ctx| {
            if action == Action::Click {
                click.click(linear_idx, SelectModifiers::default(), Instant::now());
                EventResponse::Handled
            } else {
                EventResponse::Ignored
            }
        }));
        let root = ctx
            .add(Padding::new(0.0, RIGHT_GUTTER, 0.0, 0.0).child(
                Expand::horizontal().child(FixedSize::new().height(ROW_HEIGHT).child(body)),
            ));
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, ROW_HEIGHT))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::ListItem);
        builder.set_name(self.label());
        builder.set_selected(self.selected.as_ref().is_some_and(Signal::get));
        if let RowItem::Song(item) = &self.item {
            if !item.song.enabled {
                builder.set_description(tr!(playlist_row_disabled()).resolve_now());
            } else if self.vm.playing_index().get() == Some(item.linear_idx) {
                builder.set_description(tr!(playlist_row_playing()).resolve_now());
            }
        }
        builder.add_action(Action::Click);
    }
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// The list region of the panel; see the module documentation.
pub struct PlaylistList {
    vm: PlaylistViewModel,
    viewport: Rc<Cell<f32>>,
    bounds: Rc<Cell<Rect>>,
    list: Rc<Cell<Option<WidgetId>>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for PlaylistList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaylistList")
            .field("viewport", &self.viewport.get())
            .finish_non_exhaustive()
    }
}

impl PlaylistList {
    /// The list of `vm`.
    pub fn new(vm: PlaylistViewModel) -> Self {
        Self::with_list_handle(vm, Rc::new(Cell::new(None)))
    }

    /// The list of `vm`, writing the `ListView`'s id into `list` when it is
    /// built.
    pub fn with_list_handle(vm: PlaylistViewModel, list: Rc<Cell<Option<WidgetId>>>) -> Self {
        Self {
            vm,
            viewport: Rc::new(Cell::new(0.0)),
            bounds: Rc::new(Cell::new(Rect::new(0.0, 0.0, 0.0, 0.0))),
            list,
            root: None,
        }
    }

    /// A handle on the `ListView`'s id, set when the list is built.
    pub fn list_id(&self) -> Rc<Cell<Option<WidgetId>>> {
        self.list.clone()
    }

    /// What a key does to the list (quirk fix 2): `true` when the key is the
    /// list's.
    pub fn handle_key(vm: &PlaylistViewModel, key: &Key, modifiers: Modifiers) -> bool {
        match key {
            Key::ArrowUp => vm.key_move(false, modifiers.shift()),
            Key::ArrowDown => vm.key_move(true, modifiers.shift()),
            Key::ArrowLeft => vm.collapse_selected(),
            Key::ArrowRight => vm.expand_selected(),
            Key::Enter => vm.activate_focused(),
            Key::Delete => vm.remove_focused(),
            Key::Space => vm.toggle_enabled_focused(),
            _ => return false,
        }
        true
    }

    /// A click at `x`, `y` in the list (`PlaySongIndex`,
    /// `TogglePlaylistFolder`, `ToggleSongEnabled`, `ToggleGroupEnabled`,
    /// `PlaylistDeselect`).
    pub fn handle_click(vm: &PlaylistViewModel, hit: ListHit, modifiers: Modifiers) {
        match hit {
            ListHit::Row(linear_idx) => {
                vm.click(linear_idx, select_modifiers(modifiers), Instant::now());
            }
            ListHit::SongDot(linear_idx) => vm.toggle_song_enabled(linear_idx),
            ListHit::GroupDot(linear_idx) => vm.toggle_group_enabled(linear_idx),
            ListHit::Background => vm.deselect(),
        }
    }

    /// The menu for a right-click on `hit`: the row's, selecting it first
    /// when it is not in the selection, or the background's.
    pub fn menu_for(vm: &PlaylistViewModel, hit: ListHit) -> Vec<MenuEntry> {
        match hit {
            ListHit::Row(linear_idx)
            | ListHit::SongDot(linear_idx)
            | ListHit::GroupDot(linear_idx) => {
                vm.menu_opened_on(linear_idx);
                menus::item_menu(vm, linear_idx)
            }
            ListHit::Background => menus::empty_list_menu(vm),
        }
    }
}

impl Widget for PlaylistList {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        let rows = vm.rows();
        let source = PlaylistRowsSource::new(rows.clone());
        let row_vm = vm.clone();
        let view = ListView::from_source(source, move |_index, item: &RowItem, _selected| {
            Box::new(PlaylistRow::new(item.clone(), row_vm.clone())) as Box<dyn Widget>
        })
        .item_height(ROW_HEIGHT)
        .selection(SelectionModel::new(SelectionMode::None))
        .smooth_scrolling(false)
        .scroll_bar_style(ScrollBarMode::Overlay);
        let scroll_y = view.scroll_y_signal().clone();
        let list = ctx.add(view.access_label(tr!(playlist_list())));
        self.list.set(Some(list));

        let empty = ctx.add(
            Padding::uniform(15.0).child(
                Center::new().child(
                    TextWidget::new(tr!(playlist_empty()))
                        .style(style::inter(16.0, FontWeight::MEDIUM))
                        .color(TextRole::Secondary),
                ),
            ),
        );
        let is_empty = rows.map(|rows| rows.is_empty());
        ctx.visible_when(list, Prop::Bound(is_empty.not()));
        ctx.visible_when(empty, Prop::Bound(is_empty));

        // The view-model's scroll requests (the Iced autoscroll).
        let (scroll_vm, viewport, scroll) = (vm.clone(), self.viewport.clone(), scroll_y.clone());
        ctx.effect(&vm.scroll_request(), move |request| {
            let Some((_, target)) = *request else {
                return;
            };
            let collapsed = scroll_vm.collapsed();
            let offset = scroll_vm.read_cursor(|cursor| {
                scroll_offset_for(
                    target,
                    cursor.active_groups(),
                    &collapsed,
                    scroll.get(),
                    viewport.get(),
                )
            });
            if let Some(Some(offset)) = offset {
                scroll.set(offset);
            }
        });

        let surfaces = ctx.text_surfaces();
        let (key_vm, tap_vm, menu_vm) = (vm.clone(), vm.clone(), vm.clone());
        let (tap_rows, menu_rows) = (rows.clone(), rows);
        let (tap_scroll, menu_scroll) = (scroll_y.clone(), scroll_y);
        let (focus_list, bounds) = (self.list.clone(), self.bounds.clone());
        let root = ctx.add(
            ZStack::new()
                .child(Expand::new().child(list))
                .child(Expand::new().child(empty))
                .hover_within(vm.hovered())
                .cursor(CursorIcon::Pointer)
                .on_tap(move |event, ctx| {
                    let target = hit(
                        &tap_rows.get(),
                        event.position.x,
                        event.position.y,
                        tap_scroll.get(),
                    );
                    Self::handle_click(&tap_vm, target, event.modifiers);
                    if let Some(list) = focus_list.get() {
                        ctx.request_focus(list);
                    }
                })
                .context_menu(move |point, ctx| {
                    let target = if ctx.context_menu_trigger().is_some_and(|t| !t.is_pointer()) {
                        // The Menu key: the focused row's menu.
                        menu_vm
                            .selection()
                            .get()
                            .focused
                            .map_or(ListHit::Background, ListHit::Row)
                    } else {
                        let origin = bounds.get();
                        hit(
                            &menu_rows.get(),
                            point.x - origin.x,
                            point.y - origin.y,
                            menu_scroll.get(),
                        )
                    };
                    Some(Box::new(context_menu(Self::menu_for(&menu_vm, target))))
                })
                .on_key_preview(move |event, _ctx| {
                    let WidgetEvent::KeyDown { key, modifiers, .. } = event else {
                        return EventResponse::Ignored;
                    };
                    if surfaces.focused().is_some() {
                        return EventResponse::Ignored;
                    }
                    if Self::handle_key(&key_vm, key, *modifiers) {
                        EventResponse::Handled
                    } else {
                        EventResponse::Ignored
                    }
                }),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let child = self
            .root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0));
        // The list fills the room it is given (`Length::Fill`).
        let size = proposal.resolve(child.width, child.height);
        self.viewport.set(size.height);
        size.into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }

    fn paint(&self, bounds: Rect, _canvas: &mut Canvas, _ctx: &PaintContext) {
        // Where the list is in the window: a context menu's point is in
        // window coordinates.
        self.bounds.set(bounds);
    }
}

#[cfg(test)]
mod tests;
