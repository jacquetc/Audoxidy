//! [`LibraryGrid`]: the album grid of the Grid mode.
//!
//! Port of the Grid arm of the Iced library view (`src/gui/library.rs:1476-1746`),
//! on the stock `GridView` over the grid's albums
//! ([`AlbumListModel`](crate::teksilo_ui::models::AlbumListModel)), with the
//! keyed selection of [`LibraryViewModel::grid_selection`]:
//!
//! - a reactive [`GridSizing::FixedColumnCount`] of
//!   `clamp(floor(width / 187), 2, 10)` columns (at most 5 in low-resource
//!   mode, [`grid_columns`]), recomputed when the grid's width changes;
//! - 252 px rows with no gap between them, 5 px between the cards, 5 px of
//!   space before the first column (the content's leading padding,
//!   `library.rs:1947`);
//! - each card ([`AlbumTile`]): 182 px wide at its tile's leading edge
//!   (`GridView` stretches its tiles to share the width, the card does not
//!   stretch), 18 px of padding on top and 15 px elsewhere, the 152 px cover
//!   with an 8 px radius, then, 5 px below, four lines of Inter Medium 12 px
//!   in the primary colour (artist, album, genre, year; 14 px lines 2 px
//!   apart; the first three cut to fit 122 px as Iced estimates it) beside
//!   the 30 px chevron. The card is `COLOR_CONTRAST` with a 10 px radius while
//!   its album is selected or open in the detail panel;
//! - a click selects the card (Ctrl toggles, Shift extends, a drag on the
//!   background sweeps a selection: `GridView`'s own), a double click or the
//!   chevron opens or closes the album in the detail panel;
//! - with no album, "La biblioteca está vacía." (or "No se encontraron
//!   álbumes." while a search is typed), centred, Inter Medium, secondary.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::canvas::EdgeInsets;
use teksilo::core::signal::ObserverHandle;
use teksilo::core::styles::{GridMarqueeRecipe, GridViewStyle};
use teksilo::core::widget::WidgetPlacement;
use teksilo::data::{DataChange, ListDataSource, ListModel};
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    Center, FixedSize, GridSizing, GridView, HStack, Padding, RectWidget, ScrollBarMode, Spacer,
    TextWidget, TileContext, VStack, ZStack,
};

use super::library_vm::{LibraryViewModel, ScrollRequest, grid_columns};
use crate::teksilo_ui::icons;
use crate::teksilo_ui::models::AlbumCard;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::{AlbumArt, Placeholder, Pressable};
use crate::utils::truncate_text;

/// A grid row's height (`library.rs:471`).
pub const ROW_HEIGHT: f32 = 252.0;
/// A card's width (`library.rs:1629`).
pub const CARD_WIDTH: f32 = 182.0;
/// The gap between two cards of a row (`library.rs:1539`).
pub const COLUMN_SPACING: f32 = 5.0;
/// The space before the first column (`library.rs:1951`).
pub const LEADING_INSET: f32 = 5.0;
/// The card's top padding (`library.rs:1631`).
pub const CARD_PADDING_TOP: f32 = 18.0;
/// The card's padding on the other sides (`library.rs:1632-1634`).
pub const CARD_PADDING: f32 = 15.0;
/// The cover's side (`library.rs:1562`).
pub const COVER_SIZE: f32 = 152.0;
/// The cover's radius (`library.rs:1563`).
pub const COVER_RADIUS: f32 = 8.0;
/// The highlighted card's radius (`library.rs:1641`).
pub const CARD_RADIUS: f32 = 10.0;
/// The space between the cover and the text (`library.rs:1623`).
pub const COVER_GAP: f32 = 5.0;
/// The chevron's side (`library.rs:1605`).
pub const CHEVRON_SIZE: f32 = 30.0;
/// The text column's width: the cover's width less the chevron's.
pub const INFO_WIDTH: f32 = COVER_SIZE - CHEVRON_SIZE;
/// The card's text size (`library.rs:1569`).
pub const TEXT_SIZE: f32 = 12.0;
/// The height of each text line (`library.rs:1589`; `widgets.rs:967`).
pub const LINE_HEIGHT: f32 = 14.0;
/// The space between two text lines (`library.rs:1593`).
pub const LINE_SPACING: f32 = 2.0;
/// The empty grid's text size (Iced's default text size).
pub const EMPTY_TEXT_SIZE: f32 = 16.0;

