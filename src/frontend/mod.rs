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

pub mod adapters;
pub mod app_context;
pub mod commands;
pub mod db_worker;
pub mod engine_worker;
pub mod error;
pub mod events;
#[cfg(any(test, feature = "teksilo-mocks"))]
pub mod fakes;
pub mod long_ops;
pub mod monitor;
pub mod ports;
pub mod preload;
pub mod reply;
pub mod session;
pub mod settings;
#[cfg(any(test, feature = "teksilo-mocks"))]
pub mod test_fixtures;
pub mod threading;

pub use app_context::AppContext;
pub use db_worker::DbWorker;
pub use engine_worker::EngineWorker;
pub use error::FrontendError;
pub use events::{Event, EventHub, Origin, SubscriptionToken};
pub use long_ops::{LongOperations, OpContext};
pub use reply::{Latest, Reply, Ticket, TrackedReply};
pub use threading::{is_ui_thread, mark_ui_thread};
