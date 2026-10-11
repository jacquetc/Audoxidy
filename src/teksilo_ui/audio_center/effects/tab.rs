//! `EffectsTab`: the body of the Audio Center's "Efectos de Audio" tab.
//!
//! Port of `view_audio_effects` (`src/gui/audio_center.rs:3036-3704`). Under a
//! 18 px top inset (3 px padding and a 15 px space, `3685-3703`) come three
//! columns of equal width, 15 px apart (`FillPortion(1)` each), and the cards
//! stack in each column 15 px apart:
//!
//! | Left | Middle | Right |
//! | --- | --- | --- |
//! | Sub-bass | Mid-bass | Voice boost |
//! | Noise gate | Stereo expander (Natural / Surround) | Stereo balance |
//! | Compressor (two sliders, 92 px) | Limiter (92 px) | Reverb (two sliders, 92 px) |
//! | divider | "Volumen de canales en mezcla menor a 5.1" | divider |
//! | Subwoofer channel | Center channel | Surround channels |
//!
//! A card (`view_effect_with_secondary`, `3066-3283`) is a 8 px rounded box
//! filled with `COLOR_CONTRAST`, padded 12 px above and below and 10 px at
//! the sides, with a 1 px border that is the accent while the effect is on and
//! the fill's own colour (so, none) while it is off. Inside: the title row
//! (title 13 px medium, the optional extra widget 6 px after it, the switch
//! at the far end), a 10 px gap, and a row per slider ("label" 11 px, 10 px,
//! the [`EffectSlider`]); a second slider sits 6 px below the first. The
//! compressor, limiter and reverb cards are 92 px high whatever they hold
//! (the limiter adds 12 px to its gap to line its slider up with the others).
//!
//! # Differences from the Iced view
//!
//! - Each switch and slider is a real control with an accessible name
//!   (`ac-effects-*-name`, not in the Iced UI); a switch is announced by the
//!   card's title.
//! - A switch acts on the release of the press; Iced's acts on press
//!   (see `widgets::stock_styles`).

use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, Padding, RectWidget, Spacer, TextWidget, Toggle, VStack,
};

use super::effects_vm::EffectsViewModel;
use crate::frontend::commands::dsp::{DOWNMIX_RANGE, DOWNMIX_STEP, DspEffect, DspParam};
use crate::frontend::settings::DownmixChannel;
use crate::teksilo_ui::style::inter;
use crate::teksilo_ui::widgets::stock_styles::{self, ToggleLook};
use crate::teksilo_ui::widgets::{EffectSlider, Frame};

/// The space between the columns and between the cards of a column
/// (`audio_center.rs:3685-3703`, `.spacing(15)`).
pub const GAP: f32 = 15.0;
/// What sits above the columns: 3 px of padding and a 15 px space (`3693`).
pub const TOP_INSET: f32 = 3.0 + 15.0;
/// A card's corner radius (`audio_center.rs:3118`).
pub const CARD_RADIUS: f32 = 8.0;
/// A card's border (`3117`).
pub const CARD_BORDER: f32 = 1.0;
/// A card's vertical padding (`3109`).
pub const CARD_PADDING_Y: f32 = 12.0;
/// A card's horizontal padding (`3109`).
pub const CARD_PADDING_X: f32 = 10.0;
/// The height of the cards with a second slider, and of the limiter (`3377`).
pub const TALL_CARD_HEIGHT: f32 = 92.0;
/// The height of a plain card: the padding, the 17 px title row, the 10 px
/// gap and the 18 px slider row.
pub const CARD_HEIGHT: f32 = 2.0 * CARD_PADDING_Y + TITLE_ROW + 10.0 + ROW;
/// The title row's height: the 17 px switch (the 13 px title is lower).
const TITLE_ROW: f32 = 17.0;
/// A slider row's height.
const ROW: f32 = 18.0;
/// The gap between the title row and the first slider (`3098`).
const TITLE_GAP: f32 = 10.0;
/// The gap between the sliders of a card (`3101`).
const SLIDER_GAP: f32 = 6.0;
/// The gap after a slider's label (`3135`).
const LABEL_GAP: f32 = 10.0;
/// The gap after a card's title (`3085`).
const TITLE_TO_EXTRA: f32 = 6.0;
/// The divider's height: 6 px above, the 1 px line, 4 px below (`3425`).
pub const DIVIDER_HEIGHT: f32 = 11.0;
/// The divider's inset on each side (`3425`).
const DIVIDER_INSET: f32 = 15.0;
/// The downmix heading's height: the 12 px text's line (15.6 px) less the
/// 2 px negative padding above and below (`3466`).
pub const HEADING_HEIGHT: f32 = 12.0;

