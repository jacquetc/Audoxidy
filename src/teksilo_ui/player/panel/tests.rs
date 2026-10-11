//! Tests of the player panel: the geometry of every layer at Iced's
//! coordinates, the control zones, the wheel, the seek bar, the menu, the
//! accessible names and the Spanish strings.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use teksilo::canvas::Point;
use teksilo::core::accesskit::Role;
use teksilo::core::widget_tree::WidgetTree;
use teksilo::core::{Key, Modifiers, ScrollDelta, WidgetEvent};
use teksilo::prelude::AsyncRuntimeHandle;
use teksilo::widgets::{ToastInstallOptions, ToastRegistry};

use super::*;
use crate::frontend::events::{PlaybackEvent, PlaybackSnapshot};
use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
use crate::frontend::{AppContext, Event};
use crate::teksilo_ui::app::audio_center_tab;
use crate::teksilo_ui::app::services::AppServices;
use crate::teksilo_ui::mock_data::screenshot_library;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{
    VIEWPORT, access_click, first_of_type, layout, press, run_until, tree_with_app_state,
    with_shipped_messages,
};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Everything a panel test needs.
struct Fixture {
    services: AppServices,
    fakes: InMemoryFakes,
    runtime: AsyncRuntimeHandle,
    toasts: ToastRegistry,
    tree: WidgetTree,
    panel: WidgetId,
    vm: PlayerViewModel,
}

impl Fixture {
    /// A panel at `(left, 0)` over services whose restore has settled when
    /// `restored` is true.
    fn new(left: f32, inset: f32, restored: bool) -> Self {
        Self::in_locale("es-ES", left, inset, restored)
    }

    /// [`Fixture::new`] with the shipped messages of `locale` installed for
    /// the rest of the test (every test runs on its own thread).
    fn in_locale(locale: &str, left: f32, inset: f32, restored: bool) -> Self {
        install_locale(locale);
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        screenshot_library()
            .seed_on(&ctx.db)
            .recv_blocking()
            .unwrap();
        let runtime = AsyncRuntimeHandle::new();
        let toasts = ToastRegistry::new(ToastInstallOptions {
            archive: None,
            ..ToastInstallOptions::default()
        });
        let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
        state.insert(
            TypeId::of::<AsyncRuntimeHandle>(),
            Box::new(runtime.clone()),
        );
        state.insert(TypeId::of::<ToastRegistry>(), Box::new(toasts.clone()));
        let mut tree = tree_with_app_state(&ctx, state);
        let services = AppServices::new(&AppHandles::new(ctx), runtime.clone());
        if restored {
            services
                .session
                .start_restore(&services.ctx, &services.runtime);
            let status = services.session.status();
            assert!(run_until(&runtime, TIMEOUT, || status.get().is_settled()));
        }
        let vm = PlayerViewModel::new(&services);
        let panel = tree.add(PlayerPanel::new(vm.clone()).leading_inset(inset));
        tree.add(
            HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Top)
                .child(gap(left))
                .child(panel)
                .child(Spacer::new()),
        );
        layout(&mut tree);
        let _ = tree.render();
        Self {
            services,
            fakes,
            runtime,
            toasts,
            tree,
            panel,
            vm,
        }
    }

    fn named(&self, name: &str) -> WidgetId {
        match self.tree.find_by_label(name) {
            Some(id) => id,
            None => panic!("no widget named {name:?}"),
        }
    }

    fn rect(&self, id: WidgetId) -> (f32, f32, f32, f32) {
        let b = self.tree.bounds(id);
        (b.x, b.y, b.width, b.height)
    }

    /// Plays `title` / `artist` as the monitor would announce them.
    fn play(&self, title: &str, artist: &str, duration: f64) {
        let mut state = PlaybackSnapshot::from_state(&self.services.ctx.player.snapshot());
        state.title = title.to_string();
        state.artist = artist.to_string();
        state.is_playing = true;
        state.duration_secs = duration;
        self.services
            .now_playing
            .on_event(&Event::Playback(PlaybackEvent::TrackChanged(Arc::new(
                state,
            ))));
    }

    fn settle(&mut self) {
        layout(&mut self.tree);
        let _ = self.tree.render();
    }

    fn finish(self) {
        assert!(self.services.ctx.shutdown());
    }
}

