//! Headless tests of [`EffectsTab`]: the grid geometry at Iced's coordinates,
//! each card's controls and ranges, the switches and sliders reaching the
//! engine, the accessible names and the Spanish strings.

use teksilo::canvas::Point;
use teksilo::core::accesskit::Role;
use teksilo::core::{Key, PointerButton, WidgetTree};
use teksilo::tokens::{HAlignment, VAlignment};
use teksilo::widgets::{FixedSize, HStack, Spacer, VStack};

use super::*;
use crate::audio::dsp::ExpanderMode;
use crate::frontend::AppContext;
use crate::frontend::commands::dsp;
use crate::frontend::fakes::InMemoryFakes;
use crate::frontend::ports::PlayerPort;
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{
    a11y_node, access_click, find_by_name, key_down, layout, pointer_click, tree_with_async,
    with_shipped_messages,
};

/// The width the shell gives a tab: 940 less the 2 px inset and the 15 px
/// side padding of the Iced window (`audio_center.rs:1971`, `1999`).
const TAB_WIDTH: f32 = 906.0;
/// The width of a column: a third of the width less the two 15 px gaps.
const COLUMN: f32 = (TAB_WIDTH - 2.0 * GAP) / 3.0;

struct Fixture {
    tree: WidgetTree,
    services: AppServices,
    fakes: InMemoryFakes,
    tab: WidgetId,
}

impl Fixture {
    fn new() -> Self {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        Self::over(ctx, fakes)
    }

    fn over(ctx: AppContext, fakes: InMemoryFakes) -> Self {
        let (mut tree, runtime) = tree_with_async(&ctx);
        let services = AppServices::new(&AppHandles::new(ctx), runtime);
        let tab = tree.add(EffectsTab::new(EffectsViewModel::new(&services)));
        let fixed = tree.add(FixedSize::new().width(TAB_WIDTH).child(tab));
        tree.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(
                    HStack::new()
                        .spacing(0.0)
                        .alignment(VAlignment::Top)
                        .child(fixed)
                        .child(Spacer::new()),
                )
                .child(Spacer::new()),
        );
        layout(&mut tree);
        let _ = tree.render();
        Self {
            tree,
            services,
            fakes,
            tab,
        }
    }

    /// The twelve cards, left column first, top to bottom.
    fn cards(&self) -> Vec<WidgetId> {
        let mut found = Vec::new();
        collect(&self.tree, self.tab, "::Frame", &mut found);
        found
    }

    fn sliders(&self) -> Vec<WidgetId> {
        let mut found = Vec::new();
        collect_role(&self.tree, self.tab, Role::Slider, &mut found);
        found
    }

    fn switches(&self) -> Vec<WidgetId> {
        let mut found = Vec::new();
        collect_role(&self.tree, self.tab, Role::Switch, &mut found);
        found
    }

    fn engine(&self) -> crate::frontend::settings::DspSettings {
        dsp::current(&self.services.ctx)
    }

    /// The switch of the card titled `title`.
    fn switch(&self, title: &str) -> WidgetId {
        let found: Vec<WidgetId> = self
            .switches()
            .into_iter()
            .filter(|&s| self.tree.accessibility_node(s).name() == Some(title))
            .collect();
        assert_eq!(found.len(), 1, "{title:?} titles {} switches", found.len());
        found[0]
    }

    fn named(&self, name: &str) -> WidgetId {
        let found = find_by_name(&self.tree, self.tab, name);
        assert_eq!(found.len(), 1, "{name:?} names {} widgets", found.len());
        found[0]
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.services.ctx.shutdown();
    }
}

fn collect(tree: &WidgetTree, id: WidgetId, suffix: &str, found: &mut Vec<WidgetId>) {
    if tree
        .widget_type_name(id)
        .is_some_and(|name| name.ends_with(suffix))
    {
        found.push(id);
    }
    for child in tree.children(id) {
        collect(tree, child, suffix, found);
    }
}

fn collect_role(tree: &WidgetTree, id: WidgetId, role: Role, found: &mut Vec<WidgetId>) {
    if tree.accessibility_node(id).role() == role {
        found.push(id);
    }
    for child in tree.children(id) {
        collect_role(tree, child, role, found);
    }
}

