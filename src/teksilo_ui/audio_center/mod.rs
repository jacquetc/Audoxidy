//! The Audio Center: the floating 940 × 480 panel with the audio settings
//! (`src/gui/audio_center.rs`), opened from the player menu.
//!
//! - [`shell`]: the panel itself (frame, header, tabs, dragging, closing) and
//!   its view-model, which owns the open state through
//!   `AppServices::links.audio_center`;
//! - one module per tab, each with its own view-model: [`config`]
//!   ("Configuración de Audio"), [`equalizer`] ("Ecualizador"), [`effects`]
//!   ("Efectos de Audio") and [`volume_mix`] ("Volumen y Mezcla").

pub mod config;
pub mod effects;
pub mod equalizer;
pub mod shell;
pub mod volume_mix;
