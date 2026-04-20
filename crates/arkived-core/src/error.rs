//! Error and result types for Arkived.

use std::result::Result as StdResult;

/// Alias for `Result` with the Arkived [`Error`] type.
pub type Result<T> = StdResult<T, Error>;

/// Top-level Arkived error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An operation was denied by the policy layer.
    #[error("operation denied by policy: {0}")]
    PolicyDenied(String),

    /// A storage backend returned an error.
    #[error("storage backend error: {0}")]
    Backend(String),

    /// An authentication error.
    #[error("authentication error: {0}")]
    Auth(String),

    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A catch-all for other errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
