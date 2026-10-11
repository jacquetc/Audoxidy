//! What every view-model is built from: the facade, the executor, the
//! session, the cover store, the Layer A models and singles, and the links
//! through which the panels ask each other for something.
//!
//! [`AppServices`] is created once, when the root is built (the main-thread
//! executor is reachable from app state only there), and every model and
//! single is subscribed to the facade's events from that build
//! ([`AppServices::wire`]), so they live as long as the window. Cloning it
//! shares everything.
//!
//! Peer view-models never import each other. Where one panel needs another
//! to act, it goes through a link in [`UiLinks`], which the app owns:
//!
//! - [`AudioCenterLink`]: the player menu opens the Audio Center on a tab
//!   (Iced's `ToggleAudioCenter`, `src/gui/app.rs:4388`);
//! - [`UiLinks::reveal_in_library`]: the playlist's "Mostrar en la
//!   biblioteca" asks the library to reveal a song by its path
//!   (`PlaylistShowInLibrary`, `src/gui/app.rs:3674`).
//!
//! Everything else goes through the facade: adding songs to the active
//! playlist is a database write whose `Playlists::ContentChanged` event the
//! playlist content model reloads on; transport commands act on the shared
//! playback cursor (`SessionState::with_cursor`).

use std::cell::Cell;
use std::rc::Rc;

use teksilo::prelude::{AsyncRuntimeHandle, BuildContext, Signal};

use super::SessionState;
use crate::frontend::AppContext;
use crate::logic::filter_index::FilterType;
use crate::teksilo_ui::models::{
    AlbumListModel, AudioDevicesModel, EqPresetsModel, FilterHierarchyModel, LibrarySongsModel,
    PlaylistContentModel, PlaylistTabsModel,
};
use crate::teksilo_ui::shared::cover_store::CoverStore;
use crate::teksilo_ui::shell::main_window::{AppHandles, CloseHooks};
use crate::teksilo_ui::singles::{
    SingleActivePlaylist, SingleLibraryStats, SingleNowPlaying, SingleOutputDevice,
};

/// A request from one panel to another: each [`Request::send`] is a new
/// request, even with the same value, so a consumer observing the signal acts
/// on every send (revealing the same song twice reveals it twice).
#[derive(Clone)]
pub struct Request<T: Clone + 'static> {
    latest: Signal<Option<(u64, T)>>,
    sequence: Rc<Cell<u64>>,
}

impl<T: Clone + 'static> std::fmt::Debug for Request<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("sent", &self.sequence.get())
            .finish()
    }
}

impl<T: Clone + 'static> Default for Request<T> {
    fn default() -> Self {
        Self {
            latest: Signal::new(None),
            sequence: Rc::new(Cell::new(0)),
        }
    }
}

impl<T: Clone + 'static> Request<T> {
    /// Sends `value`.
    pub fn send(&self, value: T) {
        let next = self.sequence.get() + 1;
        self.sequence.set(next);
        self.latest.set(Some((next, value)));
    }

    /// The latest request with its sequence number, for the consumer to
    /// observe (`ctx.effect(&request.latest(), …)`): a new sequence number is
    /// a new request.
    pub fn latest(&self) -> Signal<Option<(u64, T)>> {
        self.latest.clone()
    }
}

/// The Audio Center tabs, in Iced's order (`selected_tab`).
pub mod audio_center_tab {
    /// "Configuración de Audio".
    pub const CONFIG: usize = 0;
    /// "Ecualizador".
    pub const EQUALIZER: usize = 1;
    /// "Efectos de Audio".
    pub const EFFECTS: usize = 2;
    /// "Volumen y Mezcla".
    pub const VOLUME_MIX: usize = 3;
}

/// Whether the Audio Center is open, on which tab, and where the user dragged
/// it. The Audio Center view-model owns what opening and closing do (closing
/// saves the equalizer and the effects); this is only the shared state and
/// Iced's toggle rule.
#[derive(Clone, Debug)]
pub struct AudioCenterLink {
    /// Whether the panel is shown.
    pub open: Signal<bool>,
    /// The selected tab (see [`audio_center_tab`]).
    pub tab: Signal<usize>,
    /// The panel's top-leading corner after a drag; `None` centres it, as it
    /// opens (`window_pos`).
    pub position: Signal<Option<(f32, f32)>>,
}

