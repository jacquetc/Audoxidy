//! The Audio Center's configuration tab, "Configuración de Audio"
//! (`view_audio_config`, `src/gui/audio_center.rs:2033-2779`).
//!
//! - [`ConfigViewModel`] holds the pending selections (host, device, sample
//!   rate, bit depth, channels, buffer, and the sound server's rate and
//!   quantum), Apply and its busy state, the information column's rows and
//!   the polled sound-server status;
//! - [`ConfigTab`] is the tab's widget.
//!
//! The device lists, the output stream and the current track come from the
//! Layer A [`AudioDevicesModel`](crate::teksilo_ui::models::AudioDevicesModel),
//! which enumerates them on the engine worker; the sound-server status is
//! polled every 3 s, also off the UI thread, while the tab is on screen.

pub mod config_tab;
pub mod config_vm;

pub use config_tab::ConfigTab;
pub use config_vm::ConfigViewModel;
