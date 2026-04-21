//! # arkived-core
//!
//! The shared core library for [Arkived](https://arkived.app) — a fast, open-source,
//! Rust-native storage client for Microsoft Azure.
//!
//! This crate provides:
//!
//! - A `StorageBackend` trait seam for future multi-cloud support (currently `pub(crate)`)
//! - An `AzureBackend` implementation (to be added in Stage 1)
//! - A [`Policy`] trait for human-in-the-loop confirmation of destructive operations
//! - Shared error types, progress events, and auth abstractions
//!
//! # Design note
//!
//! The public API of `arkived-core` is **Azure-first**. Do not program against the
//! `StorageBackend` trait as a public interface — it is intentionally `pub(crate)`
//! until the project decides whether to expose it as part of a stable multi-cloud API.

#![deny(rust_2018_idioms, unsafe_code, missing_docs)]
#![warn(clippy::all)]

pub mod auth;
pub mod config;
pub mod ctx;
pub mod error;
pub mod policy;
pub mod progress;
pub mod store;
pub mod types;

pub mod backend;

// Re-export policy: the crate root surfaces top-level contracts (traits,
// primary entry points, shared error types). Data-record types that only
// matter alongside a specific subsystem (SignIn, Subscription, StorageAccount,
// AttachedResource, CurrentContext, PolicyAllowEntry; CredentialStore, OsKeyring;
// ProgressEvent, ProgressSink, NoopSink, MemorySink) stay reachable via their
// module path (`arkived_core::store::SignIn` etc.) to keep the top-level namespace
// focused.
pub use auth::{AuthProvider, ResolvedCredential};
pub use backend::{
    AzureBlobBackend, BlobEntry, BlobPath, ByteStream, Container, DeleteOpts, Page, Range,
    WriteOpts, WriteResult,
};
pub use config::{ArkivedConfig, ConfirmMode, OutputFormat};
pub use ctx::{CancellationToken, Ctx};
pub use error::{Error, Result};
pub use policy::{Action, ActionContext, Policy, PolicyDecision};
pub use store::Store;
pub use types::{AuthKind, AzureEnvironment, ResourceKind};
