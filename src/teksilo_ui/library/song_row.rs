//! [`SongRow`]: a song of the library lists and of the grid's detail panel.
//!
//! Port of `universal_song_row_widget` (`src/gui/widgets.rs:2392-2523`) and
//! of its two presets, `library_song_row_widget` (32 px, no cover) and
//! `thumbnail_song_row_widget` (42 px, with a cover):
//!
//! - the row fills the width it is given, `COLOR_CONTRAST` while selected;
//! - 5 px in on both sides, the cover (Thumbnail mode: a 42 px wide slot
//!   centring the 32 px cover, radius 4, small placeholder), then one cell per
//!   column (the album card and thumbnail columns have none), each exactly its
//!   column width, clipped, 15 px in (0 for "#") and 5 px from its end;
//! - a cell's text is the song's value for the column
//!   (`utils::format_metadata`) cut to `(width − 10) / 7` characters with
//!   "..." (`truncate_text`), in Inter Medium 13 px, primary when the row is
//!   selected and secondary otherwise;
//! - the "#" cell is two 26 px halves: a centred accent "•" while the song
//!   plays, then the track number right-aligned.
//!
//! A click on the row selects it (with the click's modifiers): the view-model
//! decides what that does. The row is not a Tab stop: the list it is in takes
//! the keyboard.

use std::rc::Rc;
use std::sync::Arc;

use teksilo::core::Modifiers;
use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight, VAlignment};
use teksilo::widgets::{
    Center, FixedSize, HStack, Padding, RectWidget, Spacer, TextWidget, VStack, ZStack,
};

use crate::db::database::SongData;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::shared::cover_store::CoverStore;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::column_header::{ColumnWidths, width_of};
use crate::teksilo_ui::widgets::{AlbumArt, Placeholder};
use crate::utils::{SortColumn, format_metadata, truncate_text};

/// A song row's height in the Simple and Detailed lists and the grid detail
/// (`widgets.rs:2542`).
pub const ROW_HEIGHT: f32 = 32.0;
/// A song row's height in the Thumbnail list (`widgets.rs:2566`).
pub const THUMBNAIL_ROW_HEIGHT: f32 = 42.0;
/// The row's padding on each side (`widgets.rs:2504`).
pub const ROW_PADDING: f32 = 5.0;
/// A cell's leading padding ("#" has none; `widgets.rs:2453-2457`).
pub const CELL_LEADING: f32 = 15.0;
/// A cell's trailing padding (`widgets.rs:2464`).
pub const CELL_TRAILING: f32 = 5.0;
/// Each half of the "#" cell (`widgets.rs:2427`, `:2437`).
pub const TRACK_HALF: f32 = 26.0;
/// The text size (`widgets.rs:2420`, `:2446`).
pub const TEXT_SIZE: f32 = 13.0;
/// The cover slot of a Thumbnail row (`widgets.rs:2487`).
pub const THUMBNAIL_SLOT: f32 = 42.0;
/// The cover of a Thumbnail row (`widgets.rs:2481`).
pub const THUMBNAIL_SIZE: f32 = 32.0;
/// The cover's radius (`widgets.rs:2482`).
pub const THUMBNAIL_RADIUS: f32 = 4.0;

/// The characters a cell `width` px wide shows (`widgets.rs:2412`).
pub fn cell_chars(width: f32) -> usize {
    ((width - 10.0) / 7.0).max(1.0) as usize
}

/// The text of `song`'s `column` in a cell `width` px wide.
pub fn cell_text(song: &SongData, column: SortColumn, width: f32) -> String {
    truncate_text(&format_metadata(song, &column), cell_chars(width))
}

/// Whether `column` has a cell in a song row (the album card and thumbnail
/// columns are drawn by the list around the rows).
pub fn has_cell(column: SortColumn) -> bool {
    !matches!(column, SortColumn::AlbumCard | SortColumn::AlbumThumbnail)
}