#[test]
fn the_cards_make_three_columns_of_equal_width_fifteen_pixels_apart() {
    let f = Fixture::new();
    let cards = f.cards();
    assert_eq!(cards.len(), 12);
    let xs: Vec<f32> = cards.iter().map(|&c| f.tree.bounds(c).x).collect();
    let column_x = [0.0, COLUMN + GAP, 2.0 * (COLUMN + GAP)];
    for (index, x) in xs.iter().enumerate() {
        let expected = column_x[[0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2][index]];
        assert!((x - expected).abs() < 0.01, "card {index} at {x}");
        assert!((f.tree.bounds(cards[index]).width - COLUMN).abs() < 0.01);
    }
}

#[test]
fn the_cards_stack_at_icheds_heights_and_gaps() {
    let f = Fixture::new();
    let cards = f.cards();
    let tops_and_heights: Vec<(f32, f32)> = cards
        .iter()
        .map(|&c| {
            let b = f.tree.bounds(c);
            (b.y, b.height)
        })
        .collect();
    // 69 = 12 + 17 + 10 + 18 + 12; the dividers are 11 high and the heading 12.
    assert_eq!(CARD_HEIGHT, 69.0);
    let plain = CARD_HEIGHT;
    let tall = TALL_CARD_HEIGHT;
    let first = TOP_INSET;
    let second = first + plain + GAP;
    let third = second + plain + GAP;
    let fourth_left = third + tall + GAP + DIVIDER_HEIGHT + GAP;
    let fourth_middle = third + tall + GAP + HEADING_HEIGHT + GAP;
    let expected = [
        (first, plain),
        (second, plain),
        (third, tall),
        (fourth_left, plain),
        (first, plain),
        (second, plain),
        (third, tall),
        (fourth_middle, plain),
        (first, plain),
        (second, plain),
        (third, tall),
        (fourth_left, plain),
    ];
    assert_eq!(tops_and_heights, expected.to_vec());
}

#[test]
fn there_are_thirteen_switches_and_fourteen_sliders() {
    let f = Fixture::new();
    assert_eq!(f.switches().len(), 13, "twelve cards and the mode switch");
    assert_eq!(f.sliders().len(), 14);
}

#[test]
fn every_slider_has_icheds_range_step_and_value() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new();
        let cases = [
            ("Sub-bass boost: level dB", -4.0, 24.0, 0.1, 0.0),
            ("Noise reduction: threshold dB", -85.0, -10.0, 0.1, -60.0),
            ("Compressor: threshold dB", -24.0, 0.0, 0.1, -3.0),
            ("Compressor: intensity %", 0.0, 100.0, 1.0, 50.0),
            ("Subwoofer channel: mix", 0.0, 2.0, 0.01, 0.66),
            ("Bass boost: level dB", -4.0, 15.0, 0.1, 0.0),
            ("Stereo expander: width %", 0.0, 260.0, 0.1, 100.0),
            ("Limiter: ceiling dB", -12.0, 0.0, 0.1, -1.0),
            ("Center channel: mix", 0.0, 2.0, 0.01, 0.76),
            ("Voice boost: level dB", -4.0, 13.0, 0.1, 0.0),
            ("Stereo balance: left | right", -1.0, 1.0, 0.1, 0.0),
            ("Reverb: mix", 0.0, 1.0, 0.01, 0.5),
            ("Reverb: size", 0.0, 1.0, 0.01, 0.5),
            ("Surround channels: mix", 0.0, 2.0, 0.01, 0.74),
        ];
        for (name, min, max, step, value) in cases {
            let slider = f.named(name);
            let node = a11y_node(&mut f.tree, slider);
            assert_eq!(node.min_numeric_value(), Some(min), "{name}");
            assert_eq!(node.max_numeric_value(), Some(max), "{name}");
            assert!(
                (node.numeric_value_step().unwrap_or(0.0) - step).abs() < 1e-9,
                "{name}"
            );
            assert!(
                (node.numeric_value().unwrap_or(f64::NAN) - value).abs() < 1e-5,
                "{name}"
            );
        }
    });
}

#[test]
fn the_labels_and_names_are_icheds_spanish() {
    with_shipped_messages("es-ES", || {
        let f = Fixture::new();
        for title in [
            "Refuerzo de Sub-Graves",
            "Reducción de Ruido",
            "Compresor",
            "Canal de Subwoofer",
            "Refuerzo de Graves",
            "Expansor Estéreo",
            "Limitador",
            "Canal Central",
            "Refuerzo de Voces",
            "Balance Estéreo",
            "Reverberación",
            "Canales Surround  SL/SR | SBL/SBR",
        ] {
            assert_eq!(
                f.switches()
                    .iter()
                    .filter(|&&s| f.tree.accessibility_node(s).name() == Some(title))
                    .count(),
                1,
                "the switch of {title:?}"
            );
        }
        for name in [
            "Refuerzo de Sub-Graves: Nivel dB",
            "Compresor: Intensidad %",
            "Reverberación: Tamaño",
            "Balance Estéreo: Left | Right",
            "Canales Surround: Mix",
        ] {
            f.named(name);
        }
        f.named("Modo del expansor estéreo");
        for text in [
            "Nivel dB",
            "Umbral dB",
            "Ancho %",
            "Left | Right",
            "Intensidad %",
            "Techo dB",
            "Tamaño",
            "Natural",
            "Surround",
            "Volumen de canales en mezcla menor a 5.1",
        ] {
            assert!(
                !find_by_name(&f.tree, f.tab, text).is_empty(),
                "{text:?} is shown"
            );
        }
    });
}