/// `text` cut the way Iced's `smart_truncate_text` estimates a line of
/// `size` px text in `available` px: `0.55 × size` px per character, and
/// "..." when it does not fit (`widgets.rs:936-959`).
pub fn smart_truncate(text: &str, size: f32, available: f32) -> String {
    let char_width = size * 0.55;
    let needed = text.chars().count() as f32 * char_width;
    if needed <= available {
        text.to_string()
    } else {
        let max_chars = (available.max(0.0) / char_width).floor() as usize;
        truncate_text(text, max_chars)
    }
}

/// The sizing of a grid `width` px wide.
pub fn grid_sizing(width: f32, low_resource: bool) -> GridSizing {
    GridSizing::FixedColumnCount {
        count: grid_columns(width, low_resource),
        height: ROW_HEIGHT,
    }
}

/// The grid's albums as a keyed source: an album's key is its composite id,
/// so the selection follows the album through a sort or a filter.
#[derive(Clone)]
pub struct AlbumSource {
    list: ListModel<AlbumCard>,
}

impl std::fmt::Debug for AlbumSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumSource")
            .field("albums", &self.list.len())
            .finish()
    }
}

impl AlbumSource {
    /// The source over `list`.
    pub fn new(list: ListModel<AlbumCard>) -> Self {
        Self { list }
    }
}

impl ListDataSource for AlbumSource {
    type Item = AlbumCard;
    type Key = String;

    fn len(&self) -> usize {
        self.list.len()
    }

    fn with_item<R>(&self, index: usize, f: impl FnOnce(&AlbumCard) -> R) -> Option<R> {
        self.list.with_item(index, f)
    }

    fn key_at(&self, index: usize) -> Option<String> {
        self.list.with_item(index, |card| card.id.clone())
    }

    fn index_of(&self, key: &String) -> Option<usize> {
        (0..self.list.len()).find(|&i| self.list.with_item(i, |card| card.id == *key) == Some(true))
    }

    fn observe_changes(&self, f: impl Fn(&DataChange) + 'static) -> ObserverHandle {
        self.list.observe_changes(f)
    }
}

/// The grid's decoration: the Iced marquee (the accent at 30 % with a 1 px
/// accent outline, `library.rs:1989-1994`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LibraryGridStyle;

impl GridViewStyle for LibraryGridStyle {
    fn marquee(&self) -> GridMarqueeRecipe {
        GridMarqueeRecipe {
            role: BorderRole::Accent,
            fill_alpha: 0.3,
            stroke_width: 1.0,
        }
    }
}

/// One line of a card's text.
fn card_line(text: String) -> FixedSize {
    FixedSize::new().height(LINE_HEIGHT).child(
        TextWidget::new(lit!(text))
            .style(style::inter(TEXT_SIZE, FontWeight::MEDIUM))
            .color(TextRole::Primary)
            .single_line()
            .a11y_hidden(),
    )
}

/// A grid card; see the module documentation.
pub struct AlbumTile {
    card: AlbumCard,
    vm: LibraryViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for AlbumTile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumTile")
            .field("album", &self.card.id)
            .finish_non_exhaustive()
    }
}

impl AlbumTile {
    /// The card of `card`.
    pub fn new(card: AlbumCard, vm: LibraryViewModel) -> Self {
        Self {
            card,
            vm,
            root: None,
        }
    }
}