/// The width a row of `columns` needs: the padding, the cover slot and every
/// cell.
pub fn row_width(columns: &[SortColumn], widths: &ColumnWidths, thumbnail: bool) -> f32 {
    let cells: f32 = columns
        .iter()
        .filter(|c| has_cell(**c))
        .map(|c| f32::from(width_of(widths, *c)))
        .sum();
    2.0 * ROW_PADDING + cells + if thumbnail { THUMBNAIL_SLOT } else { 0.0 }
}

/// What a click on the row does: the click's modifiers.
pub type PressFn = Rc<dyn Fn(Modifiers, &mut EventContext)>;

/// A song row; see the module documentation.
pub struct SongRow {
    song: Arc<SongData>,
    columns: Vec<SortColumn>,
    widths: Signal<ColumnWidths>,
    height: f32,
    covers: Option<CoverStore>,
    selected: Signal<bool>,
    playing: Signal<bool>,
    on_press: Option<PressFn>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for SongRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SongRow")
            .field("song", &self.song.id)
            .field("height", &self.height)
            .field("thumbnail", &self.covers.is_some())
            .finish_non_exhaustive()
    }
}

impl SongRow {
    /// A 32 px row of `song` with the cells of `columns`, sized by `widths`.
    pub fn new(
        song: Arc<SongData>,
        columns: Vec<SortColumn>,
        widths: Signal<ColumnWidths>,
    ) -> Self {
        Self {
            song,
            columns,
            widths,
            height: ROW_HEIGHT,
            covers: None,
            selected: Signal::new(false),
            playing: Signal::new(false),
            on_press: None,
            root: None,
        }
    }

    /// The 42 px Thumbnail row, its cover from `covers`.
    pub fn thumbnail(mut self, covers: CoverStore) -> Self {
        self.height = THUMBNAIL_ROW_HEIGHT;
        self.covers = Some(covers);
        self
    }

    /// Whether the row is selected.
    pub fn selected(mut self, selected: Signal<bool>) -> Self {
        self.selected = selected;
        self
    }

    /// Whether the row's song is the one playing.
    pub fn playing(mut self, playing: Signal<bool>) -> Self {
        self.playing = playing;
        self
    }

    /// What a click on the row does.
    pub fn on_press(mut self, f: impl Fn(Modifiers, &mut EventContext) + 'static) -> Self {
        self.on_press = Some(Rc::new(f));
        self
    }

    /// The row's text style for its selection state.
    fn text_role(&self) -> Signal<TextRole> {
        self.selected.map(|selected| {
            if *selected {
                TextRole::Primary
            } else {
                TextRole::Secondary
            }
        })
    }

    /// The text of one cell.
    fn cell_label(&self, column: SortColumn) -> TextWidget {
        let song = self.song.clone();
        let text = self
            .widths
            .map(move |widths| cell_text(&song, column, f32::from(width_of(widths, column))));
        TextWidget::new(lit!(""))
            .text(text)
            .style(style::inter(TEXT_SIZE, FontWeight::MEDIUM))
            .color(self.text_role())
            .single_line()
            .no_shrink()
            .a11y_hidden()
    }

    /// One cell, exactly its column wide.
    fn cell(&self, column: SortColumn) -> impl Widget + 'static {
        let width = self
            .widths
            .map(move |widths| f32::from(width_of(widths, column)));
        let content: Box<dyn Widget> = if column == SortColumn::TrackNumber {
            let dot = TextWidget::new(lit!("•"))
                .style(style::inter(TEXT_SIZE, FontWeight::MEDIUM))
                .color(TextRole::Accent)
                .single_line()
                .no_shrink()
                .a11y_hidden()
                .visible_when(self.playing.clone());
            Box::new(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Center)
                    .child(
                        FixedSize::new()
                            .width(TRACK_HALF)
                            .height(self.height)
                            .child(Center::new().child(dot)),
                    )
                    .child(
                        FixedSize::new()
                            .width(TRACK_HALF)
                            .height(self.height)
                            .child(
                                HStack::new()
                                    .spacing(0.0)
                                    .alignment(VAlignment::Center)
                                    .child(Spacer::new())
                                    .child(self.cell_label(column)),
                            ),
                    ),
            )
        } else {
            Box::new(
                VStack::new()
                    .spacing(0.0)
                    .alignment(teksilo::tokens::HAlignment::Leading)
                    .child(Spacer::new())
                    .child(self.cell_label(column))
                    .child(Spacer::new()),
            )
        };
        let leading = if column == SortColumn::TrackNumber {
            0.0
        } else {
            CELL_LEADING
        };
        FixedSize::new()
            .width(width)
            .height(self.height)
            .child(Padding::new(0.0, CELL_TRAILING, 0.0, leading).child(BoxedChild::new(content)))
            .clips_children_on(true)
    }
}

