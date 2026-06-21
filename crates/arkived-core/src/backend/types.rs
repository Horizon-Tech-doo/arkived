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
        Self {
            container: container.into(),
            blob: blob.into(),
        }
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

/// Blob access tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// Hot — frequent access, lowest access cost.
    Hot,
    /// Cool — infrequent access, ~30-day minimum.
    Cool,
    /// Cold — rare access, ~90-day minimum.
    Cold,
    /// Archive — offline, must be rehydrated before read.
    Archive,
}

impl Tier {
    /// The Azure `x-ms-access-tier` header value for this tier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Hot => "Hot",
            Tier::Cool => "Cool",
            Tier::Cold => "Cold",
            Tier::Archive => "Archive",
        }
    }

    /// Parse a tier from a case-insensitive string (e.g. CLI argument).
    pub fn parse(s: &str) -> Option<Tier> {
        match s.to_ascii_lowercase().as_str() {
            "hot" => Some(Tier::Hot),
            "cool" => Some(Tier::Cool),
            "cold" => Some(Tier::Cold),
            "archive" => Some(Tier::Archive),
            _ => None,
        }
    }
}

/// Container public-access level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicAccess {
    /// No anonymous access (private container).
    Private,
    /// Anonymous read access to blobs only.
    Blob,
    /// Anonymous read access to blobs and container metadata.
    Container,
}

impl PublicAccess {
    /// The `x-ms-blob-public-access` header value, or `None` for `Private`
    /// (the header is omitted entirely for private containers).
    pub fn header_value(&self) -> Option<&'static str> {
        match self {
            PublicAccess::Private => None,
            PublicAccess::Blob => Some("blob"),
            PublicAccess::Container => Some("container"),
        }
    }

    /// Parse from a case-insensitive string (`private`/`blob`/`container`).
    pub fn parse(s: &str) -> Option<PublicAccess> {
        match s.to_ascii_lowercase().as_str() {
            "private" | "none" | "off" => Some(PublicAccess::Private),
            "blob" => Some(PublicAccess::Blob),
            "container" => Some(PublicAccess::Container),
            _ => None,
        }
    }
}

/// System (HTTP) properties of a blob, as returned by `get_properties`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobProperties {
    /// Size of the blob in bytes (`Content-Length`).
    pub content_length: u64,
    /// `Content-Type`.
    pub content_type: Option<String>,
    /// `Content-Encoding`.
    pub content_encoding: Option<String>,
    /// `Content-Language`.
    pub content_language: Option<String>,
    /// `Cache-Control`.
    pub cache_control: Option<String>,
    /// `Content-Disposition`.
    pub content_disposition: Option<String>,
    /// `Content-MD5` (base64).
    pub content_md5: Option<String>,
    /// `ETag`.
    pub etag: Option<String>,
    /// Blob type (`"BlockBlob"`, `"PageBlob"`, `"AppendBlob"`).
    pub blob_type: Option<String>,
    /// Access tier (`"Hot"`, `"Cool"`, `"Cold"`, `"Archive"`).
    pub access_tier: Option<String>,
    /// Lease state (`"available"`, `"leased"`, …).
    pub lease_state: Option<String>,
    /// Lease status (`"locked"`, `"unlocked"`).
    pub lease_status: Option<String>,
}

/// Settable system (HTTP) properties for `set_properties`.
///
/// Azure's "Set Blob Properties" replaces *all* system properties in one call:
/// any field left `None` is cleared on the blob. Callers should read current
/// properties first if they want to preserve unspecified fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlobPropertiesUpdate {
    /// `Content-Type`.
    pub content_type: Option<String>,
    /// `Content-Encoding`.
    pub content_encoding: Option<String>,
    /// `Content-Language`.
    pub content_language: Option<String>,
    /// `Cache-Control`.
    pub cache_control: Option<String>,
    /// `Content-Disposition`.
    pub content_disposition: Option<String>,
    /// `Content-MD5` (base64).
    pub content_md5: Option<String>,
}

/// The resource a SAS grants access to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SasResource {
    /// A whole container (signed resource `"c"`).
    Container(String),
    /// A single blob (signed resource `"b"`).
    Blob(BlobPath),
}

/// SAS protocol restriction (`spr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SasProtocol {
    /// HTTPS only (`"https"`). The safe default.
    HttpsOnly,
    /// HTTPS or HTTP (`"https,http"`).
    HttpsAndHttp,
}

impl SasProtocol {
    /// The `spr` query/sign value.
    pub fn as_str(&self) -> &'static str {
        match self {
            SasProtocol::HttpsOnly => "https",
            SasProtocol::HttpsAndHttp => "https,http",
        }
    }
}

/// Options for generating a Service SAS.
#[derive(Debug, Clone)]
pub struct SasOptions {
    /// Permission letters (e.g. `"r"`, `"rwd"`). Reordered into Azure's
    /// canonical order during signing; unknown letters are dropped.
    pub permissions: String,
    /// Expiry time (`se`).
    pub expiry: OffsetDateTime,
    /// Optional start time (`st`).
    pub start: Option<OffsetDateTime>,
    /// Protocol restriction (`spr`).
    pub protocol: SasProtocol,
    /// Optional allowed IP or IP range (`sip`).
    pub ip: Option<String>,
}

/// Convenience alias for a byte-producing stream.
pub type ByteStream = BoxStream<'static, crate::Result<Bytes>>;
