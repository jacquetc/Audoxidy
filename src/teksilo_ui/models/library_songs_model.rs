//! `LibrarySongsModel`: the library snapshot the library views read.
//!
//! It holds the current search parameters (search text and the artist, album,
//! genre, year and folder filters) and the [`LibrarySnapshot`] they produced:
//! the matching songs, every album, the totals and the folders. The snapshot
//! is exposed as a `Signal<Rc<LibrarySnapshot>>` and a version counter that
//! bumps with every new snapshot, for views that rebuild on a change.
//!
//! Sorting and grouping are not done here: `logic::library_sort` and
//! `logic::library_groups` do them, applied by the view-model (and by
//! [`AlbumListModel`](super::AlbumListModel) for the grid).
//!
//! ## Loading
//!
//! A reload (the first load, a [`LibraryEvent::Changed`] or a finished scan)
//! queries the whole snapshot with `commands::library::load_library`. A change
//! of the parameters only needs the songs, so it queries those with
//! `commands::library::search` and keeps the albums, totals and folders. Both
//! share one [`Latest`]: a newer request supersedes an older one, so an older
//! search can never overwrite a newer one (Iced has this race, see the quirk
//! fix list in `docs/teksilo-port-quirk-fixes.md`). A parameter change while a
//! full reload is still pending asks for the full snapshot again, so a
//! superseded reload never leaves the albums stale.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use super::coalesced_reload::{ReloadCoalescer, reload_on_events};
use super::load::{finish_tracked, prepare_runtime};
use crate::db::database::LibrarySearchParams;
use crate::frontend::commands::library::{self, LibrarySnapshot};
use crate::frontend::events::LibraryEvent;
use crate::frontend::{AppContext, Event, Latest, Origin};
use crate::logic::library_groups::LibraryStats;

struct Inner {
    ctx: AppContext,
    runtime: AsyncRuntimeHandle,
    params: RefCell<LibrarySearchParams>,
    latest: Latest,
    /// Whether the next request must query the whole snapshot.
    full_needed: Cell<bool>,
    snapshot: Signal<Rc<LibrarySnapshot>>,
    version: Signal<u64>,
    loading: Signal<bool>,
}

/// The library snapshot and the parameters that produced it. UI thread only;
/// clones share one state.
#[derive(Clone)]
pub struct LibrarySongsModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for LibrarySongsModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibrarySongsModel")
            .field("version", &self.inner.version.get())
            .field("loading", &self.inner.loading.get())
            .finish_non_exhaustive()
    }
}

/// Whether `params` narrow the library at all. Without a filter the grid shows
/// every album; with one it shows the albums of the matching songs.
pub fn has_any_filter(params: &LibrarySearchParams) -> bool {
    params.query.as_deref().is_some_and(|q| !q.is_empty())
        || params.artist.is_some()
        || params.album.is_some()
        || params.genre.is_some()
        || params.year.is_some()
        || params.folder_id.is_some()
}

/// Whether `event` makes the snapshot stale: the library content changed, or a
/// scan finished (a scan that is still running reports its changes with
/// `Changed`).
pub fn is_reload_event(event: &Event) -> bool {
    matches!(
        event,
        Event::Library(LibraryEvent::Changed | LibraryEvent::ScanFinished)
    )
}

/// A snapshot of an empty library.
fn empty_snapshot() -> Rc<LibrarySnapshot> {
    Rc::new(LibrarySnapshot {
        songs: Vec::new(),
        albums: Vec::new(),
        stats: LibraryStats::default(),
        folders: Vec::new(),
    })
}

impl LibrarySongsModel {
    /// An empty model with no search parameters. Nothing is loaded until
    /// [`reload`](Self::reload).
    pub fn new(ctx: AppContext, runtime: AsyncRuntimeHandle) -> Self {
        Self {
            inner: Rc::new(Inner {
                ctx,
                runtime,
                params: RefCell::new(LibrarySearchParams::default()),
                latest: Latest::new(),
                full_needed: Cell::new(true),
                snapshot: Signal::new(empty_snapshot()),
                version: Signal::new(0),
                loading: Signal::new(false),
            }),
        }
    }