impl Widget for SongRow {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let id = ctx.self_id();
        self.selected
            .bind_to(id, ctx.binding_registry(), BindingLevel::AccessibilityOnly);
        let mut row = HStack::new().spacing(0.0).alignment(VAlignment::Center);
        if let (Some(covers), Some(path)) = (&self.covers, &self.song.compressed_cached_cover_root)
        {
            let cover = covers.request_shared(path);
            row = row.child(cover_slot(cover, self.height));
        } else if self.covers.is_some() {
            row = row.child(cover_slot(Signal::new(None), self.height));
        }
        for column in self.columns.clone() {
            if has_cell(column) {
                row = row.child(self.cell(column));
            }
        }
        let fill = self.selected.map(|selected| {
            if *selected {
                SurfaceRole::Raised
            } else {
                SurfaceRole::Transparent
            }
        });
        if let Some(on_press) = self.on_press.clone() {
            ctx.apply_self_handlers(
                HandlerSet::new()
                    .on_tap(move |tap, ctx| on_press(tap.modifiers, ctx))
                    .cursor(CursorIcon::Pointer),
            );
        }
        let root = ctx.add(
            FixedSize::new().height(self.height).child(
                ZStack::new()
                    .alignment(Alignment::CENTER_LEADING)
                    .child(RectWidget::new().background(fill))
                    .child(
                        HStack::new()
                            .spacing(0.0)
                            .alignment(VAlignment::Center)
                            .child(Padding::new(0.0, ROW_PADDING, 0.0, ROW_PADDING).child(row))
                            .child(Spacer::new()),
                    ),
            ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, self.height))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::Row);
        builder.set_name(format_metadata(&self.song, &SortColumn::Title));
        builder.set_description(format_metadata(&self.song, &SortColumn::Artist));
        builder.set_selected(self.selected.get());
    }
}

/// The 42 px slot centring the 32 px cover (`widgets.rs:2475-2492`).
fn cover_slot(
    cover: Signal<Option<crate::teksilo_ui::widgets::SharedCover>>,
    height: f32,
) -> FixedSize {
    FixedSize::new().width(THUMBNAIL_SLOT).height(height).child(
        Center::new().child(
            AlbumArt::new(
                cover,
                Placeholder::Small(icons::shared::album),
                THUMBNAIL_SIZE,
            )
            .radius(THUMBNAIL_RADIUS),
        ),
    )
}

/// A boxed widget as a child: builds it and takes its size.
pub struct BoxedChild {
    widget: Option<Box<dyn Widget>>,
    id: Option<WidgetId>,
}

impl std::fmt::Debug for BoxedChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoxedChild").finish_non_exhaustive()
    }
}

impl BoxedChild {
    /// `widget` as a child.
    pub fn new(widget: Box<dyn Widget>) -> Self {
        Self {
            widget: Some(widget),
            id: None,
        }
    }
}

impl Widget for BoxedChild {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        if let Some(widget) = self.widget.take() {
            self.id = Some(ctx.add_boxed(widget));
        }
        self.id.into_iter().collect()
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.id
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.id.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use teksilo::canvas::Point;
    use teksilo::core::{PointerButton, WidgetTree};

