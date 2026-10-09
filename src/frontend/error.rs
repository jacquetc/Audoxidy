//! The single error type of the facade.
//!
//! Commands, workers and long operations all report failures as a
//! [`FrontendError`], so a view-model handles one type whatever the layer that
//! failed: the database, the audio engine, the file system, or the facade's own
//! threads.

#![warn(missing_docs)]

use crate::audio::AudioError;

/// Every failure the facade reports to a UI.
#[derive(Debug, thiserror::Error)]
pub enum FrontendError {
    /// A database query or statement failed.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// The audio engine refused a command or failed to rebuild its stream.
    #[error("audio error: {0}")]
    Audio(#[from] AudioError),
    /// A file-system operation failed (import, export, show-in-folder...).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The named worker thread has stopped, so the request was not run (or its
    /// reply was lost). This is what every request returns after
    /// [`AppContext::shutdown`](super::app_context::AppContext::shutdown).
    #[error("the {0} worker has stopped")]
    WorkerStopped(&'static str),
    /// A job panicked on the named worker. The worker itself keeps running.
    #[error("a job panicked on the {0} worker")]
    JobPanicked(&'static str),
    /// A coalesced write was replaced by a newer write with the same key
    /// before it started, so it never ran (last write wins).
    #[error("the write was superseded by a newer one with the same key")]
    Superseded,
    /// The operation was cancelled before it finished.
    #[error("the operation was cancelled")]
    Cancelled,
    /// The caller passed a value the facade cannot use.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_errors_convert_into_frontend_errors() {
        let db: FrontendError = rusqlite::Error::InvalidQuery.into();
        assert!(matches!(db, FrontendError::Database(_)));
        let audio: FrontendError = AudioError::NoActiveOutput.into();
        assert!(matches!(audio, FrontendError::Audio(_)));
        let io: FrontendError = std::io::Error::other("boom").into();
        assert!(matches!(io, FrontendError::Io(_)));
    }

    #[test]
    fn the_messages_name_the_worker_and_the_cause() {
        assert_eq!(
            FrontendError::WorkerStopped("database").to_string(),
            "the database worker has stopped"
        );
        assert_eq!(
            FrontendError::InvalidInput("empty name".into()).to_string(),
            "invalid input: empty name"
        );
        assert_eq!(
            FrontendError::Cancelled.to_string(),
            "the operation was cancelled"
        );
    }
}