/// Installs the shipped messages of `locale` on this thread, as
/// `with_shipped_messages` does, and leaves them installed.
fn install_locale(locale: &str) {
    use teksilo::i18n::config::I18nConfig;
    use teksilo::i18n::manager::I18nManager;
    use teksilo::i18n::thread_local::{clear, install};
    let Some(&(tag, files)) = crate::teksilo_ui::startup::app_locales()
        .iter()
        .find(|(tag, _)| *tag == locale)
    else {
        panic!("{locale} is not a locale the app ships");
    };
    let id: teksilo::i18n::LanguageIdentifier = tag.parse().unwrap();
    clear();
    let config = I18nConfig::new()
        .source_locale(id.clone())
        .supported_locales([id.clone()])
        .compile_in(&[(tag, files)])
        .auto_detect_os_locale(false)
        .fallback_locale(id);
    install(I18nManager::from_config(&config));
}

fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

fn move_to(fixture: &mut Fixture, x: f32, y: f32) {
    use teksilo::core::{EventTime, PointerPhase, PointerSample};
    fixture.tree.dispatch_pointer(PointerSample::mouse(
        PointerPhase::Move,
        Point::new(x, y),
        EventTime::from_millis(1),
    ));
    layout(&mut fixture.tree);
}

/// A primary-button tap at `at`: the legacy events hit-test at the point and
/// are recognised as a tap (samples through `dispatch_pointer` are not).
fn tap_at(tree: &mut WidgetTree, at: Point) {
    use teksilo::core::event::PointerButton;
    tree.dispatch_event(WidgetEvent::pointer_down(
        at,
        PointerButton::Primary,
        Modifiers::NONE,
    ));
    tree.dispatch_event(WidgetEvent::pointer_up(
        at,
        PointerButton::Primary,
        Modifiers::NONE,
    ));
    layout(tree);
}

fn wheel(fixture: &mut Fixture, y: f32, at: Point) {
    move_to(fixture, at.x, at.y);
    fixture.tree.dispatch_event(WidgetEvent::scroll_at(
        ScrollDelta::Lines { x: 0.0, y },
        Modifiers::NONE,
        at,
    ));
    layout(&mut fixture.tree);
}

// --- Geometry ------------------------------------------------------------------------

#[test]
fn the_panel_is_400_by_400() {
    let f = Fixture::new(0.0, 0.0, false);
    assert_eq!(f.rect(f.panel), (0.0, 0.0, 400.0, 400.0));
    f.finish();
}

#[test]
fn the_panel_fills_its_region_wherever_it_is() {
    let f = Fixture::new(50.0, 0.0, false);
    assert_eq!(f.rect(f.panel), (50.0, 0.0, 400.0, 400.0));
    let _ = VIEWPORT;
    f.finish();
}

#[test]
fn the_cover_fills_the_panel() {
    let f = Fixture::new(0.0, 0.0, false);
    let art = first_of_type(&f.tree, f.panel, "AlbumArt").unwrap();
    assert_eq!(f.rect(art), (0.0, 0.0, 400.0, 400.0));
    f.finish();
}

#[test]
fn the_menu_button_is_31_pixels_8_in_and_centred_in_the_top_row() {
    let f = Fixture::new(0.0, 0.0, false);
    let button = f.named("Menú");
    assert_eq!(f.rect(button), (8.0, 4.5, 31.0, 31.0));
    f.finish();
}

#[test]
fn the_leading_inset_moves_the_menu_button() {
    let f = Fixture::new(0.0, 70.0, false);
    let button = f.named("Menú");
    assert_eq!(f.rect(button).0, 78.0);
    f.finish();
}

#[test]
fn the_title_and_artist_rows_sit_where_iced_puts_them() {
    let f = Fixture::new(0.0, 0.0, true);
    f.play("Heroes", "David Bowie", 200.0);
    let mut f = f;
    f.settle();
    let title = f.tree.find_by_label("Heroes").unwrap();
    let artist = f.tree.find_by_label("David Bowie").unwrap();
    let (tx, ty, _, th) = f.rect(title);
    let (ax, ay, _, ah) = f.rect(artist);
    assert_eq!(tx, 12.0, "10 px of row padding and 2 of cell padding");
    assert_eq!(ax, 12.0);
    // The title row is 22 px at y 302; the artist row 36 px at y 324.
    assert!(ty >= 302.0 && ty + th <= 324.0 + 0.5, "{ty} {th}");
    assert!(ay >= 324.0 && ay + ah <= 360.0, "{ay} {ah}");
    f.finish();
}

