//! `CoverStore`: decoded album covers, shared by every tile and the player.
//!
//! An LRU of decoded covers (256 entries; 64 in low-resource mode, which the
//! Iced UI also halves its caches for), decoded off the UI thread and
//! downsampled to the display edge times the window's scale factor, so a 160
//! px tile on a 2x display holds a 320 px bitmap rather than the 400 px cache
//! file. A tile asks for a path and binds the per-path
//! `Signal<Option<Rc<DecodedCover>>>` it gets back: the signal is `None` until
//! the pixels are ready, so a tile that scrolls into view shows its
//! placeholder, then the cover, without a frame of work on the UI thread.
//!
//! - **One decode per path.** Every request for a path shares one entry, so
//!   fifty tiles of one album (or a tile and the player's cover) decode it
//!   once. A path whose decode failed (a missing or unreadable file) stays
//!   empty, and is not retried until it leaves the LRU.
//! - **Bounded work.** At most four decodes run at once (two in low-resource
//!   mode), each on its own thread (`spawn_blocking`); the others wait their
//!   turn, the most recently requested first, since that is what is on screen.
//!   A queued path that the LRU evicted meanwhile is skipped. A decode that is
//!   running is never cancelled: its result lands in the entry, which a tile
//!   that still holds its signal still shows.
//! - **Memory.** [`CoverStore::trim`] drops the oldest covers for the
//!   memory-purge path ([`MaintenanceEvent::PurgeRequested`](crate::frontend::events::MaintenanceEvent::PurgeRequested)); the pixels of a
//!   cover that a tile still shows survive in that tile's signal until it is
//!   rebuilt.
//! - **Display size.** [`CoverStore::set_display`] sets the edge the covers are
//!   decoded at. When it changes (a window moved to a display with another
//!   scale factor), the cached covers are decoded again at the new edge in the
//!   background, and keep showing the old pixels until the new ones arrive.
//!
//! The pixels are drawn with [`cover_image`], which uses
//! `ImageWidget::from_raw`: Teksilo releases the texture on the first frame
//! that does not draw the widget, so GPU memory follows what is on screen.
//! (`RasterIcon` plus `ImageWidget::new` would keep one texture per distinct
//! cover for the life of the window.)
//!
//! ## Real and mock
//!
//! The one seam is where the pixels come from. The real build decodes the
//! cover file with `utils::covers::decode_cover_rgba`. The mocks build, whose
//! cover paths do not exist on disk, produces a deterministic gradient per path
//! (two colours from a hash of the path).

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use teksilo::prelude::{AsyncRuntimeHandle, Signal, spawn_blocking};
use teksilo::widgets::ImageWidget;

use crate::teksilo_ui::widgets::{CoverImage, SharedCover};
use crate::utils::covers::{CoverCache, DecodedCover};

/// The most covers kept.
pub const CAPACITY: usize = 256;

/// The most covers kept in low-resource mode.
pub const LOW_RESOURCE_CAPACITY: usize = 64;

/// The most decodes running at once.
pub const MAX_IN_FLIGHT: usize = 4;

/// The most decodes running at once in low-resource mode.
pub const LOW_RESOURCE_MAX_IN_FLIGHT: usize = 2;

/// How many of the oldest covers [`CoverStore::trim`] drops (twice as many in
/// low-resource mode, as the Iced UI's purge does).
pub const TRIM_COUNT: usize = 32;

/// The edge a store decodes at until [`CoverStore::set_display`] says
/// otherwise: the size of the cache files.
pub const DEFAULT_EDGE: u32 = 400;

/// The pixels of a cover, as a tile binds them.
pub type CoverSignal = Signal<Option<Rc<DecodedCover>>>;

/// The capacity for the given resource mode.
pub fn capacity_for(low_resource: bool) -> usize {
    if low_resource {
        LOW_RESOURCE_CAPACITY
    } else {
        CAPACITY
    }
}

