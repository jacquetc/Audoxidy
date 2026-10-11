//! Headless tests of [`EffectsViewModel`]: every control reaches the engine
//! (the fake player's calls and state) and the database, resets write the
//! Iced defaults, and the signals follow the engine.

use teksilo::core::WidgetTree;

use super::*;
use crate::frontend::AppContext;
use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::{DspSettings, keys};
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{press, tree_with_async};

struct Fixture {
    services: AppServices,
    fakes: InMemoryFakes,
    tree: WidgetTree,
    vm: EffectsViewModel,
}

impl Fixture {
    fn new() -> Self {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        let (tree, runtime) = tree_with_async(&ctx);
        let services = AppServices::new(&AppHandles::new(ctx), runtime);
        let vm = EffectsViewModel::new(&services);
        Self {
            services,
            fakes,
            tree,
            vm,
        }
    }

    /// Runs `f` as a handler would, with a real `EventContext`.
    fn act(&mut self, f: impl Fn(&EffectsViewModel, &mut EventContext) + 'static) {
        let vm = self.vm.clone();
        press(&mut self.tree, move |ctx| f(&vm, ctx));
    }

    /// What the database holds under `key`, after every write queued before.
    fn stored(&self, key: &'static str) -> Option<String> {
        self.services
            .ctx
            .db
            .read(move |db| Ok(db.get_setting(key)))
            .recv_blocking()
            .unwrap()
    }

    fn engine(&self) -> DspSettings {
        dsp::current(&self.services.ctx)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.services.ctx.shutdown();
    }
}

#[test]
fn the_signals_start_from_what_the_engine_holds() {
    let fixture = Fixture::new();
    let vm = &fixture.vm;
    for effect in EFFECTS {
        assert!(!vm.enabled(effect).get(), "{effect:?}");
    }
    for param in DspParam::ALL {
        assert_eq!(vm.value(param).get(), param.default_value(), "{param:?}");
    }
    assert!(!vm.surround().get());
    for channel in DownmixChannel::ALL {
        assert_eq!(vm.downmix_mix(channel).get(), dsp::downmix_default(channel));
    }
}

#[test]
fn refresh_shows_a_change_made_behind_the_tabs_back() {
    let fixture = Fixture::new();
    let ctx = &fixture.services.ctx;
    dsp::set_value(ctx, DspParam::SubBassGain, 9.0)
        .recv_blocking()
        .unwrap();
    dsp::set_enabled(ctx, DspEffect::Reverb, true)
        .recv_blocking()
        .unwrap();
    dsp::set_expander_mode(ctx, ExpanderMode::Surround)
        .recv_blocking()
        .unwrap();
    dsp::set_downmix_mix(ctx, DownmixChannel::Lfe, 1.5)
        .recv_blocking()
        .unwrap();
    dsp::set_downmix_enabled(ctx, DownmixChannel::Center, true)
        .recv_blocking()
        .unwrap();
    fixture.vm.refresh();
    assert_eq!(fixture.vm.value(DspParam::SubBassGain).get(), 9.0);
    assert!(fixture.vm.enabled(DspEffect::Reverb).get());
    assert!(fixture.vm.surround().get());
    assert_eq!(fixture.vm.downmix_mix(DownmixChannel::Lfe).get(), 1.5);
    assert!(fixture.vm.downmix_enabled(DownmixChannel::Center).get());
}

#[test]
fn each_toggle_reaches_the_engine_and_the_database() {
    for effect in EFFECTS {
        let mut f = Fixture::new();
        f.act(move |vm, ctx| vm.set_enabled(effect, true, ctx));
        assert!(effect.is_enabled(&f.engine()), "{effect:?}");
        assert!(f.vm.enabled(effect).get(), "{effect:?}");
        assert_eq!(f.fakes.player.calls(), vec![PlayerCall::UpdateDsp]);
        assert_eq!(f.stored(effect.key().key()).as_deref(), Some("1"));
        f.act(move |vm, ctx| vm.set_enabled(effect, false, ctx));
        assert!(!effect.is_enabled(&f.engine()), "{effect:?}");
        assert_eq!(f.stored(effect.key().key()).as_deref(), Some("0"));
    }
}