#[test]
fn the_elapsed_time_ends_two_pixels_before_the_artist_rows_padding() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.play("Heroes", "David Bowie", 200.0);
    f.services
        .now_playing
        .on_event(&Event::Playback(PlaybackEvent::PositionChanged {
            position_secs: 95.0,
            duration_secs: 200.0,
        }));
    f.settle();
    let time = f.tree.find_by_label("01:35").unwrap();
    let (x, _, w, _) = f.rect(time);
    assert_eq!(x + w, 388.0);
    f.finish();
}

#[test]
fn the_info_is_hidden_until_the_track_plays_or_moves() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.settle();
    assert!(!f.vm.info_visible().get());
    f.play("Heroes", "David Bowie", 200.0);
    assert!(f.vm.info_visible().get());
    f.finish();
}

#[test]
fn the_seek_slider_is_374_by_10_at_13_and_375() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.play("A", "B", 200.0);
    f.settle();
    let slider = first_of_type(&f.tree, f.panel, "Slider").unwrap();
    assert_eq!(f.rect(slider), (13.0, 375.0, 374.0, 10.0));
    f.finish();
}

#[test]
fn the_seek_strip_is_40_pixels_at_the_bottom() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.play("A", "B", 200.0);
    f.settle();
    let strip = first_of_type(&f.tree, f.panel, "SeekBar").unwrap();
    assert_eq!(f.rect(strip), (0.0, 360.0, 400.0, 40.0));
    f.finish();
}

#[test]
fn the_seek_slider_is_announced_and_a_flat_rail_replaces_it_when_the_duration_is_unknown() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.play("A", "B", 200.0);
    f.settle();
    assert!(first_of_type(&f.tree, f.panel, "Slider").is_some());
    assert!(f.tree.find_by_role(Role::Slider).is_some());
    f.play("A", "B", 0.0);
    f.settle();
    assert!(
        first_of_type(&f.tree, f.panel, "Slider").is_none(),
        "no slider to drag without a duration"
    );
    assert!(f.tree.find_by_role(Role::Slider).is_none());
    f.finish();
}

#[test]
fn the_three_zones_split_the_band_in_thirds() {
    let f = Fixture::new(0.0, 0.0, true);
    let previous = f.rect(f.named("Pista anterior"));
    let play = f.rect(f.named("Reproducir o pausar"));
    let next = f.rect(f.named("Pista siguiente"));
    let third = 400.0 / 3.0;
    for (zone, index) in [(previous, 0.0), (play, 1.0), (next, 2.0)] {
        assert!((zone.0 - index * third).abs() < 0.01, "{zone:?}");
        assert!((zone.2 - third).abs() < 0.01, "{zone:?}");
        assert_eq!((zone.1, zone.3), (40.0, 320.0));
    }
    f.finish();
}

// --- The zones -------------------------------------------------------------------------

#[test]
fn the_zones_are_disabled_until_the_restore_settles() {
    let mut f = Fixture::new(0.0, 0.0, false);
    for name in ["Pista anterior", "Reproducir o pausar", "Pista siguiente"] {
        let id = f.named(name);
        assert!(f.tree.accessibility_node(id).is_disabled(), "{name}");
    }
    let play = f.named("Reproducir o pausar");
    access_click(&mut f.tree, play);
    assert!(f.fakes.player.calls().is_empty(), "quirk fix 6");
    f.services
        .session
        .start_restore(&f.services.ctx, &f.services.runtime);
    let status = f.services.session.status();
    assert!(run_until(&f.runtime, TIMEOUT, || status.get().is_settled()));
    f.settle();
    assert!(!f.tree.accessibility_node(play).is_disabled());
    f.finish();
}

#[test]
fn the_middle_zone_toggles_play_and_pause() {
    let mut f = Fixture::new(0.0, 0.0, true);
    let play = f.named("Reproducir o pausar");
    access_click(&mut f.tree, play);
    assert!(
        f.fakes
            .player
            .calls()
            .contains(&PlayerCall::TogglePlayPause)
    );
    f.finish();
}

