//! The Detailed list mode: artist headers and album blocks.
//!
//! Port of `detailed_song_list` (`src/gui/widgets.rs:2785-3115`). Under each
//! artist header, one block per album ([`AlbumBlock`]), then 10 px:
//!
//! - an open album: the 250 × 323 card (15 px of padding; the 220 px cover
//!   with an 8 px radius, then, 5 px below, the artist, the album and the
//!   genre in Inter Medium 13 px cut to fit, and the year), and beside it the
//!   album header (32 px) over the album's song rows, each 32 px, with the
//!   Detailed columns (`library_song_row_widget`). The card is
//!   `COLOR_CONTRAST`, its bottom corners rounded by 10 px, while the album
//!   **or any of its songs** is selected (`widgets.rs:3017`, `:2935-2941`); a
//!   click on it selects the album;
//! - a collapsed album: 250 px of empty space beside its header.
//!
//! A block is `max(323, 32 + songs × 32) + 10` px high when open and
//! `32 + 10` when collapsed (`logic::library_geometry::album_block_height`).
//! Its songs are a bounded `Repeater`: an album's songs are all realized with
//! the block, which the list virtualizes as one row.
//!
//! The block follows the column header's horizontal offset as one piece
//! ([`HShift`], quirk fix 36): the card stays under the "Album Card" column
//! and the song cells under theirs, and since the shifted content is as wide
//! as the visible width plus the offset, the album header, which fills the
//! column beside the card, ends (its figures and chevron) at the list's
//! trailing edge; the song rows keep their columns' width, cut at that edge.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use teksilo::core::Modifiers;
use teksilo::data::ListModel;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    Expand, FixedSize, HStack, Padding, RectWidget, Repeater, Spacer, TextWidget, VStack, ZStack,
};

use super::grid::smart_truncate;
use super::header_rows::HeaderRow;
use super::hshift::HShift;
use super::library_vm::LibraryViewModel;
use super::list_rows::LibraryList;
use super::list_rows::{playing_signal, selected_signal};
use super::song_row::{ROW_HEIGHT, SongRow};
use super::structure::ListStructure;
use crate::db::database::SongData;
use crate::logic::library_geometry::{
    DETAILED_BLOCK_GAP, DETAILED_CARD_HEIGHT, LibraryViewMode, album_block_height,
};
use crate::logic::library_groups::album_composite_id;
use crate::logic::library_selection::LibraryListItem;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::column_header::columns_for;
use crate::teksilo_ui::widgets::{AlbumArt, Placeholder};

/// The Detailed list of `vm`.
pub fn detailed_list(vm: LibraryViewModel) -> LibraryList {
    LibraryList::new(vm, LibraryViewMode::DetailedList)
}

/// The card's width (`widgets.rs:2820`).
pub const CARD_WIDTH: f32 = 250.0;
/// The card's padding (`widgets.rs:2951-2956`).
pub const CARD_PADDING: f32 = 15.0;
/// The cover's side: the card less its padding (`widgets.rs:2896`).
pub const COVER_SIZE: f32 = CARD_WIDTH - 2.0 * CARD_PADDING;
/// The cover's radius (`widgets.rs:2897`).
pub const COVER_RADIUS: f32 = 8.0;
/// The highlighted card's bottom radius (`widgets.rs:2962-2963`).
pub const CARD_RADIUS: f32 = 10.0;
/// The space between the cover and the text (`widgets.rs:2944`).
pub const COVER_GAP: f32 = 5.0;
/// The card's text size (`widgets.rs:2903`).
pub const TEXT_SIZE: f32 = 13.0;
/// The space between two text lines (`widgets.rs:2924`).
pub const LINE_SPACING: f32 = 2.0;

/// What an album block shows.
#[derive(Debug, Clone)]
pub struct BlockData {
    /// The artist group's name.
    pub artist: String,
    /// The album's name.
    pub name: String,
    /// The album's composite id.
    pub id: String,
    /// The album's songs.
    pub songs: Vec<Arc<SongData>>,
    /// Their total length.
    pub duration_secs: f64,
    /// The genre of the first song.
    pub genre: String,
    /// The year of the first song.
    pub year: String,
    /// The cached cover.
    pub cover_path: Option<String>,
    /// Whether the album is open.
    pub expanded: bool,
}

