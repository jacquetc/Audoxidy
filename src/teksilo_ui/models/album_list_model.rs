//! `AlbumListModel`: the album grid's tiles.
//!
//! A `ListModel<AlbumCard>` derived from the library snapshot
//! ([`LibrarySongsModel`]) through `logic::library_groups`, the same way the
//! Iced library derives its grid (`LibraryManager::update_processed_data`,
//! `src/gui/library.rs:1189`):
//!
//! - with no filter in the search parameters the grid shows every album of the
//!   library;
//! - with a filter it shows the albums of the matching songs, grouped by
//!   effective artist (`group_by_artist`, `filtered_albums`).
//!
//! Either way the albums are ordered by `logic::library_sort::sort_albums`
//! (the sort column and direction set with [`AlbumListModel::set_sort`], or
//! the canonical artist, year, title order).
//!
//! A card's id is the album's composite id, `"{artist}|{hash}"` (see
//! `logic::id_codec`). The list is updated with keyed reconcile on that id, so
//! a tile keeps its identity, its decoded cover and its position in the
//! scroll across a reload that does not touch it, and a selection of tiles
//! survives.
//!
//! The grouping runs on the UI thread, as in Iced: it is a pass over the
//! songs with no I/O.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use teksilo::core::signal::ObserverHandle;
use teksilo::data::ListModel;

use super::library_songs_model::{LibrarySongsModel, has_any_filter};
use super::reconcile::reconcile_unique;
use crate::db::database::LibrarySearchParams;
use crate::frontend::commands::library::LibrarySnapshot;
use crate::logic::library_groups::{
    AlbumEntry, album_composite_id, filtered_albums, group_by_artist,
};
use crate::logic::library_sort::sort_albums;
use crate::utils::SortColumn;

/// One tile of the album grid.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumCard {
    /// The composite id, `"{artist}|{hash}"`; the card's identity.
    pub id: String,
    /// Album title.
    pub title: String,
    /// Album artist (the artist group's name when the grid is filtered).
    pub artist: String,
    /// Genre.
    pub genre: String,
    /// Release year.
    pub year: String,
    /// Path of the cached cover, if any.
    pub cover_path: Option<String>,
}

impl From<AlbumEntry> for AlbumCard {
    fn from(entry: AlbumEntry) -> Self {
        Self {
            id: album_composite_id(&entry.artist, &entry.id),
            title: entry.title,
            artist: entry.artist,
            genre: entry.genre,
            year: entry.year,
            cover_path: entry.cover_path,
        }
    }
}

/// The cards of `snapshot` under `params`, sorted by `sort`.
fn cards_for(
    snapshot: &LibrarySnapshot,
    params: &LibrarySearchParams,
    sort: (Option<SortColumn>, Option<bool>),
) -> Vec<AlbumCard> {
    let (column, ascending) = sort;
    let entries = if has_any_filter(params) {
        let groups = group_by_artist(&snapshot.songs);
        filtered_albums(&groups, &snapshot.albums, column, ascending)
    } else {
        let mut all = snapshot.albums.clone();
        sort_albums(&mut all, column, ascending);
        all
    };
    entries.into_iter().map(AlbumCard::from).collect()
}

struct Inner {
    source: LibrarySongsModel,
    list: ListModel<AlbumCard>,
    sort: Cell<(Option<SortColumn>, Option<bool>)>,
    /// Keeps the subscription to the source's version alive.
    observer: std::cell::RefCell<Option<ObserverHandle>>,
}

/// The album grid's model. UI thread only; clones share one list.
#[derive(Clone)]
pub struct AlbumListModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for AlbumListModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumListModel")
            .field("albums", &self.inner.list.len())
            .finish_non_exhaustive()
    }
}