#[test]
fn the_side_zones_change_track() {
    let mut f = Fixture::new(0.0, 0.0, true);
    let version = f.services.session.cursor_version();
    let before = version.get();
    let next = f.named("Pista siguiente");
    access_click(&mut f.tree, next);
    assert!(version.get() > before, "Next moved the cursor");
    let after_next = version.get();
    let previous = f.named("Pista anterior");
    access_click(&mut f.tree, previous);
    assert!(version.get() > after_next, "Previous moved it back");
    f.finish();
}

#[test]
fn a_click_in_a_zone_activates_it_through_the_pointer() {
    let mut f = Fixture::new(0.0, 0.0, true);
    let play = f.rect(f.named("Reproducir o pausar"));
    let at = Point::new(play.0 + play.2 / 2.0, play.1 + 60.0);
    tap_at(&mut f.tree, at);
    assert!(
        f.fakes
            .player
            .calls()
            .contains(&PlayerCall::TogglePlayPause)
    );
    f.finish();
}

#[test]
fn the_pointer_in_a_zone_lights_it_while_the_user_is_active() {
    let mut f = Fixture::new(50.0, 0.0, true);
    // The panel is at x 50: its local x for the left third is 20.
    move_to(&mut f, 50.0 + 20.0, 200.0);
    assert_eq!(
        f.vm.hover_zone().get(),
        HoverZone::Previous,
        "panel-local coordinates"
    );
    move_to(&mut f, 50.0 + 200.0, 200.0);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::PlayPause);
    move_to(&mut f, 50.0 + 350.0, 200.0);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::Next);
    assert!(f.vm.controls_active().get());
    f.finish();
}

#[test]
fn leaving_the_panel_clears_the_zone() {
    let mut f = Fixture::new(0.0, 0.0, true);
    move_to(&mut f, 200.0, 200.0);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::PlayPause);
    move_to(&mut f, 700.0, 500.0);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::None);
    f.finish();
}

// --- The wheel and the volume bubble ------------------------------------------------------

#[test]
fn the_wheel_over_the_band_moves_the_volume_five_percent_and_shows_the_bubble() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.vm.set_volume(0.5);
    // Iced's wheel up is Teksilo's negative offset.
    wheel(&mut f, -1.0, Point::new(200.0, 200.0));
    assert_eq!(f.vm.volume().get(), Some(55.0));
    wheel(&mut f, 1.0, Point::new(200.0, 200.0));
    wheel(&mut f, 1.0, Point::new(200.0, 200.0));
    assert_eq!(f.vm.volume().get(), Some(45.0));
    assert!((f.services.ctx.player.snapshot().volume - 0.45).abs() < 1e-6);
    f.finish();
}

#[test]
fn the_wheel_outside_the_band_does_not_change_the_volume() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.vm.set_volume(0.5);
    wheel(&mut f, -1.0, Point::new(200.0, 20.0));
    wheel(&mut f, -1.0, Point::new(200.0, 380.0));
    assert_eq!(f.vm.volume().get(), None);
    assert!((f.services.ctx.player.snapshot().volume - 0.5).abs() < 1e-6);
    f.finish();
}

#[test]
fn a_wheel_with_no_turn_is_not_a_step() {
    let mut f = Fixture::new(0.0, 0.0, true);
    wheel(&mut f, 0.0, Point::new(200.0, 200.0));
    assert_eq!(f.vm.volume().get(), None);
    f.finish();
}

#[test]
fn the_bubble_shows_the_percentage_and_the_speaker_for_it() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.vm.set_volume(0.5);
    wheel(&mut f, -1.0, Point::new(200.0, 200.0));
    f.settle();
    let text = f.tree.find_by_label("55").expect("the percentage is drawn");
    let (x, y, w, h) = f.rect(text);
    // The bubble is centred in the band (x 200, y 200); its text is the
    // lower part, 13 px above the bubble's bottom padding.
    assert!(x < 200.0 && x + w > 200.0, "{x} {w}");
    assert!(y > 200.0 && y + h < 280.0, "{y} {h}");
    f.finish();
}

