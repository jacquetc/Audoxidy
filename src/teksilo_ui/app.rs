//! The root widget: the window's layout, and the start of the session.
//!
//! The layout is the Iced UI's (`AudoxidyApp::view`, `src/gui/app.rs:5134`):
//!
//! ```text
//! ┌──────────────┬────────────────────────────────────────────┐
//! │ player       │ title bar (40 px)                          │
//! │ 400 × 400    ├────────────┬───────────────────────────────┤
//! ├──────────────┤ filters    │ library                       │
//! │ playlist     │ 202 px     │ (the rest)                    │
//! │ 400 px wide  │            │                               │
//! └──────────────┴────────────┴───────────────────────────────┘
//! ```
//!
//! Each region is a slot the feature panels are mounted in. Building the root
//! also starts the startup restore ([`SessionState::start_restore`]) on the
//! main-thread executor, and creates the [`AppServices`] every panel's
//! view-model is built from, subscribed to the facade's events for the life
//! of the window. The root owns what spans panels: the Ctrl+A shortcut
//! ([`keys`]) and the effects between view-models and the UI's memory purge
//! ([`wiring`]).

pub mod keys;
pub mod services;
pub mod session;
pub mod wiring;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use teksilo::core::PlatformTitleBarHost;
#[cfg(test)]
use teksilo::core::WidgetTree;
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, HAlignment, VAlignment};
use teksilo::widgets::{Expand, FixedSize, HStack, Panel, Spacer, VStack, ZStack};

pub use self::services::{AppServices, AudioCenterLink, Request, UiLinks, audio_center_tab};
pub use self::session::{RestoreStatus, SessionState};
use crate::teksilo_ui::audio_center::config::{ConfigTab, ConfigViewModel};
use crate::teksilo_ui::audio_center::effects::{EffectsTab, EffectsViewModel};
use crate::teksilo_ui::audio_center::equalizer::{EqualizerTab, EqualizerViewModel};
use crate::teksilo_ui::audio_center::shell::{AudioCenterPanel, AudioCenterViewModel};
use crate::teksilo_ui::audio_center::volume_mix::{VolumeMixTab, VolumeMixViewModel};
use crate::teksilo_ui::filters::{FiltersPanel, FiltersViewModel};
use crate::teksilo_ui::library::{LibraryPanel, LibraryViewModel};
use crate::teksilo_ui::meter::{MeterSlot, MeterViewModel};
use crate::teksilo_ui::player::{PlayerPanel, PlayerViewModel};
use crate::teksilo_ui::playlist::{PlaylistPanel, PlaylistViewModel};
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::shell::title_bar;

/// The width of the left column (player and playlist).
pub const LEFT_COLUMN_WIDTH: f32 = 400.0;
/// The player panel's height.
pub const PLAYER_HEIGHT: f32 = 400.0;
/// The filter column's width.
pub const FILTERS_WIDTH: f32 = title_bar::LOGO_WIDTH;

/// The ids of the layout's regions, for the panels mounted in them and for
/// tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regions {
    /// The player slot (top of the left column).
    pub player: WidgetId,
    /// The playlist slot (rest of the left column).
    pub playlist: WidgetId,
    /// The title bar.
    pub title_bar: WidgetId,
    /// The filter column slot.
    pub filters: WidgetId,
    /// The library slot.
    pub library: WidgetId,
}

/// The view-models the root builds for the panels that other parts of the app
/// act on, and the playlist list's id (what takes the playlist's keys).
#[derive(Clone)]
pub struct AppViewModels {
    /// The services they were built from.
    pub services: AppServices,
    /// The playlist's.
    pub playlist: PlaylistViewModel,
    /// The library's.
    pub library: LibraryViewModel,
    /// The filter column's.
    pub filters: FiltersViewModel,
    /// The playlist's list, once the panel is built.
    pub playlist_list: Rc<Cell<Option<WidgetId>>>,
}

impl std::fmt::Debug for AppViewModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppViewModels")
            .field("playlist_list", &self.playlist_list.get())
            .finish_non_exhaustive()
    }
}

