//! Shared request/response types for the backend layer.

use bytes::Bytes;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use time::OffsetDateTime;

/// A fully-qualified blob path: `(container, blob_name)`.
///
/// `blob_name` may contain `/` — the delimiter is just a name part, not a
/// filesystem separator. ADLS Gen2 uses the same path as hierarchical name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobPath {
    /// Container name (lowercase, letters/digits/hyphens, 3-63 chars).
    pub container: String,
    /// Blob name (slash-delimited for hierarchical paths).
    pub blob: String,
}

impl BlobPath {
    /// Construct from container + blob.
    pub fn new(container: impl Into<String>, blob: impl Into<String>) -> Self {
        Self { container: container.into(), blob: blob.into() }
    }
}

/// A list page — items plus an optional continuation token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    /// Items on this page.
    pub items: Vec<T>,
    /// Continuation token for the next page, or `None` if this is the last.
    pub continuation: Option<String>,
}

/// A container in the list-containers response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Container {
    /// Container name.
    pub name: String,
    /// Last-modified timestamp.
    pub last_modified: Option<OffsetDateTime>,
    /// ETag.
    pub etag: Option<String>,
    /// Lease status (e.g. `"available"`, `"leased"`).
    pub lease_status: Option<String>,
    /// Lease state.
    pub lease_state: Option<String>,
    /// Public-access level (`"blob"`, `"container"`, or `None` for private).
    pub public_access: Option<String>,
}

/// A blob or virtual directory entry in the list-blobs response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobEntry {
    /// A concrete blob.
    Blob {
        /// Full blob name.
        name: String,
        /// Size in bytes.
        size: u64,
        /// Blob type (`"BlockBlob"`, `"PageBlob"`, `"AppendBlob"`).
        blob_type: String,
        /// Access tier (`"Hot"`, `"Cool"`, `"Cold"`, `"Archive"`).
        tier: Option<String>,
        /// ETag.
        etag: Option<String>,
        /// Content-Type header from upload.
        content_type: Option<String>,
        /// Last-modified timestamp.
        last_modified: Option<OffsetDateTime>,
        /// Lease state.
        lease_state: Option<String>,
    },
    /// A virtual directory prefix (emitted when `delimiter` is used).
    Prefix {
        /// The directory name (includes trailing delimiter).
        name: String,
    },
}

/// An HTTP byte range for `read_blob`.
#[derive(Debug, Clone, Copy)]
pub struct Range {
    /// Start offset (inclusive).
    pub start: u64,
    /// End offset (inclusive). `None` = to end of blob.
    pub end: Option<u64>,
}

/// Upload options.
#[derive(Debug, Clone, Default)]
pub struct WriteOpts {
    /// If `false`, fail with `Conflict` when the blob already exists.
    pub overwrite: bool,
    /// Conditional: only overwrite if server ETag matches.
    pub if_match: Option<String>,
    /// Content-Type metadata to set on the blob.
    pub content_type: Option<String>,
    /// Arbitrary blob metadata headers.
    pub metadata: HashMap<String, String>,
    /// Max block size in bytes. Default 4 MiB.
    pub block_size: Option<usize>,
    /// Max parallel block uploads. Default 8.
    pub max_parallelism: Option<usize>,
}

/// Result of a successful upload.
#[derive(Debug, Clone)]
pub struct WriteResult {
    /// Server-assigned ETag.
    pub etag: String,
    /// Server-reported last-modified timestamp.
    pub last_modified: Option<OffsetDateTime>,
    /// Blob type (always `"BlockBlob"` for v0.1.0).
    pub blob_type: String,
}

/// Delete options.
#[derive(Debug, Clone, Default)]
pub struct DeleteOpts {
    /// Delete snapshots too. Required if the blob has any (otherwise 409).
    pub include_snapshots: bool,
}

/// Convenience alias for a byte-producing stream.
pub type ByteStream = BoxStream<'static, crate::Result<Bytes>>;
