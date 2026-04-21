//! Shared context threaded through every backend call.
//!
//! Bundles the things every destructive operation needs: auth, policy,
//! progress sink, cancellation, and a request-id for tracing correlation.

use crate::auth::AuthProvider;
use crate::policy::Policy;
use crate::progress::{NoopSink, ProgressSink};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use uuid::Uuid;

/// Cooperative cancellation token. Set once; cloneable across tasks.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a new uncancelled token.
    pub fn new() -> Self {
        Self::default()
    }
    /// Signal cancellation. Safe to call from any thread.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// Context bundle for a single logical operation. Cheap to clone (all Arc/value).
#[derive(Clone)]
pub struct Ctx {
    /// Factory producing credentials for this operation.
    pub auth: Arc<dyn AuthProvider>,
    /// Confirmation gate for destructive actions.
    pub policy: Arc<dyn Policy>,
    /// Progress event sink.
    pub progress: Arc<dyn ProgressSink>,
    /// Cooperative cancel token.
    pub cancel: CancellationToken,
    /// Tracing correlation id.
    pub request_id: Uuid,
}

impl Ctx {
    /// Construct a new `Ctx` with a `NoopSink` progress sink and a fresh
    /// cancellation token.
    pub fn new(auth: Arc<dyn AuthProvider>, policy: Arc<dyn Policy>) -> Self {
        Self {
            auth,
            policy,
            progress: Arc::new(NoopSink),
            cancel: CancellationToken::new(),
            request_id: Uuid::new_v4(),
        }
    }

    /// Override the progress sink.
    pub fn with_progress(mut self, sink: Arc<dyn ProgressSink>) -> Self {
        self.progress = sink;
        self
    }

    /// Override the cancellation token (so multiple Ctx clones share cancel state).
    pub fn with_cancel(mut self, token: CancellationToken) -> Self {
        self.cancel = token;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Credential;
    use crate::policy::{Action, ActionContext, AllowAllPolicy, PolicyDecision};
    use crate::progress::MemorySink;
    use crate::types::{AuthKind, ResourceKind};
    use async_trait::async_trait;

    #[derive(Debug)]
    struct FakeCred;
    impl Credential for FakeCred {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
    }

    struct FakeAuth;
    #[async_trait]
    impl AuthProvider for FakeAuth {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
        fn display_name(&self) -> &str {
            "fake"
        }
        async fn credential(&self) -> crate::Result<Arc<dyn Credential>> {
            Ok(Arc::new(FakeCred))
        }
        fn supports(&self, _: ResourceKind) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn ctx_construction_and_allow_all_policy() {
        let auth: Arc<dyn AuthProvider> = Arc::new(FakeAuth);
        let policy: Arc<dyn Policy> = Arc::new(AllowAllPolicy);
        let sink = Arc::new(MemorySink::new());
        let ctx = Ctx::new(auth, policy).with_progress(sink.clone());

        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "test".into(),
                    target: "t".into(),
                    summary: "s".into(),
                    reversible: true,
                },
                &ActionContext::default(),
            )
            .await;
        assert_eq!(decision, PolicyDecision::Allow);

        ctx.progress
            .emit(crate::progress::ProgressEvent::Complete)
            .await;
        assert_eq!(sink.events().len(), 1);
    }

    #[test]
    fn cancellation_token_propagates_across_clones() {
        let tok = CancellationToken::new();
        assert!(!tok.is_cancelled());
        let clone = tok.clone();
        tok.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn request_ids_are_unique() {
        let auth: Arc<dyn AuthProvider> = Arc::new(FakeAuth);
        let policy: Arc<dyn Policy> = Arc::new(AllowAllPolicy);
        let a = Ctx::new(auth.clone(), policy.clone());
        let b = Ctx::new(auth, policy);
        assert_ne!(a.request_id, b.request_id);
    }
}