/// The edge, in pixels, to decode a cover shown `display_edge` logical pixels
/// wide on a window with scale factor `scale`.
pub fn target_edge(display_edge: f32, scale: f32) -> u32 {
    let edge = (display_edge * scale).ceil();
    if edge.is_finite() && edge >= 1.0 {
        // Saturating float-to-int cast: absurd values clamp.
        edge as u32
    } else {
        1
    }
}

/// An image widget that draws `cover`. The caller sizes it
/// (`.size(w, h)`, `.fit(…)`).
///
/// Uses `ImageWidget::from_raw`: the texture goes with the first frame that
/// does not draw the widget. The pixels are copied into the widget once, when
/// it is built.
pub fn cover_image(cover: &DecodedCover) -> ImageWidget {
    ImageWidget::from_raw(cover.rgba.clone(), cover.width, cover.height)
}

/// Where an entry stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Waiting for a decode slot.
    Queued,
    /// A decode is running.
    Decoding,
    /// Decoded.
    Done,
    /// The decode failed; not retried.
    Failed,
}

struct Entry {
    signal: CoverSignal,
    state: Cell<State>,
    /// The edge of the decode that is running or ran last.
    edge: Cell<u32>,
}

struct Inner {
    runtime: AsyncRuntimeHandle,
    cache: RefCell<CoverCache<Rc<Entry>>>,
    /// Paths waiting for a decode slot; the last one is served first.
    queue: RefCell<Vec<String>>,
    in_flight: Cell<usize>,
    max_in_flight: usize,
    capacity: usize,
    edge: Cell<u32>,
    decodes_started: Cell<u64>,
}

/// The decoded covers. UI thread only; clones share one cache.
#[derive(Clone)]
pub struct CoverStore {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for CoverStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverStore")
            .field("covers", &self.len())
            .field("in_flight", &self.in_flight())
            .field("edge", &self.inner.edge.get())
            .finish_non_exhaustive()
    }
}

impl CoverStore {
    /// A store sized for the current resource mode
    /// (`crate::utils::is_low_resource`), decoding on `runtime`'s executor.
    pub fn new(runtime: AsyncRuntimeHandle) -> Self {
        let low = crate::utils::is_low_resource();
        let max = if low {
            LOW_RESOURCE_MAX_IN_FLIGHT
        } else {
            MAX_IN_FLIGHT
        };
        Self::with_limits(runtime, capacity_for(low), max)
    }

