//! The Audio Center's volume-and-mix tab, "Volumen y Mezcla"
//! (`view_volumen_mezcla`, `src/gui/audio_center.rs:3708-4664`).
//!
//! - [`VolumeMixViewModel`] holds the fourteen switches and ten values of the
//!   tab, Iced's locking rules between a group's master switch and what is in
//!   the group, and writes every change through
//!   [`commands::mix`](crate::frontend::commands::mix), which applies it to the
//!   engine and stores it at once;
//! - [`VolumeMixTab`] is the tab's widget: five groups, each a master check
//!   box with a clickable title, and under it switches with clickable labels
//!   and [`Stepper`](crate::teksilo_ui::widgets::Stepper)s.

pub mod volume_mix_tab;
pub mod volume_mix_vm;

pub use volume_mix_tab::VolumeMixTab;
pub use volume_mix_vm::VolumeMixViewModel;