    use super::*;
    use crate::logic::library_geometry::LibraryViewMode;
    use crate::teksilo_ui::library::test_util::{all_of_type, mount_at, settle_layout};
    use crate::teksilo_ui::test_support::pointer_click;
    use crate::teksilo_ui::widgets::column_header::{columns_for, default_widths};

    fn song() -> Arc<SongData> {
        Arc::new(SongData {
            id: 7,
            title: Some("Moonage Daydream (2012 Remaster)".into()),
            artist: Some("David Bowie".into()),
            track_number: Some("3".into()),
            ..SongData::default()
        })
    }

    #[test]
    fn a_cell_shows_as_many_characters_as_iced_allows() {
        assert_eq!(cell_chars(275.0), 37);
        assert_eq!(cell_chars(47.0), 5);
        assert_eq!(cell_chars(5.0), 1);
        assert_eq!(cell_text(&song(), SortColumn::Artist, 155.0), "David Bowie");
        assert_eq!(cell_text(&song(), SortColumn::Title, 100.0), "Moonage D...");
        assert_eq!(cell_text(&song(), SortColumn::Genre, 135.0), "Desconocido");
    }

    #[test]
    fn a_row_is_as_wide_as_its_padding_and_cells() {
        let widths = default_widths();
        let simple = columns_for(LibraryViewMode::SimpleList);
        let cells: f32 = simple.iter().map(|c| f32::from(widths[c])).sum();
        assert_eq!(row_width(&simple, &widths, false), cells + 10.0);
        let thumbnail = columns_for(LibraryViewMode::ThumbnailList);
        let cells: f32 = thumbnail
            .iter()
            .filter(|c| has_cell(**c))
            .map(|c| f32::from(widths[c]))
            .sum();
        assert_eq!(row_width(&thumbnail, &widths, true), cells + 10.0 + 42.0);
    }

    fn mounted(row: SongRow) -> (WidgetTree, WidgetId) {
        let mut tree = WidgetTree::new();
        tree.set_theme(crate::teksilo_ui::style::audoxidy_dark());
        let id = mount_at(&mut tree, row, 1400.0, 32.0);
        settle_layout(&mut tree);
        (tree, id)
    }

    #[test]
    fn the_cells_sit_at_their_column_widths_after_the_row_padding() {
        let widths = Signal::new(default_widths());
        let columns = columns_for(LibraryViewMode::SimpleList);
        let (tree, id) = mounted(SongRow::new(song(), columns, widths));
        let row = tree.bounds(id);
        assert_eq!(row.height, ROW_HEIGHT);
        let cells = all_of_type(&tree, id, "FixedSize");
        // The first cell is "#", 47 px, 5 px in.
        let track = cells
            .iter()
            .map(|c| tree.bounds(*c))
            .find(|b| b.width == 47.0)
            .unwrap();
        assert_eq!(track.x - row.x, ROW_PADDING);
        let title = cells
            .iter()
            .map(|c| tree.bounds(*c))
            .find(|b| b.width == 275.0)
            .unwrap();
        assert_eq!(title.x - row.x, ROW_PADDING + 47.0);
    }

    #[test]
    fn a_click_reports_its_modifiers_and_the_row_names_its_song() {
        let pressed = std::rc::Rc::new(std::cell::Cell::new(None));
        let sink = pressed.clone();
        let row = SongRow::new(
            song(),
            vec![SortColumn::Title],
            Signal::new(default_widths()),
        )
        .on_press(move |modifiers, _ctx| sink.set(Some(modifiers)));
        let (mut tree, id) = mounted(row);
        let bounds = tree.bounds(id);
        pointer_click(
            &mut tree,
            Point::new(bounds.x + 50.0, bounds.y + 16.0),
            PointerButton::Primary,
            1_000,
        );
        assert_eq!(pressed.get(), Some(Modifiers::NONE));
        assert_eq!(
            tree.accessibility_node(id).name(),
            Some("Moonage Daydream (2012 Remaster)")
        );
    }
}
