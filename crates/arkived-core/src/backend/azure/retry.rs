//! Retry policy for transient errors.
//!
//! Retries are triggered by `Error::Throttled`, `Error::NetworkTransient`,
//! and `Error::Backend(...)` where the message indicates a 5xx (we treat
//! all Backend errors as retryable for simplicity — 5xx responses already
//! map there via `map_rest_error`).
//!
//! Schedule: exponential backoff with full jitter, capped at 30s, max
//! 8 attempts. `Throttled { retry_after }` respects the server's
//! suggested delay as a lower bound.

use crate::Error;
use std::time::Duration;

/// Max retry attempts (including the first).
pub(crate) const MAX_ATTEMPTS: usize = 8;

/// Maximum backoff cap.
pub(crate) const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Run `f` with exponential-backoff retries on transient errors.
pub(crate) async fn with_retries<F, Fut, T>(mut f: F) -> crate::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = crate::Result<T>>,
{
    let mut attempt = 0usize;
    loop {
        attempt += 1;
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if !is_retryable(&e) || attempt >= MAX_ATTEMPTS => return Err(e),
            Err(e) => {
                let delay = backoff_for(attempt, &e);
                tracing::debug!(attempt, ?delay, ?e, "retryable error; backing off");
                tokio::time::sleep(delay).await;
            }
        }
    }
}

fn is_retryable(e: &Error) -> bool {
    matches!(
        e,
        Error::Throttled { .. } | Error::NetworkTransient(_) | Error::Backend(_)
    )
}

fn backoff_for(attempt: usize, e: &Error) -> Duration {
    // Respect server-directed retry-after if present.
    if let Error::Throttled { retry_after } = e {
        return (*retry_after).min(MAX_BACKOFF);
    }

    // Exponential with full jitter. base = 100ms * 2^(attempt-1), capped.
    let base_ms: u64 = 100u64.saturating_mul(1u64 << (attempt.saturating_sub(1).min(10)));
    let capped = base_ms.min(MAX_BACKOFF.as_millis() as u64);
    // Full jitter: uniform in [0, capped].
    let jittered = fastrand_u64(capped);
    Duration::from_millis(jittered)
}

fn fastrand_u64(max_exclusive: u64) -> u64 {
    if max_exclusive == 0 {
        return 0;
    }
    // Tiny LCG keyed off nanosecond clock — good enough for retry jitter.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 | (d.as_secs() << 32))
        .unwrap_or(0);
    (nanos
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407))
        % max_exclusive
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn retries_on_transient_then_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = attempts.clone();
        let out: crate::Result<i32> = with_retries(|| {
            let a = a2.clone();
            async move {
                let n = a.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(Error::NetworkTransient("flaky".into()))
                } else {
                    Ok(42)
                }
            }
        })
        .await;
        assert_eq!(out.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn non_retryable_error_fails_fast() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = attempts.clone();
        let out: crate::Result<i32> = with_retries(|| {
            let a = a2.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Err(Error::NotFound {
                    resource: "x".into(),
                })
            }
        })
        .await;
        assert!(matches!(out, Err(Error::NotFound { .. })));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = attempts.clone();
        let out: crate::Result<i32> = with_retries(|| {
            let a = a2.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Err(Error::NetworkTransient("always".into()))
            }
        })
        .await;
        assert!(matches!(out, Err(Error::NetworkTransient(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), MAX_ATTEMPTS);
    }
}