impl Widget for AlbumTile {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let card = &self.card;
        let id = card.id.clone();
        let open = self.vm.expanded_album();
        let highlighted = {
            let id = id.clone();
            open.zip(&self.vm.grid_selection().selection_signal())
                .map(move |(open, selected)| {
                    open.as_deref() == Some(id.as_str()) || selected.contains(&id)
                })
        };
        let fill = highlighted.map(|on| {
            if *on {
                SurfaceRole::Raised
            } else {
                SurfaceRole::Transparent
            }
        });
        let cover = match &card.cover_path {
            Some(path) => self.vm.services().covers.request_shared(path),
            None => Signal::new(None),
        };
        let art = AlbumArt::new(cover, Placeholder::Large(icons::shared::album), COVER_SIZE)
            .radius(COVER_RADIUS);
        let info = VStack::new()
            .spacing(LINE_SPACING)
            .alignment(HAlignment::Leading)
            .child(card_line(smart_truncate(
                &card.artist,
                TEXT_SIZE,
                INFO_WIDTH,
            )))
            .child(card_line(smart_truncate(
                &card.title,
                TEXT_SIZE,
                INFO_WIDTH,
            )))
            .child(card_line(smart_truncate(
                &card.genre,
                TEXT_SIZE,
                INFO_WIDTH,
            )))
            .child(card_line(card.year.clone()));
        let chevron_icon = open.map({
            let id = id.clone();
            move |open| open.as_deref() == Some(id.as_str())
        });
        let toggle_vm = self.vm.clone();
        let toggle_id = id.clone();
        let chevron = Pressable::new(
            ChevronIcon::new(chevron_icon.clone()),
            tr!(library_open_album(album = card.title.clone())),
        )
        .on_activate(move |_ctx| toggle_vm.toggle_album(&toggle_id));
        let bottom = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(FixedSize::new().width(INFO_WIDTH).child(info))
            .child(chevron);
        let content = Padding::new(CARD_PADDING_TOP, CARD_PADDING, CARD_PADDING, CARD_PADDING)
            .child(
                VStack::new()
                    .spacing(COVER_GAP)
                    .alignment(HAlignment::Leading)
                    .child(art)
                    .child(bottom),
            );
        let card_widget = FixedSize::new().width(CARD_WIDTH).height(ROW_HEIGHT).child(
            ZStack::new()
                .child(
                    RectWidget::new()
                        .background(fill)
                        .corner_radius(CornerRadius::uniform(CARD_RADIUS)),
                )
                .child(content),
        );
        let root = ctx.add(
            HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Top)
                .child(card_widget)
                .child(Spacer::new()),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(CARD_WIDTH, ROW_HEIGHT))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// The card's 30 px chevron in the primary colour: up while the album is open
/// in the detail panel (`library.rs:1596-1611`).
struct ChevronIcon {
    up: Signal<bool>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ChevronIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChevronIcon")
            .field("up", &self.up.get())
            .finish_non_exhaustive()
    }
}

impl ChevronIcon {
    fn new(up: Signal<bool>) -> Self {
        Self { up, root: None }
    }
}

impl Widget for ChevronIcon {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.up.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            teksilo::core::binding::BindingLevel::Rebuild,
        );
        let icon = if self.up.get() {
            icons::shared::arrow_up_chevron()
        } else {
            icons::shared::arrow_down_chevron()
        };
        let root = ctx.add(
            FixedSize::new()
                .width(CHEVRON_SIZE)
                .height(CHEVRON_SIZE)
                .child(icon.icon_size(CHEVRON_SIZE).color(TextRole::Primary)),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| Size::new(CHEVRON_SIZE, CHEVRON_SIZE))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// The album grid; see the module documentation.
pub struct LibraryGrid {
    vm: LibraryViewModel,
    sizing: Signal<GridSizing>,
    scroll_y: Option<Signal<f32>>,
    height: Rc<Cell<f32>>,
    grid: Option<WidgetId>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for LibraryGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryGrid")
            .field("sizing", &self.sizing.get())
            .finish_non_exhaustive()
    }
}

impl LibraryGrid {
    /// The grid of `vm`'s albums.
    pub fn new(vm: LibraryViewModel) -> Self {
        let sizing = Signal::new(grid_sizing(0.0, vm.low_resource()));
        Self {
            vm,
            sizing,
            scroll_y: None,
            height: Rc::new(Cell::new(0.0)),
            grid: None,
            root: None,
        }
    }

