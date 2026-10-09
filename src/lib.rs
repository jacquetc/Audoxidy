pub mod audio;
pub mod db;
pub mod frontend;
#[cfg(feature = "iced-ui")]
pub mod gui;
pub mod integrations;
pub mod logic;
pub mod utils;

#[cfg(test)]
#[path = "audio/dsp_tests.rs"]
mod dsp_tests;

#[cfg(all(test, feature = "iced-ui"))]
#[path = "gui/widgets_tests.rs"]
mod widgets_tests;
