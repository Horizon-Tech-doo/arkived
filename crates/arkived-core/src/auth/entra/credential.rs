//! `EntraTokenCredential` — our implementation of `azure_core::credentials::TokenCredential`
//! backed by an access token obtained via the device-code flow.
//!
//! Implements automatic token refresh on expiry when a refresh context is attached.

use async_trait::async_trait;
use azure_core::credentials::{AccessToken, Secret, TokenCredential, TokenRequestOptions};
use std::sync::Mutex;
use time::OffsetDateTime;

/// Access token + refresh token + expiry + optional refresh context.
#[derive(Debug, Clone)]
pub struct TokenBundle {
    /// Bearer access token.
    pub access_token: String,
    /// Refresh token (present if Entra returned one).
    pub refresh_token: Option<String>,
    /// Unix timestamp after which the access token is no longer valid.
    pub expires_at: OffsetDateTime,
    /// Context used to perform refreshes. `None` in tests / pre-config.
    pub refresh_context: Option<RefreshContext>,
}

/// `TokenCredential` impl — holds an access token and can refresh automatically on expiry.
#[derive(Debug)]
pub struct EntraTokenCredential {
    inner: Mutex<TokenBundle>,
}

impl EntraTokenCredential {
    /// Construct from a token bundle.
    pub fn new(bundle: TokenBundle) -> Self {
        Self {
            inner: Mutex::new(bundle),
        }
    }

    /// Access the current bundle (clone of).
    pub fn snapshot(&self) -> TokenBundle {
        self.inner.lock().unwrap().clone()
    }

    /// Replace the bundle (used after a refresh).
    pub(crate) fn replace(&self, new_bundle: TokenBundle) {
        *self.inner.lock().unwrap() = new_bundle;
    }

    /// Attach refresh context so subsequent `get_token` calls can refresh automatically.
    pub fn with_refresh_context(self, ctx: RefreshContext) -> Self {
        {
            let mut guard = self.inner.lock().unwrap();
            guard.refresh_context = Some(ctx);
        }
        self
    }
}

/// Data needed to request a token refresh.
#[derive(Debug, Clone)]
pub struct RefreshContext {
    /// HTTP client used for the refresh request.
    pub client: reqwest::Client,
    /// Entra tenant (`common`, `organizations`, or a specific tenant id).
    pub tenant: String,
    /// Public client id.
    pub client_id: String,
    /// Scope to request on refresh (normally `https://storage.azure.com/.default`).
    pub scope: String,
}

use crate::auth::entra::device_code::{refresh_access_token, TokenResponse};

/// Seconds before expiry to proactively refresh.
const REFRESH_LEAD_SECONDS: i64 = 300;

#[async_trait]
impl TokenCredential for EntraTokenCredential {
    async fn get_token(
        &self,
        _scopes: &[&str],
        _options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        let (snapshot, needs_refresh) = {
            let guard = self.inner.lock().unwrap();
            let now = OffsetDateTime::now_utc();
            let needs = guard.expires_at <= now + time::Duration::seconds(REFRESH_LEAD_SECONDS);
            (guard.clone(), needs)
        };

        if !needs_refresh || snapshot.refresh_context.is_none() || snapshot.refresh_token.is_none()
        {
            return Ok(AccessToken::new(
                Secret::new(snapshot.access_token),
                snapshot.expires_at,
            ));
        }

        let ctx = snapshot.refresh_context.unwrap();
        let refresh_token = snapshot.refresh_token.unwrap();

        let response: TokenResponse = refresh_access_token(
            &ctx.client,
            &ctx.tenant,
            &ctx.client_id,
            &refresh_token,
            &ctx.scope,
        )
        .await
        .map_err(|e| {
            azure_core::Error::with_message(
                azure_core::error::ErrorKind::Credential,
                format!("refresh failed: {e}"),
            )
        })?;

        let new_expiry =
            OffsetDateTime::now_utc() + time::Duration::seconds(response.expires_in as i64);
        let new_bundle = TokenBundle {
            access_token: response.access_token.clone(),
            refresh_token: response.refresh_token.or(Some(refresh_token.clone())),
            expires_at: new_expiry,
            refresh_context: Some(ctx),
        };
        self.replace(new_bundle);

        Ok(AccessToken::new(
            Secret::new(response.access_token),
            new_expiry,
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
            refresh_context: None,
        }
    }

    #[tokio::test]
    async fn get_token_returns_stored_access_token() {
        let c = EntraTokenCredential::new(bundle("AT-XYZ", 60));
        let tok = c
            .get_token(&["https://storage.azure.com/.default"], None)
            .await
            .unwrap();
        assert_eq!(tok.token.secret(), "AT-XYZ");
    }

    #[test]
    fn replace_updates_snapshot() {
        let c = EntraTokenCredential::new(bundle("old", 10));
        c.replace(bundle("new", 60));
        assert_eq!(c.snapshot().access_token, "new");
    }
}