#[test]
fn each_parameter_reaches_the_engine_and_the_database() {
    let cases: [(DspParam, f32, &str); 11] = [
        (DspParam::SubBassGain, 6.5, "6.5"),
        (DspParam::MidBassGain, -2.0, "-2.0"),
        (DspParam::VoiceBoostGain, 3.5, "3.5"),
        (DspParam::NoiseGateThreshold, -40.0, "-40.0"),
        (DspParam::StereoExpanderWidth, 150.0, "1.5"),
        (DspParam::StereoBalance, -0.5, "-0.5"),
        (DspParam::CompressorThreshold, -12.0, "-12.0"),
        (DspParam::CompressorIntensity, 80.0, "0.8"),
        (DspParam::LimiterCeiling, -3.0, "-3.0"),
        (DspParam::ReverbWet, 0.35, "0.35"),
        (DspParam::ReverbRoomSize, 0.8, "0.80"),
    ];
    let mut f = Fixture::new();
    for (param, value, stored) in cases {
        f.act(move |vm, ctx| vm.set_value(param, value, ctx));
        assert_eq!(f.vm.value(param).get(), value, "{param:?}");
        assert!((param.value(&f.engine()) - value).abs() < 1e-4, "{param:?}");
        assert_eq!(f.stored(param.key().key()).as_deref(), Some(stored));
    }
    assert_eq!(f.fakes.player.calls(), vec![PlayerCall::UpdateDsp; 11]);
    f.fakes.player.read_dsp(&mut |dsp| {
        assert_eq!(dsp.stereo_expander.width, 1.5, "the width is a percentage");
        assert_eq!(dsp.compressor.intensity, 0.8, "so is the intensity");
    });
}

#[test]
fn a_value_beyond_the_range_is_clamped_by_the_facade() {
    let mut f = Fixture::new();
    f.act(|vm, ctx| vm.set_value(DspParam::SubBassGain, 99.0, ctx));
    assert_eq!(f.engine().sub_bass_gain, 24.0);
}

#[test]
fn a_secondary_click_value_resets_each_parameter_to_its_iced_default() {
    let mut f = Fixture::new();
    for param in DspParam::ALL {
        let top = *param.range().end();
        f.act(move |vm, ctx| vm.set_value(param, top, ctx));
        assert_eq!(f.vm.value(param).get(), top);
        f.act(move |vm, ctx| vm.set_value(param, param.default_value(), ctx));
        assert_eq!(f.vm.value(param).get(), param.default_value(), "{param:?}");
    }
    assert_eq!(f.engine(), DspSettings::default());
    assert_eq!(
        f.stored(keys::DSP_STEREO_EXPANDER_WIDTH).as_deref(),
        Some("1.0")
    );
    assert_eq!(
        f.stored(keys::DSP_COMPRESSOR_INTENSITY).as_deref(),
        Some("0.5")
    );
    assert_eq!(
        f.stored(keys::DSP_NOISE_GATE_THRESHOLD).as_deref(),
        Some("-60.0")
    );
    assert_eq!(f.stored(keys::DSP_REVERB_WET).as_deref(), Some("0.50"));
}

#[test]
fn the_defaults_are_the_iced_ones() {
    // audio_center.rs:3322-3640: the reset message of each card.
    let defaults = [
        (DspParam::SubBassGain, 0.0),
        (DspParam::MidBassGain, 0.0),
        (DspParam::VoiceBoostGain, 0.0),
        (DspParam::NoiseGateThreshold, -60.0),
        (DspParam::StereoExpanderWidth, 100.0),
        (DspParam::StereoBalance, 0.0),
        (DspParam::CompressorThreshold, -3.0),
        (DspParam::CompressorIntensity, 50.0),
        (DspParam::LimiterCeiling, -1.0),
        (DspParam::ReverbWet, 0.5),
        (DspParam::ReverbRoomSize, 0.5),
    ];
    for (param, default) in defaults {
        assert_eq!(param.default_value(), default, "{param:?}");
    }
    assert_eq!(dsp::downmix_default(DownmixChannel::Lfe), 0.66);
    assert_eq!(dsp::downmix_default(DownmixChannel::Center), 0.76);
    assert_eq!(dsp::downmix_default(DownmixChannel::Surround), 0.74);
}