impl BlockData {
    /// The block of the album run `album` of the structure's artist `artist`.
    pub fn from_structure(structure: &ListStructure, artist: usize, album: usize) -> Option<Self> {
        let va = structure.artists.get(artist)?;
        let run = va.albums.get(album)?;
        let id = album_composite_id(&va.artist_name, &run.album_hash);
        let first = run.songs.first().map(|(song, _)| song.clone());
        Some(Self {
            artist: va.artist_name.clone(),
            name: run.album_name.clone(),
            expanded: !structure.collapsed_albums.contains(&id),
            id,
            songs: run.songs.iter().map(|(song, _)| song.clone()).collect(),
            duration_secs: run.duration_secs,
            genre: first
                .as_ref()
                .and_then(|s| s.genre.as_deref())
                .unwrap_or_default()
                .to_string(),
            year: first
                .as_ref()
                .and_then(|s| s.release_year.as_deref())
                .unwrap_or_default()
                .to_string(),
            cover_path: structure.cover_of(&va.artist_name, &run.album_hash),
        })
    }

    /// The block's height.
    pub fn height(&self) -> f32 {
        album_block_height(
            LibraryViewMode::DetailedList,
            self.expanded,
            self.songs.len(),
        )
    }
}

/// An album block; see the module documentation.
pub struct AlbumBlock {
    vm: LibraryViewModel,
    data: BlockData,
    list: Rc<Cell<Option<WidgetId>>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for AlbumBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumBlock")
            .field("album", &self.data.id)
            .field("expanded", &self.data.expanded)
            .finish_non_exhaustive()
    }
}

impl AlbumBlock {
    /// The block of `data`; a click on a song focuses `list`.
    pub fn new(vm: LibraryViewModel, data: BlockData, list: Rc<Cell<Option<WidgetId>>>) -> Self {
        Self {
            vm,
            data,
            list,
            root: None,
        }
    }

    /// The block of a structure's album run.
    pub fn from_structure(
        vm: &LibraryViewModel,
        structure: &ListStructure,
        artist: usize,
        album: usize,
        list: Rc<Cell<Option<WidgetId>>>,
    ) -> Option<Self> {
        BlockData::from_structure(structure, artist, album)
            .map(|data| Self::new(vm.clone(), data, list))
    }

    /// Whether the card is lit: the album or one of its songs is selected.
    fn highlighted(&self) -> Signal<bool> {
        let album = LibraryListItem::Album(self.data.id.clone());
        let songs: Vec<LibraryListItem> = self
            .data
            .songs
            .iter()
            .map(|s| LibraryListItem::Song(s.id))
            .collect();
        self.vm.selection().map(move |s| {
            s.selected.contains(&album) || songs.iter().any(|song| s.selected.contains(song))
        })
    }

