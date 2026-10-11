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
//! of the window.

pub mod services;
pub mod session;

use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::PlatformTitleBarHost;
#[cfg(test)]
use teksilo::core::WidgetTree;
use teksilo::prelude::*;
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{Expand, FixedSize, HStack, Panel, Spacer, VStack};

pub use self::services::{AppServices, AudioCenterLink, Request, UiLinks, audio_center_tab};
pub use self::session::{RestoreStatus, SessionState};
use crate::teksilo_ui::filters::{FiltersPanel, FiltersViewModel};
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

/// The root widget; see the module documentation.
pub struct App {
    handles: AppHandles,
    services: Option<AppServices>,
    host: Option<Rc<dyn PlatformTitleBarHost>>,
    regions: Rc<Cell<Option<Regions>>>,
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
            root: None,
        }
    }

    /// A handle on the regions, filled when the root is built.
    pub fn regions(&self) -> Rc<Cell<Option<Regions>>> {
        self.regions.clone()
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

impl App {
    /// Adds the panel `make` builds from the services, or an empty slot when
    /// there are none (no async runtime: logged by `start_session`).
    fn panel(
        &self,
        ctx: &mut BuildContext,
        make: impl FnOnce(&AppServices) -> Box<dyn Widget>,
    ) -> WidgetId {
        match &self.services {
            Some(services) => ctx.add_boxed(make(services)),
            None => ctx.add(Spacer::new()),
        }
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

        let player = ctx.add(
            FixedSize::new()
                .width(LEFT_COLUMN_WIDTH)
                .height(PLAYER_HEIGHT)
                .child(Spacer::new()),
        );
        let playlist = ctx.add(Expand::new().child(Spacer::new()));
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
        let filters_panel = self.panel(ctx, |services| {
            Box::new(FiltersPanel::new(FiltersViewModel::new(services)))
        });
        let filters = ctx.add(FixedSize::new().width(FILTERS_WIDTH).child(filters_panel));
        let filters_column = ctx.add(Expand::vertical().child(filters));
        let library = ctx.add(Expand::new().child(Spacer::new()));
        let body = ctx.add(row().child(filters_column).child(library));
        let right = ctx.add(
            Expand::new().child(
                column()
                    .child(Expand::horizontal().child(title_bar))
                    .child(Expand::new().child(body)),
            ),
        );

        let root = ctx.add(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .background(SurfaceRole::Main)
                .child(row().child(left).child(right)),
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
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::frontend::AppContext;
    use crate::frontend::test_fixtures::LibraryFixture;
    use crate::teksilo_ui::test_support::{VIEWPORT, layout, run_until, tree_with_async};

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn mounted() -> (WidgetTree, AsyncRuntimeHandle, AppHandles, Regions) {
        let ctx = AppContext::in_memory().unwrap();
        LibraryFixture::demo()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        let handles = AppHandles::new(ctx.clone());
        let (mut tree, runtime) = tree_with_async(&ctx);
        let app = App::new(handles.clone(), None);
        let regions = app.regions();
        tree.add(app);
        layout(&mut tree);
        let regions = regions.get().unwrap();
        (tree, runtime, handles, regions)
    }

    fn rect(tree: &WidgetTree, id: WidgetId) -> (f32, f32, f32, f32) {
        let b = tree.bounds(id);
        (b.x, b.y, b.width, b.height)
    }

    #[test]
    fn the_regions_have_the_iced_geometry() {
        let (tree, _runtime, handles, r) = mounted();
        let (w, h) = VIEWPORT;
        assert_eq!(rect(&tree, r.player), (0.0, 0.0, 400.0, 400.0));
        assert_eq!(rect(&tree, r.playlist), (0.0, 400.0, 400.0, h - 400.0));
        assert_eq!(rect(&tree, r.title_bar), (400.0, 0.0, w - 400.0, 40.0));
        assert_eq!(rect(&tree, r.filters), (400.0, 40.0, 202.0, h - 40.0));
        assert_eq!(rect(&tree, r.library), (602.0, 40.0, w - 602.0, h - 40.0));
        assert!(handles.ctx.shutdown());
    }

    #[test]
    fn the_panels_are_mounted_in_their_regions() {
        let (tree, _runtime, handles, r) = mounted();
        assert!(
            crate::teksilo_ui::test_support::first_of_type(&tree, r.filters, "FiltersPanel")
                .is_some()
        );
        assert!(handles.ctx.shutdown());
    }

    #[test]
    fn building_the_root_restores_the_session_once() {
        let (mut tree, runtime, handles, _) = mounted();
        let status = handles.session.status();
        assert!(run_until(&runtime, TIMEOUT, || status.get().is_settled()));
        assert!(matches!(status.get(), RestoreStatus::Restored(_)));
        // A relayout does not start a second restore (the facade would refuse it).
        layout(&mut tree);
        runtime.tick();
        assert!(matches!(status.get(), RestoreStatus::Restored(_)));
        assert!(handles.ctx.shutdown());
    }
}