/// The effects tab; see the module documentation.
#[derive(Debug)]
pub struct EffectsTab {
    vm: EffectsViewModel,
    root: Option<WidgetId>,
}

impl EffectsTab {
    /// The tab over `vm`. It reads the engine back each time it is built.
    pub fn new(vm: EffectsViewModel) -> Self {
        Self { vm, root: None }
    }
}

impl Widget for EffectsTab {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.refresh();
        let vm = &self.vm;
        let left = column()
            .child(dsp_card(
                vm,
                DspEffect::SubBass,
                || tr!(ac_effects_sub_bass_title()),
                vec![dsp_slider(
                    vm,
                    DspParam::SubBassGain,
                    || tr!(ac_effects_level_db()),
                    || tr!(ac_effects_sub_bass_level_name()),
                )],
            ))
            .child(dsp_card(
                vm,
                DspEffect::NoiseGate,
                || tr!(ac_effects_noise_gate_title()),
                vec![dsp_slider(
                    vm,
                    DspParam::NoiseGateThreshold,
                    || tr!(ac_effects_threshold_db()),
                    || tr!(ac_effects_noise_gate_threshold_name()),
                )],
            ))
            .child(dsp_card(
                vm,
                DspEffect::Compressor,
                || tr!(ac_effects_compressor_title()),
                vec![
                    dsp_slider(
                        vm,
                        DspParam::CompressorThreshold,
                        || tr!(ac_effects_threshold_db()),
                        || tr!(ac_effects_compressor_threshold_name()),
                    ),
                    dsp_slider(
                        vm,
                        DspParam::CompressorIntensity,
                        || tr!(ac_effects_intensity_percent()),
                        || tr!(ac_effects_compressor_intensity_name()),
                    ),
                ],
            ))
            .child(divider())
            .child(downmix_card(
                vm,
                DownmixChannel::Lfe,
                || tr!(ac_effects_downmix_lfe_title()),
                || tr!(ac_effects_downmix_lfe_mix_name()),
            ));

        let middle = column()
            .child(dsp_card(
                vm,
                DspEffect::MidBass,
                || tr!(ac_effects_mid_bass_title()),
                vec![dsp_slider(
                    vm,
                    DspParam::MidBassGain,
                    || tr!(ac_effects_level_db()),
                    || tr!(ac_effects_mid_bass_level_name()),
                )],
            ))
            .child(card(
                vm.enabled(DspEffect::StereoExpander),
                || tr!(ac_effects_stereo_expander_title()),
                Some(mode_switch(vm)),
                {
                    let vm = vm.clone();
                    move |on, ctx| vm.set_enabled(DspEffect::StereoExpander, on, ctx)
                },
                vec![dsp_slider(
                    vm,
                    DspParam::StereoExpanderWidth,
                    || tr!(ac_effects_width_percent()),
                    || tr!(ac_effects_stereo_expander_width_name()),
                )],
                CardSize::Plain,
                0.0,
            ))
            .child(card(
                vm.enabled(DspEffect::Limiter),
                || tr!(ac_effects_limiter_title()),
                None::<Spacer>,
                {
                    let vm = vm.clone();
                    move |on, ctx| vm.set_enabled(DspEffect::Limiter, on, ctx)
                },
                vec![dsp_slider(
                    vm,
                    DspParam::LimiterCeiling,
                    || tr!(ac_effects_ceiling_db()),
                    || tr!(ac_effects_limiter_ceiling_name()),
                )],
                CardSize::Tall,
                12.0,
            ))
            .child(heading())
            .child(downmix_card(
                vm,
                DownmixChannel::Center,
                || tr!(ac_effects_downmix_center_title()),
                || tr!(ac_effects_downmix_center_mix_name()),
            ));

        let right = column()
            .child(dsp_card(
                vm,
                DspEffect::VoiceBoost,
                || tr!(ac_effects_voice_boost_title()),
                vec![dsp_slider(
                    vm,
                    DspParam::VoiceBoostGain,
                    || tr!(ac_effects_level_db()),
                    || tr!(ac_effects_voice_boost_level_name()),
                )],
            ))
            .child(dsp_card(
                vm,
                DspEffect::StereoBalance,
                || tr!(ac_effects_stereo_balance_title()),
                vec![dsp_slider(
                    vm,
                    DspParam::StereoBalance,
                    || tr!(ac_effects_balance_sides()),
                    || tr!(ac_effects_stereo_balance_balance_name()),
                )],
            ))
            .child(dsp_card(
                vm,
                DspEffect::Reverb,
                || tr!(ac_effects_reverb_title()),
                vec![
                    dsp_slider(
                        vm,
                        DspParam::ReverbWet,
                        || tr!(ac_effects_mix()),
                        || tr!(ac_effects_reverb_mix_name()),
                    ),
                    dsp_slider(
                        vm,
                        DspParam::ReverbRoomSize,
                        || tr!(ac_effects_room_size()),
                        || tr!(ac_effects_reverb_size_name()),
                    ),
                ],
            ))
            .child(divider())
            .child(downmix_card(
                vm,
                DownmixChannel::Surround,
                || tr!(ac_effects_downmix_surround_title()),
                || tr!(ac_effects_downmix_surround_mix_name()),
            ));

