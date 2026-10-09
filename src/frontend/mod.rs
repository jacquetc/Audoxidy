//! UI-agnostic application facade used by the Teksilo UI (`teksilo_ui`).
//!
//! It plays the role Skribisto's `frontend` crate plays there: an
//! [`AppContext`](app_context) owning the backend (database, scanner, audio
//! engine, media controls, system audio), free-function commands that the
//! view-models call, and an event hub that tells the UI what changed. The UI
//! never touches the database or the audio engine directly.
//!
//! Threading model:
//! - every database access runs on one ordered worker thread;
//! - engine calls that rebuild or reconfigure the audio stream, and the system
//!   audio subprocesses, run on a second ordered worker;
//! - a monitor thread polls the engine and turns state changes into events;
//! - events are delivered on the hub's dispatch thread; a UI marshals them to
//!   its own thread (Teksilo does this through its `EventSource` bridge).
//!
//! New code in this tree is held to a stricter bar than the code that predates
//! the Teksilo port: any warning or clippy finding is an error, and `unwrap()`
//! / `expect()` are only allowed in tests (see `clippy.toml`).

#![deny(warnings, clippy::all, clippy::unwrap_used, clippy::expect_used)]
