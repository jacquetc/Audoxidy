//! The Audio Center's effects tab, "Efectos de Audio" (`view_audio_effects`,
//! `src/gui/audio_center.rs:3036`): the nine DSP effect cards in three
//! columns and the three downmix cards.
//!
//! - [`EffectsViewModel`]: the signals over the engine's effect chain and the
//!   methods the controls call; every change goes through the facade's
//!   `commands::dsp`, which applies it and persists it;
//! - [`EffectsTab`]: the body the Audio Center shell shows on this tab,
//!   `EffectsTab::new(EffectsViewModel::new(&services))`.

mod effects_vm;
mod tab;

pub use effects_vm::{EFFECTS, EffectsViewModel};
pub use tab::{
    CARD_HEIGHT, DIVIDER_HEIGHT, EffectsTab, GAP, HEADING_HEIGHT, TALL_CARD_HEIGHT, TOP_INSET,
};