#[test]
fn the_bubble_takes_no_input() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.vm.set_volume(0.5);
    wheel(&mut f, -1.0, Point::new(200.0, 200.0));
    f.settle();
    // A click through the bubble reaches the zone under it.
    let at = Point::new(200.0, 220.0);
    tap_at(&mut f.tree, at);
    assert!(
        f.fakes
            .player
            .calls()
            .contains(&PlayerCall::TogglePlayPause)
    );
    assert_eq!(f.vm.volume().get(), None, "and the click hid the bubble");
    f.finish();
}

#[test]
fn the_wheel_does_nothing_before_the_restore_settles() {
    let mut f = Fixture::new(0.0, 0.0, false);
    wheel(&mut f, -1.0, Point::new(200.0, 200.0));
    assert_eq!(f.vm.volume().get(), None);
    assert!(f.fakes.player.calls().is_empty());
    f.finish();
}

// --- The marquee's subscription ------------------------------------------------------------

#[test]
fn the_marquee_holds_a_frame_subscription_only_while_it_scrolls() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.settle();
    assert!(
        f.tree.frame_tick_deadline().is_none(),
        "nothing subscribed at rest"
    );
    f.play(
        "A title that is far too long to fit in the player slot",
        "B",
        200.0,
    );
    f.settle();
    assert!(f.vm.marquee_running().get());
    f.settle();
    assert!(
        f.tree.frame_tick_deadline().is_some(),
        "the guard is held while the marquee runs"
    );
    // Pausing drops it.
    let mut state = PlaybackSnapshot::from_state(&f.services.ctx.player.snapshot());
    state.is_playing = false;
    state.title = "A title that is far too long to fit in the player slot".to_string();
    f.services
        .now_playing
        .on_event(&Event::Playback(PlaybackEvent::StateChanged(Arc::new(
            state,
        ))));
    f.settle();
    f.settle();
    assert!(!f.vm.marquee_running().get());
    assert!(
        f.tree.frame_tick_deadline().is_none(),
        "no guard when paused"
    );
    f.finish();
}

#[test]
fn a_short_title_never_subscribes() {
    let mut f = Fixture::new(0.0, 0.0, true);
    f.play("Heroes", "David Bowie", 200.0);
    f.settle();
    f.settle();
    assert!(!f.vm.marquee_running().get());
    assert!(f.tree.frame_tick_deadline().is_none());
    f.finish();
}

// --- The menu -----------------------------------------------------------------------------------

fn open_menu(f: &mut Fixture) {
    let button = f
        .tree
        .find_by_label("Menú")
        .or_else(|| f.tree.find_by_label("Menu"))
        .expect("the menu button");
    access_click(&mut f.tree, button);
    let _ = f.tree.render();
}

fn menu_items(f: &Fixture) -> Vec<(String, bool)> {
    let menu = f.tree.find_by_role(Role::Menu).expect("the menu is open");
    let mut items = Vec::new();
    let mut stack = vec![menu];
    while let Some(id) = stack.pop() {
        let node = f.tree.accessibility_node(id);
        if node.role() == Role::MenuItem {
            items.push((
                node.name().unwrap_or_default().to_string(),
                node.is_disabled(),
            ));
        }
        let mut children = f.tree.children(id);
        children.reverse();
        stack.extend(children);
    }
    items
}

#[test]
fn the_menu_button_opens_the_menu_below_it() {
    let mut f = Fixture::new(0.0, 0.0, true);
    assert!(f.tree.active_overlays().is_empty());
    open_menu(&mut f);
    let overlay = f.tree.active_overlays()[0];
    let bounds = f.tree.overlay_content_bounds(overlay).unwrap();
    let button = f.rect(f.named("Menú"));
    // Quirk fix 7: anchored to the button, not at a fixed place.
    assert!(
        bounds.y >= button.1 + button.3 - 0.5,
        "{bounds:?} {button:?}"
    );
    assert!(
        bounds.y <= button.1 + button.3 + 12.0,
        "{bounds:?} {button:?}"
    );
    assert!((bounds.x - button.0).abs() <= 8.0, "{bounds:?} {button:?}");
    assert_eq!(bounds.width, 180.0);
    f.finish();
}

#[test]
fn the_menu_follows_the_button_when_the_panel_moves_or_has_an_inset() {
    let mut f = Fixture::new(120.0, 0.0, true);
    open_menu(&mut f);
    let overlay = f.tree.active_overlays()[0];
    let bounds = f.tree.overlay_content_bounds(overlay).unwrap();
    assert!(bounds.x >= 120.0, "{bounds:?}");
    f.finish();
}