    /// A store that keeps `capacity` covers and runs `max_in_flight` decodes
    /// at once (both at least 1).
    pub fn with_limits(runtime: AsyncRuntimeHandle, capacity: usize, max_in_flight: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Rc::new(Inner {
                runtime,
                cache: RefCell::new(CoverCache::new(capacity)),
                queue: RefCell::new(Vec::new()),
                in_flight: Cell::new(0),
                max_in_flight: max_in_flight.max(1),
                capacity,
                edge: Cell::new(DEFAULT_EDGE),
                decodes_started: Cell::new(0),
            }),
        }
    }

    /// The cover at `path`: a signal that is `None` until its pixels are
    /// ready. A path that is already cached or being decoded shares the
    /// entry; asking promotes it to the most recently used.
    pub fn request(&self, path: &str) -> CoverSignal {
        let inner = &self.inner;
        if let Some(entry) = inner.cache.borrow_mut().get(path) {
            return entry.signal.clone();
        }
        let entry = Rc::new(Entry {
            signal: Signal::new(None),
            state: Cell::new(State::Queued),
            edge: Cell::new(inner.edge.get()),
        });
        let signal = entry.signal.clone();
        inner.cache.borrow_mut().put(path.to_string(), entry);
        inner.queue.borrow_mut().push(path.to_string());
        self.pump();
        signal
    }

    /// The decoded pixels of `path` if they are ready, without touching the
    /// LRU order.
    pub fn peek(&self, path: &str) -> Option<Rc<DecodedCover>> {
        let entry = self.inner.cache.borrow().peek(path)?;
        entry.signal.get()
    }

    /// Whether `path` has an entry (queued, decoding or decoded), without
    /// touching the LRU order.
    pub fn contains(&self, path: &str) -> bool {
        self.inner.cache.borrow().contains(path)
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.inner.cache.borrow().len()
    }

    /// Whether the store holds no entry.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many covers the store keeps.
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    /// How many decodes are running.
    pub fn in_flight(&self) -> usize {
        self.inner.in_flight.get()
    }

    /// How many decodes were started over the store's life.
    pub fn decodes_started(&self) -> u64 {
        self.inner.decodes_started.get()
    }

    /// The edge the covers are decoded at, in pixels.
    pub fn edge(&self) -> u32 {
        self.inner.edge.get()
    }

    /// Decodes covers for tiles `display_edge` logical pixels wide on a window
    /// with scale factor `scale`. When this changes the edge, the covers that
    /// are cached are decoded again at the new one, in the background; they
    /// keep their old pixels until then.
    pub fn set_display(&self, display_edge: f32, scale: f32) {
        let edge = target_edge(display_edge, scale);
        let inner = &self.inner;
        if inner.edge.replace(edge) == edge {
            return;
        }
        // Oldest first, so the most recently used is served first.
        let paths: Vec<String> = {
            let cache = inner.cache.borrow();
            let mut paths = cache.order_mru_to_lru();
            paths.reverse();
            paths
        };
        for path in paths {
            let entry = inner.cache.borrow().peek(&path);
            if let Some(entry) = entry
                && entry.state.get() == State::Done
                && entry.edge.get() != edge
            {
                entry.state.set(State::Queued);
                inner.queue.borrow_mut().push(path);
            }
        }
        self.pump();
    }

    /// Drops the oldest covers (the memory-purge path): [`TRIM_COUNT`] of
    /// them, twice as many in low-resource mode.
    pub fn trim(&self) {
        self.trim_oldest(TRIM_COUNT);
    }

    /// Drops the `count` oldest covers (twice as many in low-resource mode).
    pub fn trim_oldest(&self, count: usize) {
        self.inner
            .cache
            .borrow_mut()
            .purge_oldest(count, crate::utils::is_low_resource());
    }

    /// Drops every cover.
    pub fn clear(&self) {
        self.inner.cache.borrow_mut().clear();
        self.inner.queue.borrow_mut().clear();
    }

    /// Starts queued decodes while there is a free slot.
    fn pump(&self) {
        let inner = &self.inner;
        while inner.in_flight.get() < inner.max_in_flight {
            let Some(path) = inner.queue.borrow_mut().pop() else {
                return;
            };
            // Gone from the LRU, or already taken by an earlier queue entry
            // for the same path: nothing to do.
            let Some(entry) = inner.cache.borrow().peek(&path) else {
                continue;
            };
            if entry.state.get() != State::Queued {
                continue;
            }
            entry.state.set(State::Decoding);
            let edge = inner.edge.get();
            entry.edge.set(edge);
            inner.in_flight.set(inner.in_flight.get() + 1);
            inner.decodes_started.set(inner.decodes_started.get() + 1);

            let store = self.clone();
            inner
                .runtime
                .spawn_local(async move {
                    let decoded = {
                        let path = path.clone();
                        spawn_blocking(move || imp::decode(Path::new(&path), edge)).await
                    };
                    store.finish(&path, &entry, decoded.ok().flatten());
                })
                .detach();
        }
    }

    /// Records a decode's result and starts the next one.
    fn finish(&self, path: &str, entry: &Rc<Entry>, decoded: Option<DecodedCover>) {
        let inner = &self.inner;
        inner.in_flight.set(inner.in_flight.get().saturating_sub(1));
        match decoded {
            Some(cover) => {
                entry.signal.set(Some(Rc::new(cover)));
                if entry.edge.get() == inner.edge.get() {
                    entry.state.set(State::Done);
                } else {
                    // The display changed while this decoded: do it again at
                    // the new edge, if the cover is still cached.
                    entry.state.set(State::Queued);
                    inner.queue.borrow_mut().push(path.to_string());
                }
            }
            None => {
                tracing::debug!("the cover {path} could not be decoded");
                entry.state.set(State::Failed);
            }
        }
        self.pump();
    }
}

