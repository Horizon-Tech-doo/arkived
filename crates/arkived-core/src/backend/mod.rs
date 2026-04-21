//! Storage backend trait and implementations.
//!
//! The [`StorageBackend`] trait is intentionally `pub(crate)` — an internal
//! seam that will enable future backends (S3, GCS, etc.) to be added without
//! restructuring the core. It is **not** part of the public API.
//!
//! Public consumers use [`azure::AzureBlobBackend`] directly; its method
//! names and shapes are Azure-native.

pub mod azure;
pub mod types;

#[allow(unused_imports)]
pub use azure::AzureBlobBackend;
pub use types::{
    BlobEntry, BlobPath, ByteStream, Container, DeleteOpts, Page, Range, WriteOpts, WriteResult,
};

use crate::Ctx;
use async_trait::async_trait;

/// The internal storage backend abstraction.
///
/// **Stability: internal.** This trait is `pub(crate)` and may change without
/// notice. Do not implement or depend on it outside this crate.
#[async_trait]
#[allow(dead_code)] // Placeholder until ops impls are added in Tasks 9–13.
pub(crate) trait StorageBackend: Send + Sync {
    /// Human-readable name (e.g. `"azure-blob"`).
    fn name(&self) -> &'static str;

    /// List containers under the storage account.
    async fn list_containers(
        &self,
        ctx: &Ctx,
        continuation: Option<String>,
    ) -> crate::Result<Page<Container>>;

    /// List blobs under a container with optional prefix and delimiter.
    ///
    /// When `delimiter` is `Some("/")`, virtual directories emerge as
    /// [`BlobEntry::Prefix`] entries.
    async fn list_blobs(
        &self,
        ctx: &Ctx,
        container: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        continuation: Option<String>,
    ) -> crate::Result<Page<BlobEntry>>;

    /// Stream the bytes of a blob.
    async fn read_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        range: Option<Range>,
    ) -> crate::Result<ByteStream>;

    /// Upload a blob. Calls `ctx.policy.confirm` before overwriting an
    /// existing blob.
    async fn write_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        body: ByteStream,
        opts: WriteOpts,
    ) -> crate::Result<WriteResult>;

    /// Delete a blob. Always policy-gated.
    async fn delete_blob(&self, ctx: &Ctx, path: &BlobPath, opts: DeleteOpts) -> crate::Result<()>;
}
