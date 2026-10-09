//! UI-agnostic application logic, shared by the Iced UI (`gui`) and the
//! Teksilo UI (`teksilo_ui`).
//!
//! Everything here is pure: no GUI types, no database or audio-engine handles,
//! no threads. The Iced view-model managers delegate to these functions, and
//! the Teksilo view-models call the same code, so the two UIs cannot drift
//! apart on sorting, selection, playback order or value formatting.
//!
//! New code in this tree is held to a stricter bar than the code that predates
//! the Teksilo port: any warning or clippy finding is an error, and `unwrap()`
//! / `expect()` are only allowed in tests (see `clippy.toml`).

#![deny(warnings, clippy::all, clippy::unwrap_used, clippy::expect_used)]

pub mod selection_modifiers;
