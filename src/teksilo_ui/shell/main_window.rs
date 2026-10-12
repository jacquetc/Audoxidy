//! The main window: its configuration, its root and its close path.
//!
//! Like the Iced window (`src/main.rs`), it opens at the configuration
//! profile's size, never smaller than 900 × 500, without native decorations.
//! Teksilo then restores the size and position it had when it was last closed
//! (the window has a stable id and the settings bundle keeps window state).
//!
//! Closing it, by its close button, the OS or a shortcut, runs the close
//! guard: the features' close hooks, then the save of the playback position.
//! (Iced also saves the equalizer and the effects when the Audio Center
//! closes; the facade stores every such change as it is made.) Once the
//! window is gone, playback stops and the facade's threads stop after writing
//! every queued save (quirk fix 3; the Iced UI calls `exit(0)` and saves
//! nothing).

use std::cell::RefCell;
use std::rc::Rc;

use teksilo::core::{PlatformTitleBarHost, WidgetTree};
use teksilo::prelude::*;
use teksilo::widgets::WindowFrame;

use crate::frontend::{AppContext, session};
use crate::teksilo_ui::app::{App, SessionState};

/// The window's stable id: its geometry is saved and restored under it.
pub const WINDOW_ID: &str = "main";
/// The window title: the application's name.
pub const TITLE: &str = "Audoxidy";
/// The smallest size the window can take (`src/main.rs`).
pub const MIN_SIZE: (u32, u32) = (900, 500);

/// A close hook: work to do while the window can still be read, before it
/// closes.
pub type CloseHook = Rc<dyn Fn(&AppContext)>;

/// The close hooks registered by the features, run in registration order.
#[derive(Clone, Default)]
pub struct CloseHooks {
    hooks: Rc<RefCell<Vec<CloseHook>>>,
}

impl std::fmt::Debug for CloseHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloseHooks")
            .field("count", &self.hooks.borrow().len())
            .finish()
    }
}

impl CloseHooks {
    /// Adds `hook`.
    pub fn register(&self, hook: impl Fn(&AppContext) + 'static) {
        self.hooks.borrow_mut().push(Rc::new(hook));
    }

    /// Runs every hook. A hook may register another; it runs at the next
    /// close.
    pub fn run(&self, ctx: &AppContext) {
        let hooks: Vec<CloseHook> = self.hooks.borrow().clone();
        for hook in hooks {
            hook(ctx);
        }
    }
}

/// What the window's root and close path need. Cloning shares everything.
#[derive(Clone, Debug)]
pub struct AppHandles {
    /// The facade.
    pub ctx: AppContext,
    /// The playback session.
    pub session: SessionState,
    /// The close hooks.
    pub close_hooks: CloseHooks,
}

impl AppHandles {
    /// The handles over `ctx`, with a session not yet restored and no close
    /// hook.
    pub fn new(ctx: AppContext) -> Self {
        Self {
            ctx,
            session: SessionState::new(),
            close_hooks: CloseHooks::default(),
        }
    }

    /// The close guard's work: the close hooks, then the save of the playback
    /// position. The saves are queued on the database worker; they are
    /// written before it stops, in [`AppHandles::after_close`].
    pub fn before_close(&self) {
        self.close_hooks.run(&self.ctx);
        let _ = self.session.persist_now(&self.ctx);
    }

    /// Once the window is gone: stops playback and the facade's threads.
    pub fn after_close(&self) {
        if !session::stop(&self.ctx) {
            tracing::warn!("the backend threads did not all stop in time");
        }
    }
}

