//! Exponential backoff with jitter. Filled in Task 7.

pub(crate) async fn with_retries<F, Fut, T>(mut f: F) -> crate::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = crate::Result<T>>,
{
    f().await
}