/// The pixels' source: the file in the real build, a gradient in the mocks
/// build.
mod imp {
    use std::path::Path;

    use crate::utils::covers::DecodedCover;

    /// Decodes the cover file at `path`, the longer edge at most `max_edge`.
    #[cfg(not(feature = "teksilo-mocks"))]
    pub fn decode(path: &Path, max_edge: u32) -> Option<DecodedCover> {
        crate::utils::covers::decode_cover_rgba(path, max_edge)
    }

    /// The mocks build's cover paths do not exist on disk: every path gets its
    /// deterministic gradient, `max_edge` pixels wide (at most
    /// [`super::MOCK_EDGE_CAP`]).
    #[cfg(feature = "teksilo-mocks")]
    pub fn decode(path: &Path, max_edge: u32) -> Option<DecodedCover> {
        let edge = max_edge.min(super::MOCK_EDGE_CAP);
        Some(super::gradient_cover(&path.to_string_lossy(), edge))
    }
}

/// The largest gradient the mocks build makes, in pixels.
#[cfg(any(test, feature = "teksilo-mocks"))]
pub const MOCK_EDGE_CAP: u32 = 160;

/// FNV-1a, 64 bits: a hash that is the same on every run and platform.
#[cfg(any(test, feature = "teksilo-mocks"))]
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A colour from hue (degrees), saturation and lightness (0 to 1).
#[cfg(any(test, feature = "teksilo-mocks"))]
fn hsl(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = lightness - chroma / 2.0;
    [r + base, g + base, b + base]
}