#[test]
fn the_english_strings_translate_every_label() {
    with_shipped_messages("en-US", || {
        let f = Fixture::new();
        for text in [
            "Level dB",
            "Threshold dB",
            "Width %",
            "Intensity %",
            "Ceiling dB",
            "Size",
            "Channel volume when mixing down below 5.1",
        ] {
            assert!(
                !find_by_name(&f.tree, f.tab, text).is_empty(),
                "{text:?} is shown"
            );
        }
    });
}

#[test]
fn a_switch_turns_its_effect_on_in_the_engine_and_in_the_database() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new();
        let switch = f.switch("Limiter");
        access_click(&mut f.tree, switch);
        assert!(f.engine().limiter_enabled);
        assert_eq!(
            f.fakes.player.calls(),
            vec![crate::frontend::fakes::PlayerCall::UpdateDsp]
        );
        let stored = f
            .services
            .ctx
            .db
            .read(|db| Ok(db.get_setting("dsp_limiter_enabled")))
            .recv_blocking()
            .unwrap();
        assert_eq!(stored.as_deref(), Some("1"));
        access_click(&mut f.tree, switch);
        assert!(!f.engine().limiter_enabled);
    });
}

#[test]
fn the_natural_surround_switch_sets_the_expander_mode() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new();
        let switch = f.named("Stereo expander mode");
        access_click(&mut f.tree, switch);
        assert_eq!(f.engine().stereo_expander_mode, ExpanderMode::Surround);
        access_click(&mut f.tree, switch);
        assert_eq!(f.engine().stereo_expander_mode, ExpanderMode::Hybrid);
    });
}

#[test]
fn the_downmix_switches_and_sliders_write_the_engine_state() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new();
        let switch = f.switch("Surround channels  SL/SR | SBL/SBR");
        access_click(&mut f.tree, switch);
        assert!(f.fakes.player.snapshot().downmix_surround_enabled);
        let slider = f.named("Surround channels: mix");
        // The track is 40 + 10 px short of the card's content width.
        let bounds = f.tree.bounds(slider);
        pointer_click(
            &mut f.tree,
            Point::new(bounds.x + bounds.width / 2.0, bounds.y + 9.0),
            PointerButton::Primary,
            10,
        );
        assert!((f.fakes.player.snapshot().downmix_surround - 1.0).abs() < 0.02);
        pointer_click(
            &mut f.tree,
            Point::new(bounds.x + 5.0, bounds.y + 9.0),
            PointerButton::Secondary,
            500,
        );
        assert_eq!(f.fakes.player.snapshot().downmix_surround, 0.74);
    });
}

#[test]
fn a_slider_moves_its_parameter_and_a_secondary_press_resets_it() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new();
        let slider = f.named("Reverb: size");
        key_down(&mut f.tree, slider, Key::ArrowRight);
        assert!((f.engine().reverb_room_size - 0.51).abs() < 1e-4);
        let bounds = f.tree.bounds(slider);
        pointer_click(
            &mut f.tree,
            Point::new(bounds.x + 4.0, bounds.y + 9.0),
            PointerButton::Secondary,
            10,
        );
        assert!((f.engine().reverb_room_size - 0.5).abs() < 1e-4);
        let intensity = f.named("Compressor: intensity %");
        key_down(&mut f.tree, intensity, Key::ArrowLeft);
        assert!((f.engine().compressor_intensity - 0.49).abs() < 1e-4);
    });
}

#[test]
fn the_tab_shows_the_settings_the_engine_holds_when_it_is_built() {
    let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
    dsp::set_value(&ctx, DspParam::CompressorIntensity, 20.0)
        .recv_blocking()
        .unwrap();
    dsp::set_enabled(&ctx, DspEffect::Compressor, true)
        .recv_blocking()
        .unwrap();
    with_shipped_messages("en-US", || {
        let mut f = Fixture::over(ctx.clone(), fakes);
        let slider = f.named("Compressor: intensity %");
        let node = a11y_node(&mut f.tree, slider);
        assert!((node.numeric_value().unwrap_or(0.0) - 20.0).abs() < 1e-3);
        let switch = f.switch("Compressor");
        assert!(f.tree.accessibility_node(switch).is_toggled());
    });
}

