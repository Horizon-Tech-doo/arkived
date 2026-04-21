//! `EntraTokenCredential` — our implementation of `azure_core::credentials::TokenCredential`
//! backed by an access token obtained via the device-code flow.
//!
//! Stage 1 implementation: holds a pre-obtained access token and returns it on
//! `get_token`. Refresh logic lands in Task 15.

use azure_core::credentials::{AccessToken, Secret, TokenCredential, TokenRequestOptions};
use async_trait::async_trait;
use std::sync::Mutex;
use time::OffsetDateTime;

/// Access token + refresh token + expiry.
#[derive(Debug, Clone)]
pub struct TokenBundle {
    /// Bearer access token.
    pub access_token: String,
    /// Refresh token (present if Entra returned one).
    pub refresh_token: Option<String>,
    /// Unix timestamp after which the access token is no longer valid.
    pub expires_at: OffsetDateTime,
}

/// `TokenCredential` impl — Stage 1: returns a stored access token.
#[derive(Debug)]
pub struct EntraTokenCredential {
    inner: Mutex<TokenBundle>,
}

impl EntraTokenCredential {
    /// Construct from a token bundle.
    pub fn new(bundle: TokenBundle) -> Self {
        Self { inner: Mutex::new(bundle) }
    }

    /// Access the current bundle (clone of).
    pub fn snapshot(&self) -> TokenBundle {
        self.inner.lock().unwrap().clone()
    }

    /// Replace the bundle (used after a refresh).
    pub(crate) fn replace(&self, new_bundle: TokenBundle) {
        *self.inner.lock().unwrap() = new_bundle;
    }
}

#[async_trait]
impl TokenCredential for EntraTokenCredential {
    async fn get_token(
        &self,
        _scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        let bundle = self.inner.lock().unwrap().clone();
        // Refresh-on-expiry lands in Task 15; for now return whatever we have.
        Ok(AccessToken::new(
            Secret::new(bundle.access_token),
            bundle.expires_at,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    fn bundle(at: &str, mins: i64) -> TokenBundle {
        TokenBundle {
            access_token: at.into(),
            refresh_token: Some("RT".into()),
            expires_at: OffsetDateTime::now_utc() + Duration::minutes(mins),
        }
    }

    #[tokio::test]
    async fn get_token_returns_stored_access_token() {
        let c = EntraTokenCredential::new(bundle("AT-XYZ", 60));
        let tok = c.get_token(&["https://storage.azure.com/.default"], None).await.unwrap();
        assert_eq!(tok.token.secret(), "AT-XYZ");
    }

    #[test]
    fn replace_updates_snapshot() {
        let c = EntraTokenCredential::new(bundle("old", 10));
        c.replace(bundle("new", 60));
        assert_eq!(c.snapshot().access_token, "new");
    }
}
