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

    /// An authentication error with detail.
    #[error("authentication failed: {0}")]
    AuthFailed(String),

    /// The current credential has expired.
    #[error("authentication expired; sign in again")]
    AuthExpired,

    /// Resource not found.
    #[error("not found: {resource}")]
    NotFound {
        /// The canonical path or identifier of the missing resource.
        resource: String,
    },

    /// A conflict (ETag mismatch, lease held, etc.).
    #[error("conflict{}: {detail}", etag.as_deref().map(|e| format!(" (etag {e})")).unwrap_or_default())]
    Conflict {
        /// Human-readable description of the conflict.
        detail: String,
        /// The ETag returned by the server, if available.
        etag: Option<String>,
    },

    /// Server throttled; caller may retry after the given duration.
    #[error("throttled by server (retry after {}s)", retry_after.as_secs())]
    Throttled {
        /// Server-recommended backoff before retry.
        retry_after: std::time::Duration,
    },

    /// Transient network error.
    #[error("network transient: {0}")]
    NetworkTransient(String),

    /// Generic authentication error (for migration — prefer AuthFailed or AuthExpired).
    #[error("authentication error: {0}")]
    Auth(String),

    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A catch-all for other errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn error_variants_display() {
        assert_eq!(
            Error::NotFound { resource: "container/foo".into() }.to_string(),
            "not found: container/foo"
        );
        assert_eq!(
            Error::Conflict { detail: "etag mismatch".into(), etag: Some("0xABC".into()) }.to_string(),
            "conflict (etag 0xABC): etag mismatch"
        );
        assert_eq!(
            Error::Throttled { retry_after: Duration::from_secs(5) }.to_string(),
            "throttled by server (retry after 5s)"
        );
        assert_eq!(Error::AuthExpired.to_string(), "authentication expired; sign in again");
        assert!(matches!(
            Error::NetworkTransient("dns".into()),
            Error::NetworkTransient(_)
        ));
        assert!(matches!(
            Error::AuthFailed("bad creds".into()),
            Error::AuthFailed(_)
        ));
    }
}
