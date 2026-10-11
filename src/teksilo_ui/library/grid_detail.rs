//! [`GridDetail`]: the songs of the grid's open album, in a panel docked under
//! the grid (the plan's decision for the Grid mode: a vertical `Splitter`
//! instead of Iced's inline expansion).
//!
//! The Iced grid opens an album inline, under its row of cards
//! (`LibraryGridElement::ExpandedSongs`, `src/gui/library.rs:1653-1719`): the
//! album's songs as 32 px song rows with the Simple list's columns, 5 px in
//! from the content's edge and 10 px from its end, on the background colour.
//! Here they sit in the detail panel, under an album header
//! ([`HeaderRow::album`]: the album's name, its songs and length, the chevron
//! that closes it), in a `ListView` of [`SongRow`]s that share the column
//! header's horizontal offset (quirk fix 36).
//!
//! The rows are the album's songs that the search text keeps
//! ([`visible_album_songs`], `library.rs:547-551`). A click selects a song
//! (Ctrl toggles, Shift extends), as `SelectSong` does in Grid mode.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use teksilo::core::Modifiers;
use teksilo::core::binding::BindingLevel;
use teksilo::data::ListModel;
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::{ListView, Padding, Panel, ScrollBarMode, VStack};

use super::header_rows::HeaderRow;
use super::hshift::HShift;
use super::library_vm::{LibraryViewModel, ScrollRequest, visible_album_songs};
use super::song_row::{ROW_HEIGHT, SongRow};
use crate::db::database::SongData;
use crate::logic::library_geometry::LibraryViewMode;
use crate::logic::library_selection::LibraryListItem;
use crate::teksilo_ui::widgets::column_header::columns_for;

/// The space before the songs (`library.rs:1713`, plus the content's own
/// 5 px, `library.rs:1951`).
pub const LEADING: f32 = 10.0;
/// The space after the songs (`library.rs:1711`).
pub const TRAILING: f32 = 10.0;
/// The space under the songs (`library.rs:1712`).
pub const BOTTOM: f32 = 5.0;

/// The detail panel; see the module documentation.
pub struct GridDetail {
    vm: LibraryViewModel,
    rows: ListModel<Arc<SongData>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for GridDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GridDetail")
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

impl GridDetail {
    /// The detail panel of `vm`'s open album.
    pub fn new(vm: LibraryViewModel) -> Self {
        Self {
            vm,
            rows: ListModel::new(),
            root: None,
        }
    }

    /// The rows the panel lists (its songs under the search).
    pub fn rows(&self) -> ListModel<Arc<SongData>> {
        self.rows.clone()
    }

    fn refresh_rows(vm: &LibraryViewModel, rows: &ListModel<Arc<SongData>>) {
        let visible = visible_album_songs(&vm.expanded_songs().get(), &vm.search().get());
        let unchanged = rows.len() == visible.len()
            && visible
                .iter()
                .enumerate()
                .all(|(i, song)| rows.with_item(i, |row| row.id == song.id) == Some(true));
        if !unchanged {
            rows.replace_all(visible);
        }
    }

    /// The header of the open album: its name, its songs and length, the
    /// chevron that closes it.
    fn header(vm: &LibraryViewModel) -> Option<HeaderRow> {
        let id = vm.expanded_album().get()?;
        let card = vm.services().albums.card(&id)?;
        let songs = vm.expanded_songs().get();
        let duration: f64 = songs.iter().map(|s| s.duration_secs.unwrap_or(0.0)).sum();
        let paths: Vec<Arc<str>> = songs.iter().map(|s| s.full_file_path.clone()).collect();
        let playing = vm
            .playing_path()
            .map(move |path| paths.iter().any(|p| p.as_ref() == path.as_str()));
        let selected = {
            let id = id.clone();
            vm.grid_selection()
                .selection_signal()
                .map(move |keys| keys.contains(&id))
        };
        let select_vm = vm.clone();
        let select_id = id.clone();
        let toggle_vm = vm.clone();
        Some(HeaderRow::album(
            &card.title,
            true,
            songs.len(),
            duration,
            ROW_HEIGHT,
            selected,
            playing,
            Rc::new(move |_modifiers: Modifiers, _ctx: &mut EventContext| {
                select_vm.select_open_album(&select_id)
            }),
            Rc::new(move |_ctx: &mut EventContext| toggle_vm.toggle_album(&id)),
        ))
    }
}

impl Widget for GridDetail {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        let self_id = ctx.self_id();
        vm.expanded_album()
            .bind_to(self_id, ctx.binding_registry(), BindingLevel::Rebuild);
        Self::refresh_rows(&vm, &self.rows);
        {
            let rows = self.rows.clone();
            let refresh_vm = vm.clone();
            ctx.effect(&vm.expanded_songs(), move |_| {
                Self::refresh_rows(&refresh_vm, &rows);
            });
        }
        {
            let rows = self.rows.clone();
            let refresh_vm = vm.clone();
            ctx.effect(&vm.search(), move |_| {
                Self::refresh_rows(&refresh_vm, &rows)
            });
        }

        let columns = columns_for(LibraryViewMode::Grid);
        let list_id: Rc<Cell<Option<WidgetId>>> = Rc::new(Cell::new(None));
        let row_vm = vm.clone();
        let row_list = list_id.clone();
        let list = ListView::new(self.rows.clone(), move |_index, song, _selected| {
            let item = LibraryListItem::Song(song.id);
            let selected = {
                let item = item.clone();
                row_vm.selection().map(move |s| s.selected.contains(&item))
            };
            let path = song.full_file_path.clone();
            let playing = row_vm
                .playing_path()
                .map(move |playing| playing.as_str() == path.as_ref());
            let press_vm = row_vm.clone();
            let press_list = row_list.clone();
            let row = SongRow::new(song.clone(), columns.clone(), row_vm.column_widths())
                .selected(selected)
                .playing(playing)
                .on_press(move |modifiers, ctx| {
                    press_vm.click(item.clone(), modifiers, Instant::now());
                    if let Some(list) = press_list.get() {
                        ctx.request_focus(list);
                    }
                });
            Box::new(HShift::new(row_vm.h_offset(), row)) as Box<dyn Widget>
        })
        .item_height(ROW_HEIGHT)
        .scroll_bar_style(ScrollBarMode::Overlay)
        .smooth_scrolling(false);
        let scroll_y = list.scroll_y_signal().clone();
        let list = ctx.add(list.access_label(tr!(library_album_songs())));
        list_id.set(Some(list));

        // Reveal a row the view-model asks for (a reveal by path).
        ctx.effect(&vm.scroll_request(), move |request| {
            if let Some((_, ScrollRequest::DetailRow(row))) = request {
                scroll_y.set(*row as f32 * ROW_HEIGHT);
            }
        });

        let mut column = VStack::new().spacing(0.0).alignment(HAlignment::Leading);
        if let Some(header) = Self::header(&vm) {
            column = column.child(header);
        }
        let root = ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .background(SurfaceRole::Main)
                .child(
                    column.child(
                        teksilo::widgets::Expand::new()
                            .child(Padding::new(0.0, TRAILING, BOTTOM, LEADING).child(list)),
                    ),
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

#[cfg(test)]
mod tests;