impl Default for AudioCenterLink {
    fn default() -> Self {
        Self {
            open: Signal::new(false),
            tab: Signal::new(audio_center_tab::CONFIG),
            position: Signal::new(None),
        }
    }
}

impl AudioCenterLink {
    /// Iced's `ToggleAudioCenter` (`src/gui/app.rs:4388`):
    /// - with a tab: open on the same tab closes it; otherwise it opens
    ///   (centred, if it was closed) on that tab;
    /// - without a tab: open closes, closed opens centred on the last tab.
    pub fn toggle(&self, tab: Option<usize>) {
        match tab {
            Some(tab) if self.open.get() && self.tab.get() == tab => self.close(),
            Some(tab) => {
                if !self.open.get() {
                    self.position.set(None);
                    self.open.set(true);
                }
                self.tab.set(tab);
            }
            None if self.open.get() => self.close(),
            None => {
                self.position.set(None);
                self.open.set(true);
            }
        }
    }

    /// Closes the panel and forgets its position.
    pub fn close(&self) {
        self.open.set(false);
        self.position.set(None);
    }
}

/// The links between panels; see the module documentation.
#[derive(Clone, Debug, Default)]
pub struct UiLinks {
    /// The Audio Center's shared state.
    pub audio_center: AudioCenterLink,
    /// Reveal a song in the library, by its file path.
    pub reveal_in_library: Request<String>,
}

/// What every view-model is built from; see the module documentation.
#[derive(Clone)]
pub struct AppServices {
    /// The facade.
    pub ctx: AppContext,
    /// The main-thread executor (awaits the facade's replies).
    pub runtime: AsyncRuntimeHandle,
    /// The playback session and its cursor.
    pub session: SessionState,
    /// Work to do before the window closes.
    pub close_hooks: CloseHooks,
    /// The decoded covers.
    pub covers: CoverStore,
    /// The library snapshot and its search.
    pub library_songs: LibrarySongsModel,
    /// The grid's albums.
    pub albums: AlbumListModel,
    /// The playlist tabs.
    pub playlist_tabs: PlaylistTabsModel,
    /// The active playlist's rows.
    pub playlist_content: PlaylistContentModel,
    /// The filter tree.
    pub filter_hierarchy: FilterHierarchyModel,
    /// The equalizer presets.
    pub eq_presets: EqPresetsModel,
    /// The sound hosts and output devices.
    pub audio_devices: AudioDevicesModel,
    /// The playing track and the play state.
    pub now_playing: SingleNowPlaying,
    /// The library totals.
    pub library_stats: SingleLibraryStats,
    /// The output device and its stream format.
    pub output_device: SingleOutputDevice,
    /// The active playlist.
    pub active_playlist: SingleActivePlaylist,
    /// The links between panels.
    pub links: UiLinks,
}

impl std::fmt::Debug for AppServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppServices")
            .field("session", &self.session)
            .field("links", &self.links)
            .finish_non_exhaustive()
    }
}

impl AppServices {
    /// The services over `handles`, awaiting replies on `runtime`. The filter
    /// tree starts on genres, as Iced does.
    pub fn new(handles: &AppHandles, runtime: AsyncRuntimeHandle) -> Self {
        let ctx = handles.ctx.clone();
        let session = handles.session.clone();
        let library_songs = LibrarySongsModel::new(ctx.clone(), runtime.clone());
        Self {
            covers: CoverStore::new(runtime.clone()),
            albums: AlbumListModel::new(library_songs.clone()),
            library_songs,
            playlist_tabs: PlaylistTabsModel::new(ctx.clone(), runtime.clone()),
            playlist_content: PlaylistContentModel::new(
                ctx.clone(),
                runtime.clone(),
                session.clone(),
            ),
            filter_hierarchy: FilterHierarchyModel::new(
                ctx.clone(),
                runtime.clone(),
                FilterType::Genre,
            ),
            eq_presets: EqPresetsModel::new(ctx.clone(), runtime.clone()),
            audio_devices: AudioDevicesModel::new(ctx.clone(), runtime.clone()),
            now_playing: SingleNowPlaying::new(&ctx),
            library_stats: SingleLibraryStats::new(ctx.clone(), runtime.clone()),
            output_device: SingleOutputDevice::new(&ctx),
            active_playlist: SingleActivePlaylist::new(ctx.clone(), runtime.clone(), &session),
            links: UiLinks::default(),
            close_hooks: handles.close_hooks.clone(),
            session,
            runtime,
            ctx,
        }
    }