#[test]
fn the_menu_lists_iceds_entries_in_order_with_nine_stubs_disabled() {
    let mut f = Fixture::new(0.0, 0.0, true);
    open_menu(&mut f);
    let items = menu_items(&f);
    let names: Vec<&str> = items.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Abrir Archivo",
            "Abrir Carpeta",
            "Biblioteca",
            "Lista de Reproducción",
            "Lyrics",
            "Configuración de Audio",
            "Ecualizador",
            "Efectos de Audio",
            "Apagado Automático",
            "Personalización",
            "Preferencias",
            "Complementos",
            "Acerca",
            "Salir",
        ]
        .map(|name| name)
    );
    let disabled: Vec<&str> = items
        .iter()
        .filter(|(_, disabled)| *disabled)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        disabled,
        [
            "Abrir Archivo",
            "Biblioteca",
            "Lista de Reproducción",
            "Lyrics",
            "Apagado Automático",
            "Personalización",
            "Preferencias",
            "Complementos",
            "Acerca",
        ]
    );
    f.finish();
}

#[test]
fn a_disabled_stub_does_nothing_and_leaves_the_menu_open() {
    let mut f = Fixture::new(0.0, 0.0, true);
    open_menu(&mut f);
    let stub = f.named("Biblioteca");
    access_click(&mut f.tree, stub);
    assert_eq!(f.tree.active_overlays().len(), 1);
    assert!(!f.services.links.audio_center.open.get());
    f.finish();
}

#[test]
fn the_audio_entries_open_the_audio_center_on_their_tab_and_close_the_menu() {
    let mut f = Fixture::new(0.0, 0.0, true);
    for (name, tab) in [
        ("Configuración de Audio", audio_center_tab::CONFIG),
        ("Ecualizador", audio_center_tab::EQUALIZER),
        ("Efectos de Audio", audio_center_tab::EFFECTS),
    ] {
        f.services.links.audio_center.close();
        open_menu(&mut f);
        let entry = f.named(name);
        access_click(&mut f.tree, entry);
        let link = &f.services.links.audio_center;
        assert!(link.open.get(), "{name}");
        assert_eq!(link.tab.get(), tab, "{name}");
        assert!(
            f.tree.active_overlays().is_empty(),
            "{name} closes the menu"
        );
    }
    f.finish();
}

#[test]
fn escape_closes_the_menu_without_acting() {
    let mut f = Fixture::new(0.0, 0.0, true);
    open_menu(&mut f);
    assert_eq!(f.tree.active_overlays().len(), 1);
    f.tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut f.tree);
    assert!(f.tree.active_overlays().is_empty(), "quirk fix 4");
    assert!(!f.services.links.audio_center.open.get());
    f.finish();
}

#[test]
fn a_click_outside_closes_the_menu() {
    let mut f = Fixture::new(0.0, 0.0, true);
    open_menu(&mut f);
    let at = Point::new(900.0, 600.0);
    tap_at(&mut f.tree, at);
    assert!(f.tree.active_overlays().is_empty());
    f.finish();
}

#[test]
fn quit_asks_the_window_to_close() {
    let mut f = Fixture::new(0.0, 0.0, true);
    open_menu(&mut f);
    let quit = f.named("Salir");
    access_click(&mut f.tree, quit);
    assert!(
        f.tree.take_close_window_request(),
        "the guarded close path runs"
    );
    f.finish();
}

#[test]
fn the_zones_show_no_feedback_while_the_menu_is_open() {
    let mut f = Fixture::new(0.0, 0.0, true);
    move_to(&mut f, 200.0, 200.0);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::PlayPause);
    open_menu(&mut f);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::None);
    f.tree.press_key(Key::Escape, Modifiers::NONE);
    layout(&mut f.tree);
    move_to(&mut f, 200.0, 200.0);
    assert_eq!(f.vm.hover_zone().get(), HoverZone::PlayPause);
    f.finish();
}

// --- Errors (quirk fix 8) -------------------------------------------------------------------------

