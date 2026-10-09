//! The commands a UI's view-models call: free functions taking the
//! [`AppContext`](super::app_context::AppContext) and returning
//! `Result<_, FrontendError>` or a [`Reply`](super::reply::Reply).
//!
//! One module per area lives here (playback, playlist, library, filters,
//! audio settings, EQ, DSP, mix, meter, files); wave B2 of the Teksilo port
//! adds them on top of the facade's workers, ports and event hub.

pub mod audio_settings;
pub mod dsp;
pub mod engine_settings;
pub mod eq;
pub mod files;
pub mod filters;
pub mod library;
pub mod meter;
pub mod mix;