#[test]
fn the_expander_mode_is_hybrid_for_natural_and_surround_for_surround() {
    let mut f = Fixture::new();
    f.act(|vm, ctx| vm.set_surround(true, ctx));
    assert_eq!(f.engine().stereo_expander_mode, ExpanderMode::Surround);
    assert_eq!(
        f.stored(keys::DSP_STEREO_EXPANDER_MODE).as_deref(),
        Some("surround")
    );
    assert!(f.vm.surround().get());
    f.act(|vm, ctx| vm.set_surround(false, ctx));
    assert_eq!(f.engine().stereo_expander_mode, ExpanderMode::Hybrid);
    assert_eq!(
        f.stored(keys::DSP_STEREO_EXPANDER_MODE).as_deref(),
        Some("hybrid")
    );
}

#[test]
fn the_downmix_toggles_and_mixes_reach_the_engine_state_and_the_database() {
    let mut f = Fixture::new();
    f.act(|vm, ctx| {
        vm.set_downmix_enabled(DownmixChannel::Lfe, true, ctx);
        vm.set_downmix_enabled(DownmixChannel::Center, true, ctx);
        vm.set_downmix_enabled(DownmixChannel::Surround, true, ctx);
        vm.set_downmix_mix(DownmixChannel::Lfe, 1.25, ctx);
        vm.set_downmix_mix(DownmixChannel::Center, 0.5, ctx);
        vm.set_downmix_mix(DownmixChannel::Surround, 2.0, ctx);
    });
    let state = f.fakes.player.snapshot();
    assert!(state.downmix_lfe_enabled && state.downmix_center_enabled);
    assert!(state.downmix_surround_enabled);
    assert_eq!(state.downmix_lfe, 1.25);
    assert_eq!(state.downmix_center, 0.5);
    assert_eq!(state.downmix_surround, 2.0);
    assert_eq!(f.stored(keys::AUDIO_DOWNMIX_LFE).as_deref(), Some("1.25"));
    assert_eq!(
        f.stored(keys::AUDIO_DOWNMIX_CENTER).as_deref(),
        Some("0.50")
    );
    assert_eq!(
        f.stored(keys::AUDIO_DOWNMIX_SURROUND).as_deref(),
        Some("2.00")
    );
    assert_eq!(
        f.stored(keys::AUDIO_DOWNMIX_SURROUND_ENABLED).as_deref(),
        Some("1")
    );
    f.act(|vm, ctx| vm.set_downmix_enabled(DownmixChannel::Lfe, false, ctx));
    assert!(!f.fakes.player.snapshot().downmix_lfe_enabled);
    assert_eq!(
        f.stored(keys::AUDIO_DOWNMIX_LFE_ENABLED).as_deref(),
        Some("0")
    );
}

#[test]
fn a_downmix_mix_at_its_default_is_the_facades_reset() {
    let mut f = Fixture::new();
    for (channel, default, stored) in [
        (DownmixChannel::Lfe, 0.66, "0.66"),
        (DownmixChannel::Center, 0.76, "0.76"),
        (DownmixChannel::Surround, 0.74, "0.74"),
    ] {
        f.act(move |vm, ctx| vm.set_downmix_mix(channel, 2.0, ctx));
        f.act(move |vm, ctx| vm.set_downmix_mix(channel, default, ctx));
        assert_eq!(f.vm.downmix_mix(channel).get(), default);
        assert_eq!(dsp::current_downmix(&f.services.ctx).mix(channel), default);
        assert_eq!(f.stored(channel.mix_key()).as_deref(), Some(stored));
    }
}

#[test]
fn a_new_view_model_shows_the_restored_settings() {
    let f = Fixture::new();
    dsp::set_value(&f.services.ctx, DspParam::CompressorIntensity, 20.0)
        .recv_blocking()
        .unwrap();
    dsp::set_value(&f.services.ctx, DspParam::ReverbRoomSize, 0.9)
        .recv_blocking()
        .unwrap();
    let vm = EffectsViewModel::new(&f.services);
    assert!((vm.value(DspParam::CompressorIntensity).get() - 20.0).abs() < 1e-3);
    assert!((vm.value(DspParam::ReverbRoomSize).get() - 0.9).abs() < 1e-3);
}
