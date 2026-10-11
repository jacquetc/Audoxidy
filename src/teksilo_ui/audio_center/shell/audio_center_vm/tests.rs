use super::*;
use crate::frontend::commands::dsp::{self, DownmixChannel, DspEffect, DspParam};
use crate::frontend::commands::eq::{self, EqState};
use crate::frontend::commands::mix::{self, MixToggle, MixValue};
use crate::frontend::settings;
use crate::teksilo_ui::audio_center::shell::testing::services_with_fakes;

fn vm() -> (AudioCenterViewModel, AppServices) {
    let (services, _fakes) = services_with_fakes();
    (AudioCenterViewModel::new(&services), services)
}

#[test]
fn it_starts_closed_on_the_first_tab_and_centred() {
    let (vm, services) = vm();
    assert!(!vm.open().get());
    assert_eq!(vm.tab().get(), audio_center_tab::CONFIG);
    assert_eq!(vm.position().get(), None);
    assert!(services.ctx.shutdown());
}

#[test]
fn it_shares_its_state_with_the_link_the_player_menu_toggles() {
    let (vm, services) = vm();
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::EFFECTS));
    assert!(vm.open().get());
    assert_eq!(vm.tab().get(), audio_center_tab::EFFECTS);
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::EFFECTS));
    assert!(!vm.open().get(), "the open tab closes it, as in Iced");
    assert!(services.ctx.shutdown());
}

