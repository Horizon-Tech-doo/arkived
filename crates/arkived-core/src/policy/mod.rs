//! Policy layer — human-in-the-loop confirmation of destructive operations.
//!
//! Every surface that embeds `arkived-core` (CLI, MCP server, ACP host, Tauri app)
//! provides its own [`Policy`] implementation. The core never performs a destructive
//! operation without calling [`Policy::confirm`] first.

use async_trait::async_trait;

/// A description of an action being proposed.
#[derive(Debug, Clone)]
pub struct Action {
    /// Short verb describing the action (e.g. `"delete_blob"`).
    pub verb: String,
    /// Target resource (e.g. `"mycontainer/file.txt"`).
    pub target: String,
    /// Human-readable summary shown to the user.
    pub summary: String,
    /// Whether this action is reversible.
    pub reversible: bool,
}

/// Additional context the policy layer may use to decide.
#[derive(Debug, Clone, Default)]
pub struct ActionContext {
    /// Estimated cost of the operation in USD (if computable).
    pub cost_usd: Option<f64>,
    /// Number of sub-items affected (e.g. blobs in a bulk delete).
    pub item_count: Option<u64>,
    /// Who initiated the action (e.g. `"user"`, `"agent:claude-code"`).
    pub initiator: Option<String>,
}

/// The decision returned by a [`Policy`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Proceed with the action.
    Allow,
    /// Proceed, and remember this decision for the rest of the session.
    AllowAlways,
    /// Reject the action. Contains a human-readable reason.
    Deny(String),
}

/// The policy contract every surface must implement.
///
/// Surface-specific implementations:
///
/// - **CLI:** prompts on stdin (`"Delete 247 blobs? [y/N]"`)
/// - **Tauri:** displays a modal dialog
/// - **MCP:** returns an MCP elicitation request to the LLM client
/// - **ACP:** forwards through ACP's permission flow
#[async_trait]
pub trait Policy: Send + Sync {
    /// Called before any destructive or elevated operation.
    ///
    /// `arkived-core` guarantees this is invoked before every
    /// `delete`, `overwrite`, `generate_sas`, `set_public_access`,
    /// or `change_access_tier` operation.
    async fn confirm(&self, action: &Action, context: &ActionContext) -> PolicyDecision;
}

/// A [`Policy`] implementation that denies every destructive operation.
/// Useful as a safe default and for tests.
pub struct DenyAllPolicy;

#[async_trait]
impl Policy for DenyAllPolicy {
    async fn confirm(&self, _action: &Action, _context: &ActionContext) -> PolicyDecision {
        PolicyDecision::Deny("DenyAllPolicy denies all destructive operations".into())
    }
}

/// A [`Policy`] implementation that allows every destructive operation without prompting.
///
/// **Warning:** only use this in tests or one-off automation scripts where you
/// explicitly trust the caller. Never use this in an agent-facing surface.
pub struct AllowAllPolicy;

#[async_trait]
impl Policy for AllowAllPolicy {
    async fn confirm(&self, _action: &Action, _context: &ActionContext) -> PolicyDecision {
        PolicyDecision::Allow
    }
}