    /// Subscribes the model to [`LibraryEvent::Changed`] and
    /// [`LibraryEvent::ScanFinished`]; the reload runs once per frame however
    /// many arrive. Call it from a long-lived widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) -> ReloadCoalescer {
        prepare_runtime(ctx, &self.inner.runtime);
        let model = self.clone();
        reload_on_events(ctx, [Origin::Library], is_reload_event, move || {
            model.reload();
        })
    }

    /// The current snapshot (empty until the first load answers).
    pub fn snapshot(&self) -> Signal<Rc<LibrarySnapshot>> {
        self.inner.snapshot.clone()
    }

    /// A counter that bumps with every new snapshot.
    pub fn version(&self) -> Signal<u64> {
        self.inner.version.clone()
    }

    /// Whether a request is in flight.
    pub fn loading(&self) -> Signal<bool> {
        self.inner.loading.clone()
    }

    /// The search parameters in effect.
    pub fn params(&self) -> LibrarySearchParams {
        self.inner.params.borrow().clone()
    }

    /// Queries the whole snapshot again with the current parameters.
    pub fn reload(&self) {
        self.inner.full_needed.set(true);
        self.request();
    }

    /// Replaces the search parameters and queries the matching songs.
    pub fn set_params(&self, params: LibrarySearchParams) {
        *self.inner.params.borrow_mut() = params;
        self.request();
    }

    /// Changes the search parameters with `change` and queries the matching
    /// songs.
    pub fn update_params(&self, change: impl FnOnce(&mut LibrarySearchParams)) {
        change(&mut self.inner.params.borrow_mut());
        self.request();
    }

    /// Sets the search text (an empty text clears it) and queries the
    /// matching songs.
    pub fn set_query(&self, text: &str) {
        self.update_params(|params| {
            params.query = (!text.is_empty()).then(|| text.to_string());
        });
    }

    /// Sends the request the state calls for. It makes every earlier request
    /// stale.
    fn request(&self) {
        let inner = &self.inner;
        let params = self.params();
        inner.loading.set(true);
        let model = self.clone();
        if inner.full_needed.get() {
            let reply = inner
                .latest
                .track(library::load_library(&inner.ctx, params));
            finish_tracked(&inner.runtime, "the library", reply, move |snapshot| {
                model.finish(snapshot.map(Rc::new))
            });
        } else {
            let reply = library::search(&inner.ctx, params, &inner.latest);
            finish_tracked(&inner.runtime, "the library search", reply, move |songs| {
                model.finish_search(songs)
            });
        }
    }

    /// Applies a whole snapshot (`None`: the load failed).
    fn finish(&self, snapshot: Option<Rc<LibrarySnapshot>>) {
        self.inner.loading.set(false);
        let Some(snapshot) = snapshot else { return };
        self.inner.full_needed.set(false);
        self.publish(snapshot);
    }

    /// Applies the songs of a search to the current snapshot.
    fn finish_search(&self, songs: Option<Vec<Arc<crate::db::database::SongData>>>) {
        self.inner.loading.set(false);
        let Some(songs) = songs else { return };
        let current = self.inner.snapshot.get();
        self.publish(Rc::new(LibrarySnapshot {
            songs,
            ..(*current).clone()
        }));
    }

    fn publish(&self, snapshot: Rc<LibrarySnapshot>) {
        self.inner.snapshot.set(snapshot);
        let version = &self.inner.version;
        version.set(version.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use teksilo::prelude::AsyncRuntimeHandle;

    use super::*;
    use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};
    use crate::teksilo_ui::models::test_env::{self, TIMEOUT};
    use crate::teksilo_ui::test_support::{run_until, tree_with_async};

    fn model_over(ctx: &AppContext) -> (LibrarySongsModel, AsyncRuntimeHandle) {
        let runtime = AsyncRuntimeHandle::new();
        (
            LibrarySongsModel::new(ctx.clone(), runtime.clone()),
            runtime,
        )
    }

    fn wait_for_version(runtime: &AsyncRuntimeHandle, model: &LibrarySongsModel, version: u64) {
        assert!(
            run_until(runtime, TIMEOUT, || model.version().get() >= version),
            "the library did not reach version {version}"
        );
    }

    #[test]
    fn the_first_reload_loads_the_whole_snapshot() {
        let (ctx, seeded) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        assert_eq!(model.snapshot().get().songs.len(), 0);
        model.reload();
        assert!(model.loading().get());
        wait_for_version(&runtime, &model, 1);
        let snapshot = model.snapshot().get();
        assert_eq!(snapshot.songs.len(), seeded.songs.len());
        assert_eq!(snapshot.albums.len(), seeded.albums.len());
        assert_eq!(snapshot.stats.songs, seeded.songs.len() as u64);
        assert_eq!(snapshot.folders.len(), seeded.folders.len());
        assert!(!model.loading().get());
    }

    #[test]
    fn a_search_replaces_the_songs_and_keeps_the_albums() {
        let (ctx, seeded) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        model.reload();
        wait_for_version(&runtime, &model, 1);

        model.set_query("Static");
        wait_for_version(&runtime, &model, 2);
        let snapshot = model.snapshot().get();
        let titles: Vec<_> = snapshot
            .songs
            .iter()
            .filter_map(|song| song.title.as_deref())
            .collect();
        assert!(titles.contains(&"Static Bloom"), "{titles:?}");
        assert!(snapshot.songs.len() < seeded.songs.len());
        assert_eq!(snapshot.albums.len(), seeded.albums.len());
        assert_eq!(snapshot.stats.songs, seeded.songs.len() as u64);
        assert_eq!(model.params().query.as_deref(), Some("Static"));

        model.set_query("");
        wait_for_version(&runtime, &model, 3);
        assert_eq!(model.snapshot().get().songs.len(), seeded.songs.len());
        assert_eq!(model.params().query, None);
    }

    #[test]
    fn an_older_search_never_overwrites_a_newer_one() {
        let (ctx, _) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        model.reload();
        wait_for_version(&runtime, &model, 1);

        // Both requests are in flight before either answer is applied.
        model.set_query("Static");
        model.set_query("Glass");
        assert!(run_until(&runtime, TIMEOUT, || !model.loading().get()));
        runtime.tick();
        let snapshot = model.snapshot().get();
        let titles: Vec<_> = snapshot
            .songs
            .iter()
            .filter_map(|song| song.title.as_deref())
            .collect();
        assert_eq!(titles, vec!["Glass Harbour"]);
        assert_eq!(model.version().get(), 2, "the older answer was dropped");
    }

    #[test]
    fn a_search_that_supersedes_a_reload_still_brings_the_albums() {
        let (ctx, seeded) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        model.reload();
        model.set_query("Static");
        assert!(run_until(&runtime, TIMEOUT, || !model.loading().get()));
        runtime.tick();
        let snapshot = model.snapshot().get();
        assert_eq!(snapshot.albums.len(), seeded.albums.len());
        assert!(snapshot.songs.len() < seeded.songs.len());
    }

    #[test]
    fn filters_narrow_the_songs() {
        let (ctx, _) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        model.reload();
        wait_for_version(&runtime, &model, 1);
        model.update_params(|params| params.genre = Some("Rock".into()));
        wait_for_version(&runtime, &model, 2);
        let snapshot = model.snapshot().get();
        assert!(!snapshot.songs.is_empty());
        assert!(
            snapshot
                .songs
                .iter()
                .all(|song| song.genre.as_deref() == Some("Rock"))
        );
        assert!(has_any_filter(&model.params()));
    }

    #[test]
    fn the_filter_check_sees_each_parameter() {
        assert!(!has_any_filter(&LibrarySearchParams::default()));
        let empty_text = LibrarySearchParams {
            query: Some(String::new()),
            ..LibrarySearchParams::default()
        };
        assert!(!has_any_filter(&empty_text));
        let text = LibrarySearchParams {
            query: Some("a".into()),
            ..LibrarySearchParams::default()
        };
        assert!(has_any_filter(&text));
        let folder = LibrarySearchParams {
            folder_id: Some(1),
            ..LibrarySearchParams::default()
        };
        assert!(has_any_filter(&folder));
    }

    #[test]
    fn only_content_changes_and_finished_scans_make_the_snapshot_stale() {
        assert!(is_reload_event(&Event::Library(LibraryEvent::Changed)));
        assert!(is_reload_event(&Event::Library(LibraryEvent::ScanFinished)));
        assert!(!is_reload_event(&Event::Library(LibraryEvent::ScanStarted)));
        assert!(!is_reload_event(&Event::Playlists(
            crate::frontend::events::PlaylistsEvent::ListChanged
        )));
    }

    #[test]
    fn a_burst_of_library_events_reloads_once_per_frame() {
        let (ctx, _) = test_env::demo();
        let (mut tree, runtime) = tree_with_async(&ctx);
        let model = LibrarySongsModel::new(ctx.clone(), runtime.clone());
        let coalescer = Rc::new(RefCell::new(None));
        {
            let (model, slot) = (model.clone(), coalescer.clone());
            test_env::mount(&mut tree, move |build| {
                *slot.borrow_mut() = Some(model.wire(build));
            });
        }
        let coalescer = coalescer.borrow().clone().unwrap();
        let wake = test_env::wake();
        for _ in 0..50 {
            coalescer.mark(&wake);
        }
        assert_eq!(model.version().get(), 0);
        tree.frame_tick().set(16.0);
        tree.frame_tick().set(16.0);
        assert!(run_until(&runtime, TIMEOUT, || model.version().get() >= 1));
        runtime.tick();
        assert_eq!(model.version().get(), 1, "fifty events, one reload");
    }

    #[test]
    fn a_library_change_reloads_under_the_active_search() {
        // Kept as a quirk fix: Iced's refresh after a scan loads every song
        // (`get_all_songs`, `src/gui/app.rs:1345-1362`, `InternalLibraryRefreshed`)
        // and the list drops the search while its text stays.
        let (ctx, seeded) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        model.reload();
        wait_for_version(&runtime, &model, 1);
        model.set_query("Static");
        wait_for_version(&runtime, &model, 2);
        let matching = model.snapshot().get().songs.len();
        assert!(matching < seeded.songs.len());

        LibraryFixture::new()
            .song(FixtureSong::new(
                "/fixtures/music/New/Fresh/01 - Static Fresh.flac",
                "Static Fresh",
                "New Band",
                "Fresh",
            ))
            .song(FixtureSong::new(
                "/fixtures/music/New/Fresh/02 - Other.flac",
                "Other",
                "New Band",
                "Fresh",
            ))
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        // What the library-changed event does (`is_reload_event`).
        model.reload();
        wait_for_version(&runtime, &model, 3);
        let snapshot = model.snapshot().get();
        let titles: Vec<_> = snapshot
            .songs
            .iter()
            .filter_map(|song| song.title.as_deref())
            .collect();
        assert_eq!(snapshot.songs.len(), matching + 1, "{titles:?}");
        assert!(titles.contains(&"Static Fresh"));
        assert!(!titles.contains(&"Other"));
        assert_eq!(snapshot.stats.songs, seeded.songs.len() as u64 + 2);
        assert_eq!(model.params().query.as_deref(), Some("Static"));
    }

    #[test]
    fn a_scan_that_adds_songs_shows_after_a_reload() {
        let (ctx, seeded) = test_env::demo();
        let (model, runtime) = model_over(&ctx);
        model.reload();
        wait_for_version(&runtime, &model, 1);
        let before = model.snapshot().get().songs.len();
        assert_eq!(before, seeded.songs.len());

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
        model.reload();
        wait_for_version(&runtime, &model, 2);
        assert_eq!(model.snapshot().get().songs.len(), before + 1);
    }
}
