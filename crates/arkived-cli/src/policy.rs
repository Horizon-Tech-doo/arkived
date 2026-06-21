//! The CLI's [`Policy`] implementation: confirm destructive actions on stdin.

use arkived_core::config::ConfirmMode;
use arkived_core::policy::{Action, ActionContext, Policy, PolicyDecision};
use async_trait::async_trait;
use std::io::IsTerminal;

/// Confirms (or auto-decides) destructive actions for the CLI.
///
/// - `--yes` or `ConfirmMode::Yes` → always allow.
/// - `ConfirmMode::Auto` → always deny (scripted/safe default; requires `--yes`).
/// - `ConfirmMode::Ask` → prompt on stdin when attached to a TTY, otherwise deny.
pub struct CliPolicy {
    mode: ConfirmMode,
    assume_yes: bool,
}

impl CliPolicy {
    /// Build a policy from the resolved confirm mode and the per-invocation
    /// `--yes` flag.
    pub fn new(mode: ConfirmMode, assume_yes: bool) -> Self {
        Self { mode, assume_yes }
    }
}

#[async_trait]
impl Policy for CliPolicy {
    async fn confirm(&self, action: &Action, _context: &ActionContext) -> PolicyDecision {
        if self.assume_yes || matches!(self.mode, ConfirmMode::Yes) {
            return PolicyDecision::Allow;
        }
        if matches!(self.mode, ConfirmMode::Auto) {
            return PolicyDecision::Deny(
                "auto confirm-mode denies destructive actions; pass --yes to proceed".into(),
            );
        }
        // ConfirmMode::Ask — prompt, but only if we have a terminal to read from.
        if !std::io::stdin().is_terminal() {
            return PolicyDecision::Deny(
                "no interactive terminal to confirm; re-run with --yes".into(),
            );
        }

        let summary = action.summary.clone();
        let line = tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let mut err = std::io::stderr();
            let _ = write!(err, "{summary}\nProceed? [y/N] ");
            let _ = err.flush();
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
            buf
        })
        .await
        .unwrap_or_default();

        match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => PolicyDecision::Allow,
            _ => PolicyDecision::Deny("declined at confirmation prompt".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action() -> Action {
        Action {
            verb: "delete_blob".into(),
            target: "c/b".into(),
            summary: "delete c/b".into(),
            reversible: true,
        }
    }

    #[tokio::test]
    async fn assume_yes_allows() {
        let p = CliPolicy::new(ConfirmMode::Ask, true);
        assert_eq!(
            p.confirm(&action(), &ActionContext::default()).await,
            PolicyDecision::Allow
        );
    }

    #[tokio::test]
    async fn yes_mode_allows() {
        let p = CliPolicy::new(ConfirmMode::Yes, false);
        assert_eq!(
            p.confirm(&action(), &ActionContext::default()).await,
            PolicyDecision::Allow
        );
    }

    #[tokio::test]
    async fn auto_mode_denies() {
        let p = CliPolicy::new(ConfirmMode::Auto, false);
        assert!(matches!(
            p.confirm(&action(), &ActionContext::default()).await,
            PolicyDecision::Deny(_)
        ));
    }
}