    /// The card (`widgets.rs:2882-2977`).
    fn card(&self) -> impl Widget + 'static {
        let data = &self.data;
        let fill = self.highlighted().map(|on| {
            if *on {
                SurfaceRole::Raised
            } else {
                SurfaceRole::Transparent
            }
        });
        let cover = match &data.cover_path {
            Some(path) => self.vm.services().covers.request_shared(path),
            None => Signal::new(None),
        };
        let line = |text: String| {
            TextWidget::new(lit!(text))
                .style(style::inter(TEXT_SIZE, FontWeight::MEDIUM))
                .color(TextRole::Primary)
                .single_line()
                .a11y_hidden()
        };
        let info = VStack::new()
            .spacing(LINE_SPACING)
            .alignment(HAlignment::Leading)
            .child(line(smart_truncate(&data.artist, TEXT_SIZE, COVER_SIZE)))
            .child(line(smart_truncate(&data.name, TEXT_SIZE, COVER_SIZE)))
            .child(line(smart_truncate(&data.genre, TEXT_SIZE, COVER_SIZE)))
            .child(line(data.year.clone()));
        let item = LibraryListItem::Album(data.id.clone());
        let vm = self.vm.clone();
        FixedSize::new()
            .width(CARD_WIDTH)
            .height(DETAILED_CARD_HEIGHT)
            .child(
                ZStack::new()
                    .child(
                        RectWidget::new()
                            .background(fill)
                            .corner_radius(CornerRadius {
                                top_left: 0.0,
                                top_right: 0.0,
                                bottom_right: CARD_RADIUS,
                                bottom_left: CARD_RADIUS,
                            }),
                    )
                    .child(
                        Padding::uniform(CARD_PADDING).child(
                            VStack::new()
                                .spacing(COVER_GAP)
                                .alignment(HAlignment::Leading)
                                .child(
                                    AlbumArt::new(
                                        cover,
                                        Placeholder::Large(icons::shared::album),
                                        COVER_SIZE,
                                    )
                                    .radius(COVER_RADIUS),
                                )
                                .child(FixedSize::new().width(COVER_SIZE).child(info)),
                        ),
                    ),
            )
            .on_tap(move |tap, _ctx| vm.click(item.clone(), tap.modifiers, Instant::now()))
            .cursor(CursorIcon::Pointer)
    }

    /// The album header beside the card (`widgets.rs:2979-2993`).
    fn header(&self) -> HeaderRow {
        let data = &self.data;
        let item = LibraryListItem::Album(data.id.clone());
        let select_vm = self.vm.clone();
        let toggle_vm = self.vm.clone();
        let id = data.id.clone();
        HeaderRow::album(
            &data.name,
            data.expanded,
            data.songs.len(),
            data.duration_secs,
            ROW_HEIGHT,
            selected_signal(&self.vm, item.clone()),
            playing_signal(&self.vm, &data.songs),
            Rc::new(move |modifiers: Modifiers, _ctx: &mut EventContext| {
                select_vm.click(item.clone(), modifiers, Instant::now());
            }),
            Rc::new(move |_ctx: &mut EventContext| toggle_vm.toggle_album(&id)),
        )
    }

    /// The album's song rows: a bounded `Repeater`.
    fn songs(&self) -> Repeater<Arc<SongData>> {
        let vm = self.vm.clone();
        let list = self.list.clone();
        let columns = columns_for(LibraryViewMode::DetailedList);
        Repeater::new(ListModel::from_vec(self.data.songs.clone()), move |song| {
            let item = LibraryListItem::Song(song.id);
            let press_vm = vm.clone();
            let list = list.clone();
            Box::new(
                SongRow::new(song.clone(), columns.clone(), vm.column_widths())
                    .selected(selected_signal(&vm, item.clone()))
                    .playing(playing_signal(&vm, std::slice::from_ref(song)))
                    .on_press(move |modifiers, ctx| {
                        press_vm.click(item.clone(), modifiers, Instant::now());
                        if let Some(list) = list.get() {
                            ctx.request_focus(list);
                        }
                    }),
            ) as Box<dyn Widget>
        })
    }
}

impl Widget for AlbumBlock {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let mut right = VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(Expand::horizontal().child(self.header()));
        if self.data.expanded {
            // The rows are as wide as their columns, wider than the column
            // beside the card when the window is narrow: they are laid out
            // at their width and cut at the column's edge, which is the
            // list's edge plus the shared offset (the block's `HShift`), as
            // Iced's rows are cut at the list's. An unshifted `HShift` does
            // that; a stack would have to hold more than its width.
            right = right
                .child(Expand::horizontal().child(HShift::new(Signal::new(0.0), self.songs())));
        }
        let leading: Box<dyn Widget> = if self.data.expanded {
            Box::new(self.card())
        } else {
            Box::new(
                FixedSize::new()
                    .width(CARD_WIDTH)
                    .height(0.0)
                    .child(Spacer::new()),
            )
        };
        let row = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(super::song_row::BoxedChild::new(leading))
            .child(Expand::new().child(right));
        let root = ctx.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(
                    Expand::horizontal().child(
                        FixedSize::new()
                            .height(self.data.height() - DETAILED_BLOCK_GAP)
                            .child(HShift::new(self.vm.h_offset(), row)),
                    ),
                )
                .child(
                    FixedSize::new()
                        .height(DETAILED_BLOCK_GAP)
                        .child(Spacer::new()),
                ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, self.data.height()))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