impl AlbumListModel {
    /// A model that follows `source`: the list is rebuilt, by keyed
    /// reconcile, whenever the source publishes a new snapshot.
    pub fn new(source: LibrarySongsModel) -> Self {
        let inner = Rc::new(Inner {
            source,
            list: ListModel::new(),
            sort: Cell::new((None, None)),
            observer: std::cell::RefCell::new(None),
        });
        let weak: Weak<Inner> = Rc::downgrade(&inner);
        let handle = inner.source.version().observe(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self { inner }.refresh();
            }
        });
        *inner.observer.borrow_mut() = Some(handle);
        let model = Self { inner };
        model.refresh();
        model
    }

    /// The tiles, in grid order.
    pub fn list(&self) -> ListModel<AlbumCard> {
        self.inner.list.clone()
    }

    /// The card with composite id `id`.
    pub fn card(&self, id: &str) -> Option<AlbumCard> {
        let list = &self.inner.list;
        (0..list.len()).find_map(|index| {
            list.with_item(index, |c| (c.id == id).then(|| c.clone()))
                .flatten()
        })
    }

    /// Sets the sort column and direction (`None`, `None`: the canonical
    /// artist, year, title order) and re-orders the grid.
    pub fn set_sort(&self, column: Option<SortColumn>, ascending: Option<bool>) {
        self.inner.sort.set((column, ascending));
        self.refresh();
    }

    /// Rebuilds the cards from the source's current snapshot.
    pub fn refresh(&self) {
        let snapshot = self.inner.source.snapshot().get();
        let params = self.inner.source.params();
        let cards = cards_for(&snapshot, &params, self.inner.sort.get());
        reconcile_unique(&self.inner.list, cards, |card| card.id.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use teksilo::data::DataChange;
    use teksilo::prelude::AsyncRuntimeHandle;

    use super::*;
    use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::run_until;

    fn titles(model: &AlbumListModel) -> Vec<String> {
        let list = model.list();
        (0..list.len())
            .filter_map(|i| list.with_item(i, |card| card.title.clone()))
            .collect()
    }

    fn loaded(
        ctx: &crate::frontend::AppContext,
    ) -> (LibrarySongsModel, AlbumListModel, AsyncRuntimeHandle) {
        let runtime = AsyncRuntimeHandle::new();
        let source = LibrarySongsModel::new(ctx.clone(), runtime.clone());
        let albums = AlbumListModel::new(source.clone());
        source.reload();
        assert!(run_until(&runtime, TIMEOUT, || source.version().get() >= 1));
        (source, albums, runtime)
    }

    #[test]
    fn the_grid_shows_every_album_when_no_filter_is_set() {
        let (ctx, seeded) = test_env::demo();
        let (_source, albums, _runtime) = loaded(&ctx);
        assert_eq!(albums.list().len(), seeded.albums.len());
        let ids: Vec<String> = (0..albums.list().len())
            .filter_map(|i| albums.list().with_item(i, |card| card.id.clone()))
            .collect();
        for album in &seeded.albums {
            assert!(
                ids.contains(&album_composite_id(&album.artist, &album.hash)),
                "{album:?} is not in {ids:?}"
            );
        }
    }

    #[test]
    fn a_card_carries_the_album_tags_and_its_cover() {
        let (ctx, seeded) = test_env::demo();
        let (_source, albums, _runtime) = loaded(&ctx);
        let night = seeded
            .albums
            .iter()
            .find(|album| album.title == "Night Signals")
            .unwrap();
        let card = albums
            .card(&album_composite_id(&night.artist, &night.hash))
            .unwrap();
        assert_eq!(card.title, "Night Signals");
        assert_eq!(card.artist, "Aurora Lane");
        assert_eq!(card.year, "2019");
        assert_eq!(card.genre, "Synthpop");
        assert!(card.cover_path.is_some());
    }

    #[test]
    fn a_filter_keeps_the_albums_of_the_matching_songs() {
        let (ctx, _) = test_env::demo();
        let (source, albums, runtime) = loaded(&ctx);
        let before = albums.list().len();
        source.update_params(|params| params.genre = Some("Rock".into()));
        assert!(run_until(&runtime, TIMEOUT, || source.version().get() >= 2));
        assert_eq!(titles(&albums), vec!["Left | Right".to_string()]);
        assert!(albums.list().len() < before);
    }

    #[test]
    fn the_sort_column_orders_the_grid() {
        let (ctx, _) = test_env::demo();
        let (_source, albums, _runtime) = loaded(&ctx);
        albums.set_sort(Some(SortColumn::Album), Some(true));
        let ascending = titles(&albums);
        let mut expected = ascending.clone();
        expected.sort_by_key(|title| title.to_lowercase());
        assert_eq!(ascending, expected);
        albums.set_sort(Some(SortColumn::Album), Some(false));
        let mut descending = titles(&albums);
        descending.reverse();
        assert_eq!(descending, ascending);
    }

    #[test]
    fn a_reload_that_adds_an_album_inserts_one_tile_and_keeps_the_others() {
        let (ctx, seeded) = test_env::demo();
        let (source, albums, runtime) = loaded(&ctx);
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = log.clone();
        let _handle = albums.list().observe_changes(move |change: &DataChange| {
            sink.borrow_mut().push(format!("{change:?}"));
        });

        source.reload();
        assert!(run_until(&runtime, TIMEOUT, || source.version().get() >= 2));
        assert!(
            log.borrow().is_empty(),
            "an unchanged reload is silent: {:?}",
            log.borrow()
        );

        LibraryFixture::new()
            .song(FixtureSong::new(
                "/fixtures/music/New/Fresh/01 - Fresh.flac",
                "Fresh",
                "New Band",
                "Fresh",
            ))
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        source.reload();
        assert!(run_until(&runtime, TIMEOUT, || source.version().get() >= 3));
        assert_eq!(albums.list().len(), seeded.albums.len() + 1);
        let log = log.borrow();
        assert_eq!(log.len(), 1, "{log:?}");
        assert!(log[0].starts_with("ItemsInserted"), "{log:?}");
    }

    #[test]
    fn the_screenshot_library_has_the_albums_of_the_grid() {
        let (ctx, _) = test_env::screenshot();
        let (_source, albums, _runtime) = loaded(&ctx);
        let titles = titles(&albums);
        for wanted in [
            "Space Oddity",
            "Aladdin Sane",
            "Pinups",
            "Stage (2017)",
            "Station to Station",
            "best of bowie",
        ] {
            assert!(titles.iter().any(|t| t == wanted), "{wanted} is missing");
        }
        assert_eq!(titles.iter().filter(|t| *t == "Aladdin Sane").count(), 2);
        assert_eq!(
            titles.iter().filter(|t| *t == "Station to Station").count(),
            3
        );
    }
}