/// The root widget; see the module documentation.
pub struct App {
    handles: AppHandles,
    services: Option<AppServices>,
    host: Option<Rc<dyn PlatformTitleBarHost>>,
    regions: Rc<Cell<Option<Regions>>>,
    view_models: Rc<RefCell<Option<AppViewModels>>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("custom_chrome", &self.host.is_some())
            .field("regions", &self.regions.get())
            .finish_non_exhaustive()
    }
}

impl App {
    /// The root over `handles`. With a title-bar `host` the window draws its
    /// own chrome; without one it has native decorations.
    pub fn new(handles: AppHandles, host: Option<Rc<dyn PlatformTitleBarHost>>) -> Self {
        Self {
            handles,
            services: None,
            host,
            regions: Rc::new(Cell::new(None)),
            view_models: Rc::new(RefCell::new(None)),
            root: None,
        }
    }

    /// A handle on the regions, filled when the root is built.
    pub fn regions(&self) -> Rc<Cell<Option<Regions>>> {
        self.regions.clone()
    }

    /// A handle on the view-models, filled when the root is built with
    /// services.
    pub fn view_models(&self) -> Rc<RefCell<Option<AppViewModels>>> {
        self.view_models.clone()
    }

    /// Starts the startup restore on the executor `install_async` registered.
    /// The services, once the root is built.
    pub fn services(&self) -> Option<&AppServices> {
        self.services.as_ref()
    }

    /// Starts the startup restore on the executor `install_async` registered,
    /// and builds and wires the services every panel is built from.
    fn start_session(&mut self, ctx: &mut BuildContext) {
        let Some(runtime) = ctx.app_state::<AsyncRuntimeHandle>().cloned() else {
            tracing::error!("no async runtime is installed; the session cannot be restored");
            return;
        };
        // The restore's reply wakes the executor from a worker thread; with
        // the poster set, that wake also wakes the event loop, even if the
        // reply lands before the loop's own start-up hook ran.
        if let Some(poster) = ctx.poster() {
            runtime.set_poster(poster.clone());
        }
        self.handles
            .session
            .start_restore(&self.handles.ctx, &runtime);
        if self.services.is_none() {
            let services = AppServices::new(&self.handles, runtime);
            services.wire(ctx);
            self.services = Some(services);
        }
    }
}

/// The feature panels, before they are added to the tree.
struct Panels {
    player: Box<dyn Widget>,
    playlist: Box<dyn Widget>,
    filters: Box<dyn Widget>,
    library: Box<dyn Widget>,
    audio_center: Box<dyn Widget>,
}

impl Panels {
    /// Empty slots, when there are no services (no async runtime: logged by
    /// `start_session`).
    fn empty() -> Self {
        Self {
            player: Box::new(Spacer::new()),
            playlist: Box::new(Spacer::new()),
            filters: Box::new(Spacer::new()),
            library: Box::new(Spacer::new()),
            audio_center: Box::new(Spacer::new()),
        }
    }

    /// The panels over `services`, with what spans them registered on the
    /// root being built: the Ctrl+A shortcut and the wiring between
    /// view-models.
    fn new(
        ctx: &mut BuildContext,
        services: &AppServices,
        leading_inset: f32,
    ) -> (Self, AppViewModels) {
        let playlist = PlaylistViewModel::new(services);
        let filters = FiltersViewModel::new(services);
        let library = LibraryViewModel::new(services);
        let in_playlist = Signal::new(false);
        let in_library = Signal::new(false);
        keys::register_select_all(
            ctx,
            keys::SelectAllPanels {
                playlist: playlist.clone(),
                library: library.clone(),
                in_playlist: in_playlist.clone(),
                in_library: in_library.clone(),
            },
        );
        wiring::wire(ctx, services, &filters, &library);
        let playlist_panel = PlaylistPanel::new(playlist.clone());
        let view_models = AppViewModels {
            services: services.clone(),
            playlist,
            library: library.clone(),
            filters: filters.clone(),
            playlist_list: playlist_panel.list_id(),
        };
        let panels = Self {
            player: Box::new(
                PlayerPanel::new(PlayerViewModel::new(services)).leading_inset(leading_inset),
            ),
            playlist: Box::new(playlist_panel.focus_within(in_playlist)),
            filters: Box::new(FiltersPanel::new(filters)),
            library: Box::new(
                LibraryPanel::new(library)
                    .meter_slot(MeterSlot::new(MeterViewModel::new(services)))
                    .focus_within(in_library),
            ),
            audio_center: Box::new(AudioCenterPanel::new(
                AudioCenterViewModel::new(services),
                Box::new(ConfigTab::new(ConfigViewModel::new(services))),
                Box::new(EqualizerTab::new(EqualizerViewModel::new(services))),
                Box::new(EffectsTab::new(EffectsViewModel::new(services))),
                Box::new(VolumeMixTab::new(VolumeMixViewModel::new(services))),
            )),
        };
        (panels, view_models)
    }
}