/// A square gradient, `edge` pixels wide, from two colours taken from a hash
/// of `path`: the same path always gives the same pixels, other paths give
/// other colours. Used for the mocks build's covers.
#[cfg(any(test, feature = "teksilo-mocks"))]
pub fn gradient_cover(path: &str, edge: u32) -> DecodedCover {
    let edge = edge.max(1);
    let hash = fnv1a(path);
    let hue = (hash % 360) as f32;
    let other = hue + 40.0 + ((hash >> 16) % 80) as f32;
    let from = hsl(hue, 0.6, 0.55);
    let to = hsl(other, 0.55, 0.25);
    let span = (2 * edge.saturating_sub(1)).max(1) as f32;
    let mut rgba = Vec::with_capacity(edge as usize * edge as usize * 4);
    for y in 0..edge {
        for x in 0..edge {
            let t = (x + y) as f32 / span;
            for channel in 0..3 {
                let value = from[channel] + (to[channel] - from[channel]) * t;
                rgba.push((value.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            rgba.push(255);
        }
    }
    DecodedCover {
        width: edge,
        height: edge,
        rgba,
    }
}

/// A decoded cover is what [`AlbumArt`](crate::teksilo_ui::widgets::AlbumArt)
/// draws.
impl CoverImage for DecodedCover {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl CoverStore {
    /// [`CoverStore::request`], as the cover type
    /// [`AlbumArt`](crate::teksilo_ui::widgets::AlbumArt) takes.
    pub fn request_shared(&self, path: &str) -> Signal<Option<SharedCover>> {
        self.request(path)
            .map(|cover| cover.clone().map(|cover| cover as SharedCover))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::teksilo_ui::test_support::run_until;

    const TIMEOUT: Duration = Duration::from_secs(20);

    /// A directory of PNG covers: `cover-{i}.png`, 100 by 50 pixels.
    struct Covers {
        dir: PathBuf,
    }

    impl Covers {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "audoxidy-cover-store-{label}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn path(&self, index: usize) -> String {
            let path = self.dir.join(format!("cover-{index}.png"));
            if !path.exists() {
                let colour = image::Rgba([(index * 40 % 256) as u8, 90, 200, 255]);
                image::RgbaImage::from_pixel(100, 50, colour)
                    .save(&path)
                    .unwrap();
            }
            path.to_string_lossy().into_owned()
        }
    }

    impl Drop for Covers {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn settled(runtime: &AsyncRuntimeHandle, store: &CoverStore) -> bool {
        run_until(runtime, TIMEOUT, || {
            store.in_flight() == 0 && store.inner.queue.borrow().is_empty()
        })
    }

    #[test]
    fn a_shared_cover_carries_the_decoded_pixels() {
        let covers = Covers::new("shared");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        let shared = store.request_shared(&covers.path(0));
        assert!(shared.get().is_none());
        assert!(run_until(&runtime, TIMEOUT, || shared.get().is_some()));
        let decoded = store.peek(&covers.path(0)).unwrap();
        let shared = shared.get().unwrap();
        assert_eq!(
            (shared.width(), shared.height()),
            (decoded.width, decoded.height)
        );
        assert_eq!(shared.rgba(), decoded.rgba.as_slice());
    }

    #[test]
    fn a_default_store_keeps_256_covers_and_a_low_resource_one_64() {
        let store = CoverStore::new(AsyncRuntimeHandle::new());
        assert_eq!(store.capacity(), 256);
        assert_eq!(capacity_for(false), 256);
        assert_eq!(capacity_for(true), 64);
        assert_eq!(store.edge(), DEFAULT_EDGE);
    }

    #[test]
    fn the_target_edge_is_the_display_edge_times_the_scale() {
        assert_eq!(target_edge(160.0, 1.0), 160);
        assert_eq!(target_edge(160.0, 2.0), 320);
        assert_eq!(target_edge(160.0, 1.25), 200);
        assert_eq!(target_edge(100.0, 1.5), 150);
        assert_eq!(target_edge(0.0, 2.0), 1);
        assert_eq!(target_edge(f32::NAN, 2.0), 1);
    }

    #[test]
    fn a_cover_is_empty_until_decoded_and_then_ready() {
        let covers = Covers::new("ready");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        let signal = store.request(&covers.path(0));
        assert!(signal.get().is_none(), "nothing on the UI thread");
        assert!(run_until(&runtime, TIMEOUT, || signal.get().is_some()));
        let cover = signal.get().unwrap();
        assert_eq!(cover.rgba.len(), (cover.width * cover.height * 4) as usize);
        assert_eq!(store.peek(&covers.path(0)).unwrap().width, cover.width);
    }

    #[test]
    fn several_requests_for_one_path_share_one_decode() {
        let covers = Covers::new("shared");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        let path = covers.path(0);
        let first = store.request(&path);
        let second = store.request(&path);
        let third = store.clone().request(&path);
        assert!(Signal::same(&first, &second) && Signal::same(&first, &third));
        assert!(run_until(&runtime, TIMEOUT, || first.get().is_some()));
        assert!(settled(&runtime, &store));
        assert_eq!(store.decodes_started(), 1);
        assert!(Rc::ptr_eq(&first.get().unwrap(), &third.get().unwrap()));
        // A later request is a cache hit.
        let _ = store.request(&path);
        assert_eq!(store.decodes_started(), 1);
    }

    #[test]
    fn the_lru_evicts_the_least_recently_used_cover_at_the_cap() {
        let covers = Covers::new("lru");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::with_limits(runtime.clone(), 3, 2);
        for index in 0..3 {
            let _ = store.request(&covers.path(index));
        }
        assert!(settled(&runtime, &store));
        // Touch cover 0: cover 1 becomes the oldest.
        let _ = store.request(&covers.path(0));
        let _ = store.request(&covers.path(3));
        assert_eq!(store.len(), 3, "never more than the cap");
        assert!(store.contains(&covers.path(0)));
        assert!(!store.contains(&covers.path(1)), "the oldest was evicted");
        assert!(store.contains(&covers.path(2)));
        assert!(store.contains(&covers.path(3)));
        assert!(settled(&runtime, &store));
        assert_eq!(store.decodes_started(), 4);
        // The evicted cover decodes again when asked for.
        let _ = store.request(&covers.path(1));
        assert!(settled(&runtime, &store));
        assert_eq!(store.decodes_started(), 5);
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn a_full_default_store_holds_256_covers() {
        let covers = Covers::new("cap");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        // Paths need no file: the failed decodes count as entries too.
        for index in 0..300 {
            let _ = store.request(&format!("{}/{index}.png", covers.dir.display()));
        }
        assert_eq!(store.len(), 256);
    }

    #[test]
    fn trim_drops_the_oldest_covers() {
        let covers = Covers::new("trim");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::with_limits(runtime.clone(), 10, 4);
        for index in 0..10 {
            let _ = store.request(&covers.path(index));
        }
        assert!(settled(&runtime, &store));
        store.trim_oldest(4);
        assert_eq!(store.len(), 6);
        assert!(!store.contains(&covers.path(0)) && !store.contains(&covers.path(3)));
        assert!(store.contains(&covers.path(4)) && store.contains(&covers.path(9)));
        // `trim` drops 32 (this store has fewer): everything goes.
        store.trim();
        assert!(store.is_empty());
    }

    #[test]
    fn a_signal_a_tile_holds_survives_a_trim() {
        let covers = Covers::new("held");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        let held = store.request(&covers.path(0));
        assert!(run_until(&runtime, TIMEOUT, || held.get().is_some()));
        store.trim();
        assert!(store.is_empty());
        assert!(held.get().is_some(), "the tile keeps showing its cover");
    }

    #[test]
    fn at_most_max_in_flight_decodes_run_at_once() {
        let covers = Covers::new("bounded");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::with_limits(runtime.clone(), 32, 2);
        let signals: Vec<CoverSignal> = (0..8).map(|i| store.request(&covers.path(i))).collect();
        assert_eq!(store.in_flight(), 2, "two started, six queued");
        assert_eq!(store.decodes_started(), 2);
        let peak = Cell::new(0);
        assert!(run_until(&runtime, TIMEOUT, || {
            peak.set(peak.get().max(store.in_flight()));
            signals.iter().all(|s| s.get().is_some())
        }));
        assert!(peak.get() <= 2, "peak {}", peak.get());
        assert_eq!(store.decodes_started(), 8);
    }

    #[test]
    fn a_queued_cover_that_was_evicted_is_not_decoded() {
        let covers = Covers::new("skip");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::with_limits(runtime.clone(), 2, 1);
        let _ = store.request(&covers.path(0)); // decoding
        let _ = store.request(&covers.path(1)); // queued
        let _ = store.request(&covers.path(2)); // queued; evicts cover 0's entry
        let _ = store.request(&covers.path(3)); // queued; evicts cover 1's entry
        assert!(settled(&runtime, &store));
        // 0 was already running; 1 left the LRU while it waited.
        assert_eq!(store.decodes_started(), 3);
        assert!(store.contains(&covers.path(2)) && store.contains(&covers.path(3)));
    }

    #[test]
    fn the_latest_request_is_served_first() {
        let covers = Covers::new("order");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::with_limits(runtime.clone(), 8, 1);
        let first = store.request(&covers.path(0)); // takes the slot
        let older = store.request(&covers.path(1));
        let newest = store.request(&covers.path(2));
        assert!(run_until(&runtime, TIMEOUT, || newest.get().is_some()));
        assert!(first.get().is_some());
        assert!(older.get().is_none(), "the newest was served before it");
        assert!(settled(&runtime, &store));
        assert!(older.get().is_some());
    }

    #[cfg(not(feature = "teksilo-mocks"))]
    #[test]
    fn a_missing_file_stays_empty_and_is_not_retried() {
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        let signal = store.request("/nonexistent/cover.png");
        assert!(settled(&runtime, &store));
        assert!(signal.get().is_none());
        let _ = store.request("/nonexistent/cover.png");
        assert_eq!(store.decodes_started(), 1);
        assert_eq!(store.in_flight(), 0);
    }

    #[cfg(not(feature = "teksilo-mocks"))]
    #[test]
    fn the_real_build_decodes_the_file_pixels() {
        let covers = Covers::new("pixels");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        let signal = store.request(&covers.path(1));
        assert!(run_until(&runtime, TIMEOUT, || signal.get().is_some()));
        let cover = signal.get().unwrap();
        assert_eq!((cover.width, cover.height), (100, 50), "never upscaled");
        assert_eq!(&cover.rgba[0..4], &[40, 90, 200, 255]);
    }

    #[cfg(feature = "teksilo-mocks")]
    #[test]
    fn the_mocks_build_gives_a_nonexistent_path_its_gradient() {
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        store.set_display(100.0, 1.0);
        let signal = store.request("cache/covers/does-not-exist.avif");
        assert!(run_until(&runtime, TIMEOUT, || signal.get().is_some()));
        let cover = signal.get().unwrap();
        assert_eq!(
            *cover,
            gradient_cover("cache/covers/does-not-exist.avif", 100)
        );
    }

    #[test]
    fn the_display_edge_downsamples_the_decode() {
        let covers = Covers::new("edge");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        store.set_display(10.0, 2.0);
        assert_eq!(store.edge(), 20);
        let signal = store.request(&covers.path(0));
        assert!(run_until(&runtime, TIMEOUT, || signal.get().is_some()));
        let cover = signal.get().unwrap();
        assert_eq!(cover.width.max(cover.height), 20);
    }

    #[test]
    fn a_new_display_edge_decodes_the_cached_covers_again() {
        let covers = Covers::new("retarget");
        let runtime = AsyncRuntimeHandle::new();
        let store = CoverStore::new(runtime.clone());
        store.set_display(10.0, 1.0);
        let signal = store.request(&covers.path(0));
        assert!(run_until(&runtime, TIMEOUT, || signal.get().is_some()));
        assert!(settled(&runtime, &store));
        let old = signal.get().unwrap();
        assert_eq!(old.width.max(old.height), 10);

        store.set_display(10.0, 3.0);
        assert!(
            signal.get().is_some(),
            "the old pixels stay until the new ones arrive"
        );
        assert!(run_until(&runtime, TIMEOUT, || {
            signal.get().is_some_and(|c| c.width.max(c.height) == 30)
        }));
        assert_eq!(store.decodes_started(), 2);
        // The same edge again changes nothing.
        store.set_display(10.0, 3.0);
        assert!(settled(&runtime, &store));
        assert_eq!(store.decodes_started(), 2);
    }

    #[test]
    fn the_gradient_is_deterministic_and_depends_on_the_path() {
        let a = gradient_cover("cache/covers/a.avif", 16);
        assert_eq!(a, gradient_cover("cache/covers/a.avif", 16));
        assert_ne!(a.rgba, gradient_cover("cache/covers/b.avif", 16).rgba);
        assert_eq!((a.width, a.height), (16, 16));
        assert_eq!(a.rgba.len(), 16 * 16 * 4);
        assert!(a.rgba.chunks(4).all(|pixel| pixel[3] == 255));
        // Two colours: the corners differ.
        assert_ne!(&a.rgba[0..3], &a.rgba[a.rgba.len() - 4..a.rgba.len() - 1]);
        assert_eq!(gradient_cover("x", 0).width, 1, "an edge of 0 is 1");
    }

    #[test]
    fn the_cover_image_widget_lays_out_at_the_size_it_is_given() {
        use teksilo::core::widget_tree::WidgetTree;
        use teksilo::widgets::{HStack, Spacer};

        let cover = gradient_cover("cache/covers/a.avif", 32);
        let mut tree = WidgetTree::new();
        let root = tree.add(
            HStack::new()
                .spacing(0.0)
                .child(cover_image(&cover).size(40.0, 40.0))
                .child(Spacer::new()),
        );
        crate::teksilo_ui::test_support::layout(&mut tree);
        let image = crate::teksilo_ui::test_support::first_of_type(&tree, root, "ImageWidget")
            .expect("the image is in the tree");
        let size = tree.bounds(image).size();
        assert_eq!((size.width, size.height), (40.0, 40.0));
    }
}