#[test]
fn the_card_border_is_the_accent_only_while_the_effect_is_on() {
    with_shipped_messages("en-US", || {
        let mut f = Fixture::new();
        let accent = crate::teksilo_ui::style::ACCENT.to_array();
        let card_border = |frame: &teksilo::canvas::RenderFrame| {
            frame
                .shapes
                .iter()
                .any(|s| s.stroke_width == CARD_BORDER && s.color == accent)
        };
        assert!(!card_border(&f.tree.render()));
        let switch = f.switch("Reverb");
        access_click(&mut f.tree, switch);
        assert!(card_border(&f.tree.render()));
    });
}

#[test]
fn inside_a_card_the_rows_sit_at_icheds_offsets() {
    with_shipped_messages("en-US", || {
        let f = Fixture::new();
        let card = f.tree.bounds(f.cards()[0]);
        // Padding 12 | title row 17 | gap 10 | slider row 18 | padding 12.
        let slider = f.tree.bounds(f.named("Sub-bass boost: level dB"));
        assert_eq!(slider.y, card.y + 12.0 + 17.0 + 10.0);
        assert_eq!(slider.height, 18.0);
        // The slider ends 10 px (the padding) from the card's right edge,
        // less the 10 px gap and the 40 px field it owns.
        assert!(
            (slider.x + slider.width + 10.0 + 40.0 - (card.x + card.width - 10.0)).abs() < 0.01
        );
        // The switch is 34 x 17, at the end of the title row.
        let switch = f.tree.bounds(f.switch("Sub-bass boost"));
        assert_eq!((switch.width, switch.height), (34.0, 17.0));
        assert_eq!(switch.x + switch.width, card.x + card.width - 10.0);
        assert_eq!(switch.y, card.y + 12.0);
    });
}

#[test]
fn a_second_slider_sits_six_pixels_under_the_first_and_the_limiters_gap_is_wider() {
    with_shipped_messages("en-US", || {
        let f = Fixture::new();
        let first = f.tree.bounds(f.named("Compressor: threshold dB"));
        let second = f.tree.bounds(f.named("Compressor: intensity %"));
        assert_eq!(second.y, first.y + 18.0 + 6.0);
        let limiter_card = f.tree.bounds(f.cards()[6]);
        let limiter = f.tree.bounds(f.named("Limiter: ceiling dB"));
        assert_eq!(limiter.y, limiter_card.y + 12.0 + 17.0 + 10.0 + 12.0);
        let reverb = f.tree.bounds(f.named("Reverb: mix"));
        let size = f.tree.bounds(f.named("Reverb: size"));
        assert_eq!(size.y, reverb.y + 18.0 + 6.0);
    });
}

#[test]
fn the_mode_switch_sits_six_pixels_after_the_expanders_title() {
    with_shipped_messages("en-US", || {
        let f = Fixture::new();
        let card = f.tree.bounds(f.cards()[5]);
        let mode = f.tree.bounds(f.named("Stereo expander mode"));
        assert_eq!((mode.width, mode.height), (26.0, 13.0));
        assert!(
            mode.x > card.x + 10.0 + 6.0,
            "after the title and 'Natural'"
        );
        assert!(mode.x + mode.width < card.x + card.width - 10.0 - 34.0);
    });
}

#[test]
fn the_dividers_are_inset_fifteen_pixels_and_the_heading_is_centred() {
    with_shipped_messages("en-US", || {
        let f = Fixture::new();
        let cards = f.cards();
        let left = f.tree.bounds(cards[2]);
        let lfe = f.tree.bounds(cards[3]);
        assert_eq!(lfe.y - (left.y + left.height), GAP + DIVIDER_HEIGHT + GAP);
        let heading = f.named("Channel volume when mixing down below 5.1");
        let heading = f.tree.bounds(heading);
        let middle = f.tree.bounds(cards[6]);
        // The 15.6 px line overflows the 12 px box by 2 px above and below, as
        // the negative padding of Iced makes it.
        assert_eq!(heading.y, middle.y + middle.height + GAP - 2.0);
        assert!(((heading.x + heading.width / 2.0) - (middle.x + middle.width / 2.0)).abs() < 1.0);
    });
}
