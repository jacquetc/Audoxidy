//! The Audio Center's "Ecualizador" tab: the equalizer's controls, its 20 or
//! 31 band sliders, and the preset dialogs.
//!
//! Port of `view_equalizer` (`src/gui/audio_center.rs:2783-3032`) and of the
//! equalizer parts of `AudioCenterManager::update` and of the Load and Save
//! dialogs (`src/gui/app.rs`). The shell plugs the tab in with
//! [`EqualizerTab::new`]`(`[`EqualizerViewModel::new`]`(&services))`.
//!
//! - [`equalizer_vm`]: the state, the controls, the preset operations;
//! - [`tab`]: the tab body;
//! - [`load_dialog`] and [`save_dialog`]: the two preset dialogs;
//! - [`placement`]: where a dialog opens (Iced positions them relative to the
//!   Audio Center) and the window-sized layer that holds it.

mod buttons;
pub mod equalizer_vm;
pub mod load_dialog;
pub mod placement;
pub mod save_dialog;
pub mod tab;

pub use equalizer_vm::{EqualizerViewModel, PresetLoad};
pub use tab::EqualizerTab;