/// A row as Iced lays one out by default: no spacing, children at the top.
fn row() -> HStack {
    HStack::new().spacing(0.0).alignment(VAlignment::Top)
}

/// A column as Iced lays one out by default: no spacing, children at the
/// leading edge.
fn column() -> VStack {
    VStack::new().spacing(0.0).alignment(HAlignment::Leading)
}

impl Widget for App {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.start_session(ctx);

        // On macOS the OS draws the window buttons over the player's
        // top-leading corner; the player's top row starts after them.
        let leading_inset = self
            .host
            .as_ref()
            .map_or(0.0, |host| host.reserved_leading_inset().width);
        let panels = match &self.services {
            Some(services) => {
                let (panels, view_models) = Panels::new(ctx, services, leading_inset);
                *self.view_models.borrow_mut() = Some(view_models);
                panels
            }
            None => Panels::empty(),
        };
        let player_panel = ctx.add_boxed(panels.player);
        let player = ctx.add(
            FixedSize::new()
                .width(LEFT_COLUMN_WIDTH)
                .height(PLAYER_HEIGHT)
                .child(player_panel),
        );
        let playlist_panel = ctx.add_boxed(panels.playlist);
        let playlist = ctx.add(Expand::new().child(playlist_panel));
        // `Expand::vertical` in a row claims the row's full height without
        // competing for its width: the column is as tall as the window.
        let left = ctx.add(
            Expand::vertical().child(
                FixedSize::new()
                    .width(LEFT_COLUMN_WIDTH)
                    .child(column().child(player).child(playlist)),
            ),
        );

        let title_bar = match self.host.clone() {
            Some(host) => ctx.add(title_bar::custom(host)),
            None => ctx.add(title_bar::native()),
        };
        let filters_panel = ctx.add_boxed(panels.filters);
        let filters = ctx.add(FixedSize::new().width(FILTERS_WIDTH).child(filters_panel));
        let filters_column = ctx.add(Expand::vertical().child(filters));
        let library_panel = ctx.add_boxed(panels.library);
        let library = ctx.add(Expand::new().child(library_panel));
        let body = ctx.add(row().child(filters_column).child(library));
        let right = ctx.add(
            Expand::new().child(
                column()
                    .child(Expand::horizontal().child(title_bar))
                    .child(Expand::new().child(body)),
            ),
        );

        let main = ctx.add(row().child(left).child(right));
        // The Audio Center floats over the main layout (`app.rs:5160`): a
        // top-leading layer that lets the pointer through outside its frame.
        let audio_center = ctx.add_boxed(panels.audio_center);
        let root = ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .background(SurfaceRole::Main)
                .child(
                    ZStack::new()
                        .alignment(Alignment::TOP_LEADING)
                        .child(main)
                        .child(audio_center),
                ),
        );
        self.regions.set(Some(Regions {
            player,
            playlist,
            title_bar,
            filters,
            library,
        }));
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let size = self
            .root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0));
        // The root fills the window.
        proposal.resolve(size.width, size.height).into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