        let columns = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(Expand::horizontal().child(left))
            .child(gap(GAP))
            .child(Expand::horizontal().child(middle))
            .child(gap(GAP))
            .child(Expand::horizontal().child(right));
        let root = ctx.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(
                    FixedSize::new()
                        .width(0.0)
                        .height(TOP_INSET)
                        .child(Spacer::new()),
                )
                .child(columns),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// A column of cards, `GAP` apart, each as wide as the column.
fn column() -> VStack {
    VStack::new().spacing(GAP).alignment(HAlignment::Leading)
}

/// A fixed horizontal gap.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

/// A fixed vertical gap.
fn drop_down(height: f32) -> FixedSize {
    FixedSize::new()
        .width(0.0)
        .height(height)
        .child(Spacer::new())
}

/// One slider of a card: its label, accessible name and the slider itself.
struct SliderRow {
    label: LocalizedString,
    slider: EffectSlider,
}

/// A card's slider over a DSP parameter: the range, step and default come
/// from the facade, and a change goes to the view-model.
fn dsp_slider(
    vm: &EffectsViewModel,
    param: DspParam,
    label: fn() -> LocalizedString,
    name: fn() -> LocalizedString,
) -> SliderRow {
    let decimals = match param {
        DspParam::ReverbWet | DspParam::ReverbRoomSize => 2,
        _ => 1,
    };
    let handler = vm.clone();
    SliderRow {
        label: label(),
        slider: EffectSlider::new(
            vm.value(param),
            param.range(),
            param.default_value(),
            name(),
        )
        .step(param.step())
        .decimals(decimals)
        .on_change(move |value, ctx| handler.set_value(param, value, ctx)),
    }
}

/// How high a card is.
#[derive(Clone, Copy, PartialEq, Debug)]
enum CardSize {
    /// As high as its content.
    Plain,
    /// 92 px (`fixed_height`, `audio_center.rs:3377`).
    Tall,
}

/// A card of a DSP effect with `sliders`.
fn dsp_card(
    vm: &EffectsViewModel,
    effect: DspEffect,
    title: fn() -> LocalizedString,
    sliders: Vec<SliderRow>,
) -> impl Widget + 'static {
    let size = if sliders.len() > 1 {
        CardSize::Tall
    } else {
        CardSize::Plain
    };
    let handler = vm.clone();
    card(
        vm.enabled(effect),
        title,
        None::<Spacer>,
        move |on, ctx| handler.set_enabled(effect, on, ctx),
        sliders,
        size,
        0.0,
    )
}

/// One of the three downmix cards: a switch and a 0..2 mix slider with two
/// decimals (`audio_center.rs:3402-3424`).
fn downmix_card(
    vm: &EffectsViewModel,
    channel: DownmixChannel,
    title: fn() -> LocalizedString,
    name: fn() -> LocalizedString,
) -> impl Widget + 'static {
    let handler = vm.clone();
    let on_mix = vm.clone();
    let slider = SliderRow {
        label: tr!(ac_effects_mix()),
        slider: EffectSlider::new(
            vm.downmix_mix(channel),
            DOWNMIX_RANGE,
            crate::frontend::commands::dsp::downmix_default(channel),
            name(),
        )
        .step(DOWNMIX_STEP)
        .decimals(2)
        .on_change(move |value, ctx| on_mix.set_downmix_mix(channel, value, ctx)),
    };
    card(
        vm.downmix_enabled(channel),
        title,
        None::<Spacer>,
        move |on, ctx| handler.set_downmix_enabled(channel, on, ctx),
        vec![slider],
        CardSize::Plain,
        0.0,
    )
}