#[test]
fn selecting_a_tab_keeps_the_panel_where_it_is() {
    let (vm, services) = vm();
    services.links.audio_center.toggle(Some(0));
    vm.position().set(Some((40.0, 30.0)));
    vm.select_tab(audio_center_tab::VOLUME_MIX);
    assert_eq!(vm.tab().get(), audio_center_tab::VOLUME_MIX);
    assert!(vm.open().get());
    assert_eq!(vm.position().get(), Some((40.0, 30.0)));
    vm.select_tab(7);
    assert_eq!(
        vm.tab().get(),
        audio_center_tab::VOLUME_MIX,
        "a tab that does not exist is ignored"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_tab_is_shown_only_while_the_panel_is_open_on_it() {
    let (vm, services) = vm();
    let config = vm.shows(audio_center_tab::CONFIG);
    let effects = vm.shows(audio_center_tab::EFFECTS);
    assert!(!config.get());
    services.links.audio_center.toggle(Some(0));
    assert!(config.get());
    assert!(!effects.get());
    vm.select_tab(audio_center_tab::EFFECTS);
    assert!(!config.get());
    assert!(effects.get());
    vm.close();
    assert!(!effects.get());
    assert!(services.ctx.shutdown());
}

#[test]
fn closing_hides_the_panel_and_forgets_its_position() {
    let (vm, services) = vm();
    services.links.audio_center.toggle(Some(0));
    vm.begin_drag((10.0, 10.0));
    vm.drag_to((60.0, 50.0));
    assert!(vm.position().get().is_some());
    vm.close();
    assert!(!vm.open().get());
    assert_eq!(vm.position().get(), None);
    assert!(!vm.is_dragging());
    assert!(services.ctx.shutdown());
}

#[test]
fn the_panel_is_centred_in_the_window_until_it_is_dragged() {
    let (vm, services) = vm();
    assert_eq!(vm.viewport(), INITIAL_VIEWPORT, "Iced's initial window");
    assert_eq!(vm.origin(), (210.0, 200.0));
    vm.set_viewport(1000.0, 600.0);
    assert_eq!(vm.origin(), (30.0, 60.0));
    vm.set_viewport(800.0, 400.0);
    assert_eq!(
        vm.origin(),
        (0.0, 0.0),
        "a window smaller than the panel shows its top-leading corner"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_drag_moves_the_panel_by_the_pointer_from_its_position() {
    let (vm, services) = vm();
    vm.set_viewport(1360.0, 880.0);
    vm.position().set(Some((100.0, 100.0)));
    vm.begin_drag((500.0, 300.0));
    vm.drag_to((520.0, 290.0));
    assert_eq!(vm.position().get(), Some((120.0, 90.0)));
    vm.drag_to((525.0, 300.0));
    assert_eq!(
        vm.position().get(),
        Some((125.0, 100.0)),
        "each move counts from the previous one"
    );
    vm.end_drag();
    vm.drag_to((900.0, 900.0));
    assert_eq!(
        vm.position().get(),
        Some((125.0, 100.0)),
        "moves after the release do nothing"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn the_first_drag_of_a_centred_panel_starts_from_iceds_474_high_centre() {
    // Iced (`app.rs:4716`) centres the panel on a height of 474 when the drag
    // starts, where the panel is 480 high: a 3 px jump, kept as Iced has it.
    let (vm, services) = vm();
    vm.set_viewport(1360.0, 880.0);
    vm.begin_drag((0.0, 0.0));
    vm.drag_to((10.0, 10.0));
    assert_eq!(
        vm.position().get(),
        Some((220.0, 213.0)),
        "(880 - 474) / 2 = 203, plus the 10 px of the drag"
    );
    assert!(services.ctx.shutdown());
}

#[test]
fn a_drag_is_clamped_to_the_window_minus_100_by_40() {
    let (vm, services) = vm();
    vm.set_viewport(1360.0, 880.0);
    vm.position().set(Some((100.0, 100.0)));
    vm.begin_drag((0.0, 0.0));
    vm.drag_to((5000.0, 5000.0));
    assert_eq!(
        vm.position().get(),
        Some((1260.0, 840.0)),
        "100 px wide and 40 px high stay on screen"
    );
    vm.drag_to((-9000.0, -9000.0));
    assert_eq!(vm.position().get(), Some((0.0, 0.0)));
    assert!(services.ctx.shutdown());
}

#[test]
fn the_clamp_does_not_remember_how_far_the_pointer_went_past_the_edge() {
    // Iced moves the panel by each step of the pointer from where it is, so
    // coming back from beyond the edge moves it at once.
    let (vm, services) = vm();
    vm.set_viewport(1360.0, 880.0);
    vm.position().set(Some((1200.0, 100.0)));
    vm.begin_drag((0.0, 0.0));
    vm.drag_to((500.0, 0.0));
    assert_eq!(vm.position().get(), Some((1260.0, 100.0)));
    vm.drag_to((400.0, 0.0));
    assert_eq!(vm.position().get(), Some((1160.0, 100.0)));
    assert!(services.ctx.shutdown());
}

#[test]
fn a_window_narrower_than_the_margin_pins_the_panel_to_zero() {
    let (vm, services) = vm();
    vm.set_viewport(80.0, 30.0);
    vm.position().set(Some((10.0, 10.0)));
    vm.begin_drag((0.0, 0.0));
    vm.drag_to((50.0, 50.0));
    assert_eq!(vm.position().get(), Some((0.0, 0.0)));
    assert!(services.ctx.shutdown());
}

/// Quirk fix 4 / Iced `Close`: closing saves the equalizer and the effects in
/// Iced. Here every change is stored as it is made, so the settings in the
/// database already equal the engine's before any close.
#[test]
fn the_equalizer_and_the_effects_are_stored_without_a_close() {
    let (vm, services) = vm();
    let ctx = &services.ctx;

    let mut state = EqState::default();
    eq::set_enabled(ctx, &mut state, true)
        .recv_blocking()
        .unwrap();
    eq::set_preamp(ctx, &mut state, 2.5)
        .recv_blocking()
        .unwrap();
    eq::set_band_gain(ctx, &mut state, 3, -4.0)
        .recv_blocking()
        .unwrap();
    dsp::set_enabled(ctx, DspEffect::Reverb, true)
        .recv_blocking()
        .unwrap();
    dsp::set_value(ctx, DspParam::SubBassGain, 3.0)
        .recv_blocking()
        .unwrap();
    dsp::set_downmix_mix(ctx, DownmixChannel::Center, 1.5)
        .recv_blocking()
        .unwrap();
    mix::set_toggle(ctx, MixToggle::Fades, true)
        .recv_blocking()
        .unwrap();
    mix::set_value(ctx, MixValue::FadeInMs, 1234.0)
        .recv_blocking()
        .unwrap();

    let stored = settings::load_all(&ctx.db).recv_blocking().unwrap();
    assert!(stored.eq.enabled);
    assert_eq!(stored.eq.preamp_gain, 2.5);
    assert_eq!(stored.eq.band_gains_20[3], -4.0);
    assert!(stored.dsp.reverb_enabled);
    assert_eq!(stored.dsp.sub_bass_gain, 3.0);
    assert_eq!(stored.downmix.center, 1.5);
    assert!(stored.mix.fades_enabled);
    assert_eq!(stored.mix.fade_in_ms, 1234.0);
    assert_eq!(stored.eq, state.to_settings());

    // Closing writes nothing more: the stored settings are the same after.
    services.links.audio_center.toggle(Some(0));
    vm.close();
    let after = settings::load_all(&ctx.db).recv_blocking().unwrap();
    assert_eq!(after.eq, stored.eq);
    assert_eq!(after.dsp, stored.dsp);
    assert_eq!(after.downmix, stored.downmix);
    assert_eq!(after.mix, stored.mix);
    assert!(ctx.shutdown());
}
