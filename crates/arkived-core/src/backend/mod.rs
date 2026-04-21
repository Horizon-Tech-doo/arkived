//! Storage backend trait and implementations.
//!
//! This module is intentionally `pub(crate)`. The [`StorageBackend`] trait is an
//! internal seam that will enable future backends (S3, GCS, etc.) to be added
//! without restructuring the core. It is **not** part of the public API.
//!
//! Public consumers should use the Azure-native functions exposed from the
//! `arkived-core` crate root (to be added in Stage 1).

use async_trait::async_trait;

/// The internal storage backend abstraction.
///
/// **Stability: internal.** This trait is `pub(crate)` and may change without
/// notice. Do not implement or depend on it outside this crate.
#[async_trait]
#[allow(dead_code)] // Placeholder until the Backend plan fleshes out concrete impls.
pub(crate) trait StorageBackend: Send + Sync {
    /// A human-readable name for this backend (e.g. `"azure-blob"`).
    fn name(&self) -> &'static str;

    // Full trait will be fleshed out in Stage 1:
    //   list_containers, list_blobs, read_blob, write_blob,
    //   delete_blob, generate_sas, etc.
    //
    // Each destructive method MUST route through the Policy layer
    // before executing.
}

pub mod types;

// Placeholder module — AzureBackend goes here in Stage 1:
// pub(crate) mod azure;