#[test]
fn a_scan_that_cannot_start_shows_an_error_toast() {
    let mut f = Fixture::new(0.0, 0.0, true);
    assert_eq!(f.toasts.live_count(), 0);
    let vm = f.vm.clone();
    press(&mut f.tree, move |ctx| menu::scan(&vm, Path::new(""), ctx));
    assert_eq!(
        f.toasts.live_count(),
        1,
        "the failure is shown, not only logged"
    );
    f.finish();
}

#[test]
fn a_scan_that_starts_shows_no_toast() {
    let mut f = Fixture::new(0.0, 0.0, true);
    let dir =
        std::env::temp_dir().join(format!("audoxidy-player-panel-scan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let vm = f.vm.clone();
    let path = dir.clone();
    press(&mut f.tree, move |ctx| menu::scan(&vm, &path, ctx));
    assert_eq!(f.toasts.live_count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
    f.finish();
}

#[test]
fn a_folder_dialog_that_cannot_open_shows_an_error_toast() {
    // The headless tree has no file-dialog service, as a build without
    // `install_file_dialog` would not.
    let mut f = Fixture::new(0.0, 0.0, true);
    open_menu(&mut f);
    let open = f.named("Abrir Carpeta");
    access_click(&mut f.tree, open);
    assert_eq!(f.toasts.live_count(), 1);
    assert!(f.tree.active_overlays().is_empty(), "the menu closed");
    f.finish();
}

#[test]
fn a_track_that_cannot_be_played_shows_an_error_toast() {
    let mut f = Fixture::new(0.0, 0.0, true);
    press(&mut f.tree, |ctx| {
        report(ctx, &FrontendError::InvalidInput("the file is gone".into()));
    });
    assert_eq!(f.toasts.live_count(), 1);
    f.finish();
}

// --- Names and Spanish ---------------------------------------------------------------------------

#[test]
fn the_controls_have_accessible_names_in_spanish() {
    with_shipped_messages("es-ES", || {
        let f = Fixture::new(0.0, 0.0, true);
        for name in [
            "Menú",
            "Pista anterior",
            "Reproducir o pausar",
            "Pista siguiente",
        ] {
            let id = f.named(name);
            assert_eq!(f.tree.accessibility_node(id).role(), Role::Button, "{name}");
        }
        let player = f.tree.accessibility_node(f.panel);
        assert_eq!(player.name(), Some("Reproductor"));
        f.finish();
    });
}

#[test]
fn the_controls_have_english_names_too() {
    with_shipped_messages("en-US", || {
        let f = Fixture::in_locale("en-US", 0.0, 0.0, true);
        for name in ["Menu", "Previous track", "Play or pause", "Next track"] {
            f.named(name);
        }
        f.finish();
    });
}

#[test]
fn the_menu_is_in_english_when_the_locale_is() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::in_locale("en-US", 0.0, 0.0, true);
        open_menu(&mut f);
        let names: Vec<String> = menu_items(&f).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names.first().map(String::as_str), Some("Open File"));
        assert_eq!(names.last().map(String::as_str), Some("Quit"));
        assert!(names.contains(&"Lyrics".to_string()));
        f.finish();
    });
}

#[test]
fn the_error_messages_are_spanish_and_carry_the_cause() {
    with_shipped_messages("es-ES", || {
        let scan = tr!(player_scan_failed(error = "disco lleno".to_string())).resolve_now();
        assert!(
            scan.starts_with("No se pudo escanear la carpeta: "),
            "{scan}"
        );
        assert!(scan.contains("disco lleno"));
        let track = tr!(player_track_failed(error = "x".to_string())).resolve_now();
        assert!(
            track.starts_with("No se pudo reproducir la canción: "),
            "{track}"
        );
        let dialog = tr!(player_folder_dialog_failed(error = "x".to_string())).resolve_now();
        assert!(
            dialog.starts_with("Falló el diálogo de carpetas: "),
            "{dialog}"
        );
    });
}

#[test]
fn every_message_of_the_player_exists_in_both_locales() {
    let en = include_str!("../../../../locales/en-US/player.ftl");
    let es = include_str!("../../../../locales/es-ES/player.ftl");
    let keys = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| !line.starts_with('#') && line.contains(" = "))
            .map(|line| line.split(" = ").next().unwrap_or_default().to_string())
            .collect()
    };
    assert_eq!(keys(en), keys(es));
}