    /// The sizing the grid uses (its column count follows the width).
    pub fn sizing(&self) -> Signal<GridSizing> {
        self.sizing.clone()
    }

    /// The empty grid's message (`library.rs:1487-1501`).
    fn empty_view(search: Signal<String>) -> Box<dyn Widget> {
        let text = |message: LocalizedString, shown: Signal<bool>| {
            TextWidget::new(message)
                .style(style::inter(EMPTY_TEXT_SIZE, FontWeight::MEDIUM))
                .color(TextRole::Secondary)
                .visible_when(shown)
        };
        let searching = search.map(|text| !text.is_empty());
        Box::new(
            Center::new().child(
                ZStack::new()
                    .child(text(tr!(library_empty()), searching.not()))
                    .child(text(tr!(library_no_albums()), searching)),
            ),
        )
    }
}

impl Widget for LibraryGrid {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        let list = vm.services().albums.list();
        let tile_vm = vm.clone();
        let activate_vm = vm.clone();
        let activate_list = list.clone();
        let label_list = list.clone();
        let search = vm.search();
        let grid = GridView::from_source_keyed(
            AlbumSource::new(list),
            vm.grid_selection(),
            move |tile: &TileContext<'_, AlbumCard>| {
                Box::new(AlbumTile::new(tile.item.clone(), tile_vm.clone())) as Box<dyn Widget>
            },
        )
        .sizing(self.sizing.clone())
        .column_spacing(COLUMN_SPACING)
        .row_spacing(0.0)
        .content_inset(EdgeInsets::new(0.0, 0.0, 0.0, LEADING_INSET))
        .scroll_bar_style(ScrollBarMode::Overlay)
        .smooth_scrolling(false)
        .style(LibraryGridStyle)
        .on_tile_activate(move |index, _ctx| {
            if let Some(id) = activate_list.with_item(index, |card| card.id.clone()) {
                activate_vm.toggle_album(&id);
            }
        })
        .tile_a11y_label(move |index| {
            label_list
                .with_item(index, |card| format!("{} — {}", card.artist, card.title))
                .unwrap_or_default()
        })
        .empty_view(move || Self::empty_view(search.clone()));
        let scroll_y = grid.scroll_y_signal().clone();
        self.scroll_y = Some(scroll_y.clone());
        let grid = ctx.add(grid.access_label(tr!(library_grid())));
        self.grid = Some(grid);

        // Back to the top on a new search (`app.rs:2810`), and reveal a tile
        // the view-model asks for (a reveal by path).
        let height = self.height.clone();
        let sizing = self.sizing.clone();
        ctx.effect(&vm.scroll_request(), move |request| match request {
            Some((_, ScrollRequest::Top)) => scroll_y.set(0.0),
            Some((_, ScrollRequest::GridTile(index))) => {
                let GridSizing::FixedColumnCount { count, .. } = sizing.get() else {
                    return;
                };
                let top = (index / count.max(1)) as f32 * ROW_HEIGHT;
                let viewport = height.get();
                let current = scroll_y.get();
                if top < current {
                    scroll_y.set(top);
                } else if top + ROW_HEIGHT > current + viewport {
                    scroll_y.set((top + ROW_HEIGHT - viewport).max(0.0));
                }
            }
            _ => {}
        });
        self.root = Some(grid);
        vec![grid]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn place_children(
        &self,
        bounds: Rect,
        _proposal: SizeProposal,
        children: &mut [WidgetPlacement],
        _ctx: &LayoutContext,
    ) {
        for child in children.iter_mut() {
            child.origin = bounds.origin();
            child.size = bounds.size();
        }
        self.height.set(bounds.height);
        // The column count follows the width the grid is given; written from
        // layout behind an equality guard, as the framework's own responsive
        // widgets do (it never changes the grid's width, so it cannot loop).
        let wanted = grid_sizing(bounds.width, self.vm.low_resource());
        if self.sizing.get() != wanted {
            self.sizing.set(wanted);
        }
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