    /// Subscribes every model and single to the facade's events, for as long
    /// as the widget building them lives (the root), and starts their first
    /// loads.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let _ = self.library_songs.wire(ctx);
        let _ = self.playlist_tabs.wire(ctx);
        let _ = self.playlist_content.wire(ctx);
        let _ = self.filter_hierarchy.wire(ctx);
        let _ = self.audio_devices.wire(ctx);
        self.now_playing.wire(ctx);
        let _ = self.library_stats.wire(ctx);
        self.output_device.wire(ctx);
        let _ = self.active_playlist.wire(ctx);
        self.library_songs.reload();
        self.playlist_tabs.reload();
        self.filter_hierarchy.reload();
        self.eq_presets.reload();
        self.audio_devices.reload();
        self.library_stats.reload();
    }
}

#[cfg(test)]
pub mod testing {
    //! Services over an in-memory context seeded with the mock library, for
    //! the view-model tests of every panel.

    use super::*;
    use crate::frontend::AppContext;
    use crate::teksilo_ui::mock_data::screenshot_library;

    /// Services over a fresh in-memory context seeded with
    /// [`screenshot_library`], and the executor to drive their replies
    /// (`test_support::run_until`). Nothing is loaded and the session is not
    /// restored: call [`AppServices::wire`] from a test widget, a model's
    /// `reload`, or `session.start_restore` as the test needs.
    pub fn in_memory_services() -> AppServices {
        let ctx = AppContext::in_memory().unwrap();
        screenshot_library()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        AppServices::new(&AppHandles::new(ctx), AsyncRuntimeHandle::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_opens_the_closed_audio_center_centred_on_that_tab() {
        let link = AudioCenterLink::default();
        link.position.set(Some((10.0, 10.0)));
        link.toggle(Some(audio_center_tab::EQUALIZER));
        assert!(link.open.get());
        assert_eq!(link.tab.get(), audio_center_tab::EQUALIZER);
        assert_eq!(link.position.get(), None);
    }

    #[test]
    fn the_open_tab_closes_it_and_another_tab_switches_in_place() {
        let link = AudioCenterLink::default();
        link.toggle(Some(audio_center_tab::CONFIG));
        link.position.set(Some((40.0, 30.0)));
        link.toggle(Some(audio_center_tab::EFFECTS));
        assert!(link.open.get());
        assert_eq!(link.tab.get(), audio_center_tab::EFFECTS);
        assert_eq!(
            link.position.get(),
            Some((40.0, 30.0)),
            "a switch keeps the position"
        );
        link.toggle(Some(audio_center_tab::EFFECTS));
        assert!(!link.open.get());
        assert_eq!(link.position.get(), None);
    }

    #[test]
    fn a_toggle_without_a_tab_flips_the_panel_on_its_last_tab() {
        let link = AudioCenterLink::default();
        link.toggle(Some(audio_center_tab::VOLUME_MIX));
        link.toggle(None);
        assert!(!link.open.get());
        link.toggle(None);
        assert!(link.open.get());
        assert_eq!(link.tab.get(), audio_center_tab::VOLUME_MIX);
    }

    #[test]
    fn every_send_is_a_new_request_even_with_the_same_value() {
        let request: Request<String> = Request::default();
        assert_eq!(request.latest().get(), None);
        request.send("/a.flac".to_string());
        request.send("/a.flac".to_string());
        assert_eq!(request.latest().get(), Some((2, "/a.flac".to_string())));
    }

    #[test]
    fn the_test_services_see_the_mock_library() {
        let services = testing::in_memory_services();
        let runtime = services.runtime.clone();
        services.library_songs.reload();
        let snapshot = services.library_songs.snapshot();
        assert!(crate::teksilo_ui::test_support::run_until(
            &runtime,
            std::time::Duration::from_secs(10),
            || !snapshot.get().songs.is_empty()
        ));
        assert!(services.ctx.shutdown());
    }
}