/// The main window's configuration.
pub fn main_window(handles: AppHandles) -> WindowConfig {
    let (width, height) = handles.ctx.config.ui.window_size();
    let (width, height) = initial_size(width, height);
    let on_close = handles.clone();
    let on_removed = handles.clone();
    WindowConfig::new()
        .id(WINDOW_ID)
        .title(TITLE)
        .size(width, height)
        .min_size(MIN_SIZE.0, MIN_SIZE.1)
        .decorations(DecorationsMode::CustomChrome)
        .on_close_requested(move |_ctx| {
            on_close.before_close();
            CloseResponse::Close
        })
        .on_removed(move |_event| on_removed.after_close())
        .root(move |tree, _state| {
            let host = tree.title_bar_host();
            root(tree, handles, host)
        })
}

/// The configured size in whole logical pixels, never below [`MIN_SIZE`].
pub fn initial_size(width: f32, height: f32) -> (u32, u32) {
    let clamp = |value: f32, min: u32| {
        if value.is_finite() && value >= min as f32 {
            value.round() as u32
        } else {
            min
        }
    };
    (clamp(width, MIN_SIZE.0), clamp(height, MIN_SIZE.1))
}

/// Adds the root: the app, inside a resize frame where the platform leaves
/// resizing a borderless window to the client (Wayland, X11).
pub fn root(
    tree: &mut WidgetTree,
    handles: AppHandles,
    host: Option<Rc<dyn PlatformTitleBarHost>>,
) -> WidgetId {
    let app = App::new(handles, host.clone());
    match host {
        Some(host) if host.needs_custom_resize_handles() => {
            tree.add(WindowFrame::new(host).content(app))
        }
        _ => tree.add(app),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn the_initial_size_is_the_profile_size_within_the_minimum() {
        assert_eq!(initial_size(1360.0, 880.0), (1360, 880));
        assert_eq!(initial_size(1024.4, 720.6), (1024, 721));
        assert_eq!(initial_size(640.0, 480.0), MIN_SIZE);
        assert_eq!(initial_size(f32::NAN, f32::INFINITY), MIN_SIZE);
    }

    #[test]
    fn close_hooks_run_in_order_every_close() {
        let ctx = AppContext::in_memory().unwrap();
        let hooks = CloseHooks::default();
        let order = Rc::new(RefCell::new(Vec::new()));
        for n in 0..3 {
            let order = order.clone();
            hooks.register(move |_| order.borrow_mut().push(n));
        }
        hooks.run(&ctx);
        hooks.run(&ctx);
        assert_eq!(*order.borrow(), vec![0, 1, 2, 0, 1, 2]);
        assert!(ctx.shutdown());
    }

    #[test]
    fn a_hook_registered_by_a_hook_runs_at_the_next_close() {
        let ctx = AppContext::in_memory().unwrap();
        let hooks = CloseHooks::default();
        let late = Rc::new(Cell::new(0));
        let (inner_hooks, inner_late) = (hooks.clone(), late.clone());
        hooks.register(move |_| {
            let late = inner_late.clone();
            inner_hooks.register(move |_| late.set(late.get() + 1));
        });
        hooks.run(&ctx);
        assert_eq!(late.get(), 0);
        hooks.run(&ctx);
        assert_eq!(late.get(), 1);
        assert!(ctx.shutdown());
    }

    /// A title-bar host for headless tests: no OS window behind it.
    struct FakeHost {
        resize_handles: bool,
    }

    impl PlatformTitleBarHost for FakeHost {
        fn reserved_leading_inset(&self) -> teksilo::canvas::Size {
            teksilo::canvas::Size::ZERO
        }
        fn reserved_trailing_inset(&self) -> teksilo::canvas::Size {
            teksilo::canvas::Size::ZERO
        }
        fn renders_custom_controls(&self) -> bool {
            true
        }
        fn needs_custom_resize_handles(&self) -> bool {
            self.resize_handles
        }
        fn begin_drag(&self) -> Result<(), teksilo::core::PlatformError> {
            Ok(())
        }
        fn begin_resize(
            &self,
            _edge: teksilo::core::ResizeEdge,
        ) -> Result<(), teksilo::core::PlatformError> {
            Ok(())
        }
        fn show_window_menu(
            &self,
            _at: teksilo::canvas::Point,
        ) -> Result<(), teksilo::core::PlatformError> {
            Ok(())
        }
        fn update_hit_regions(&self, _regions: &teksilo::core::HitRegions) {}
    }

    fn mounted_root(resize_handles: bool) -> (WidgetTree, WidgetId, AppContext) {
        let ctx = AppContext::in_memory().unwrap();
        let (mut tree, _runtime) = crate::teksilo_ui::test_support::tree_with_async(&ctx);
        let host: Rc<dyn PlatformTitleBarHost> = Rc::new(FakeHost { resize_handles });
        let id = root(&mut tree, AppHandles::new(ctx.clone()), Some(host));
        crate::teksilo_ui::test_support::layout(&mut tree);
        (tree, id, ctx)
    }

    #[test]
    fn with_custom_chrome_the_strip_is_a_title_bar_inside_a_resize_frame() {
        use crate::teksilo_ui::test_support::first_of_type;
        let (tree, id, ctx) = mounted_root(true);
        assert!(
            tree.widget_type_name(id)
                .is_some_and(|n| n.ends_with("WindowFrame"))
        );
        let bar = first_of_type(&tree, id, "TitleBar").unwrap();
        let bounds = tree.bounds(bar);
        assert_eq!((bounds.x, bounds.y, bounds.height), (400.0, 0.0, 40.0));
        assert!(ctx.shutdown());
    }

    #[test]
    fn without_resize_handles_the_root_is_the_app_itself() {
        use crate::teksilo_ui::test_support::first_of_type;
        let (tree, id, ctx) = mounted_root(false);
        assert!(
            tree.widget_type_name(id)
                .is_some_and(|n| n.ends_with("App"))
        );
        assert!(first_of_type(&tree, id, "WindowFrame").is_none());
        assert!(first_of_type(&tree, id, "TitleBar").is_some());
        assert!(ctx.shutdown());
    }

    #[test]
    fn quirk_3_closing_a_restored_session_stores_the_position_the_player_reached() {
        use std::time::Duration;

        use crate::frontend::commands::playback;
        use crate::teksilo_ui::mock_data::screenshot_library;
        use crate::teksilo_ui::test_support::run_until;

        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        screenshot_library()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        let runtime = AsyncRuntimeHandle::new();
        let handles = AppHandles::new(ctx.clone());
        handles.session.start_restore(&ctx, &runtime);
        let status = handles.session.status();
        assert!(run_until(&runtime, Duration::from_secs(10), || status
            .get()
            .is_settled()));
        let (playlist, song) = handles
            .session
            .with_cursor(|cursor| {
                let song = cursor.song_at(1).map(|song| song.song_id).unwrap();
                playback::play_song(&ctx, cursor, song).unwrap();
                (cursor.playlist_id(), song)
            })
            .unwrap();
        fakes.player.advance(77.0);
        assert_eq!(
            ctx.player.snapshot().current_pos_sec,
            77.0,
            "the song plays"
        );

        handles.before_close();
        handles.after_close();

        assert!(ctx.is_shut_down());
        // The workers are gone: read the connection directly.
        let connection = ctx.db.connection();
        let db = connection.lock().unwrap();
        let stored = db.get_playlist_by_id(playlist).unwrap().unwrap();
        assert_eq!(stored.last_song_id, Some(song));
        assert_eq!(stored.last_pos_sec, 77.0, "the position the player reached");
        assert!(stored.is_playing);
    }

    #[test]
    fn closing_runs_the_hooks_and_stops_the_backend() {
        let ctx = AppContext::in_memory().unwrap();
        let handles = AppHandles::new(ctx.clone());
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        handles.close_hooks.register(move |_| flag.set(true));
        handles.before_close();
        assert!(ran.get());
        assert!(!ctx.is_shut_down());
        handles.after_close();
        assert!(ctx.is_shut_down());
    }
}