/// The Natural / Surround switch beside the expander's title
/// (`audio_center.rs:3470-3495`): 5 px between "Natural", the 13 px switch
/// and "Surround", all 10 px secondary text.
fn mode_switch(vm: &EffectsViewModel) -> impl Widget + 'static {
    let handler = vm.clone();
    let switch = stock_styles::toggle(
        Toggle::new(vm.surround()).on_change(move |on, ctx| handler.set_surround(on, ctx)),
        ToggleLook::expander_mode(),
        tr!(ac_effects_expander_mode_name()),
    );
    HStack::new()
        .spacing(5.0)
        .alignment(VAlignment::Center)
        .child(
            TextWidget::new(tr!(ac_effects_mode_natural()))
                .style(inter(10.0, FontWeight::REGULAR))
                .color(TextRole::Secondary)
                .single_line(),
        )
        .child(switch)
        .child(
            TextWidget::new(tr!(ac_effects_mode_surround()))
                .style(inter(10.0, FontWeight::REGULAR))
                .color(TextRole::Secondary)
                .single_line(),
        )
}

/// A card: the frame, the title row with its switch, and the slider rows.
///
/// `extra_top` widens the gap under the title row (the limiter's 12 px).
fn card<E: Widget + 'static>(
    enabled: Signal<bool>,
    title: fn() -> LocalizedString,
    extra: Option<E>,
    on_toggle: impl Fn(bool, &mut EventContext) + 'static,
    sliders: Vec<SliderRow>,
    size: CardSize,
    extra_top: f32,
) -> impl Widget + 'static {
    let switch = stock_styles::toggle(
        Toggle::new(enabled.clone()).on_change(on_toggle),
        ToggleLook::effect(),
        title(),
    );
    let title_row = FixedSize::new().height(TITLE_ROW).child(
        HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(
                TextWidget::new(title())
                    .style(inter(13.0, FontWeight::MEDIUM))
                    .color(TextRole::Primary)
                    .single_line(),
            )
            .child(gap(TITLE_TO_EXTRA))
            .child_opt(extra)
            .child(Expand::horizontal().child(Spacer::new()))
            .child(switch),
    );
    let mut content = VStack::new()
        .spacing(0.0)
        .alignment(HAlignment::Leading)
        .child(Expand::horizontal().child(title_row))
        .child(drop_down(TITLE_GAP + extra_top));
    for (index, row) in sliders.into_iter().enumerate() {
        if index > 0 {
            content = content.child(drop_down(SLIDER_GAP));
        }
        content = content.child(Expand::horizontal().child(slider_row(row)));
    }
    let border = enabled.map(|on| {
        if *on {
            SurfaceRole::Accent
        } else {
            SurfaceRole::Raised
        }
    });
    let frame = |content: Frame| {
        content
            .fill(SurfaceRole::Raised)
            .border(border, CARD_BORDER)
            .radius(CARD_RADIUS)
            .padding(
                CARD_PADDING_Y,
                CARD_PADDING_X,
                CARD_PADDING_Y,
                CARD_PADDING_X,
            )
    };
    let frame = match size {
        CardSize::Plain => frame(Frame::new(content)),
        CardSize::Tall => frame(Frame::new(
            FixedSize::new()
                .height(TALL_CARD_HEIGHT - 2.0 * CARD_PADDING_Y)
                .child(content),
        )),
    };
    Expand::horizontal().child(frame)
}

/// A slider row: the 11 px medium label, 10 px, the slider filling the rest.
fn slider_row(row: SliderRow) -> impl Widget + 'static {
    HStack::new()
        .spacing(0.0)
        .alignment(VAlignment::Center)
        .child(
            TextWidget::new(row.label)
                .style(inter(11.0, FontWeight::MEDIUM))
                .color(TextRole::Secondary)
                .single_line(),
        )
        .child(gap(LABEL_GAP))
        .child(Expand::horizontal().child(row.slider))
}

/// The divider between a column's effects and its downmix card
/// (`audio_center.rs:3425-3437`): a 1 px line in the secondary colour, 15 px
/// in from the column's sides.
fn divider() -> impl Widget + 'static {
    Expand::horizontal().child(
        FixedSize::new().height(DIVIDER_HEIGHT).child(
            Padding::new(6.0, DIVIDER_INSET, 4.0, DIVIDER_INSET).child(
                FixedSize::new()
                    .height(1.0)
                    .child(RectWidget::new().background(TextRole::Secondary)),
            ),
        ),
    )
}

/// The heading above the centre downmix card (`audio_center.rs:3459-3472`),
/// 12 px medium, centred, with the 2 px negative padding of Iced applied to
/// its line.
fn heading() -> impl Widget + 'static {
    Expand::horizontal().child(
        FixedSize::new().height(HEADING_HEIGHT).child(
            Center::new().child(
                TextWidget::new(tr!(ac_effects_downmix_heading()))
                    .style(inter(12.0, FontWeight::MEDIUM))
                    .color(TextRole::Primary)
                    .single_line(),
            ),
        ),
    )
}

#[cfg(test)]
mod tests;
