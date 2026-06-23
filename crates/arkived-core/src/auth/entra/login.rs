//! Interactive device-code sign-in that persists a [`SignIn`].
//!
//! Ties together the raw device-code endpoints, the refresh-token cache, and
//! the [`Store`] so a CLI/GUI can run one call to sign a user in and remember
//! them. The display of the verification URL + user code is delegated to the
//! caller via the `on_prompt` callback, so each front-end controls its own UX.

use crate::auth::credentials::CredentialStore;
use crate::auth::entra::cache::{CachedRefresh, RefreshCache};
use crate::auth::entra::claims::parse_identity_claims;
use crate::auth::entra::device_code::{poll_for_token, start_device_code, DeviceCodeResponse};
use crate::auth::entra::DEFAULT_CLIENT_ID;
use crate::store::sign_in::SignIn;
use crate::store::Store;
use crate::Error;
use std::time::Duration;
use time::OffsetDateTime;

/// Scope requesting a storage access token plus OIDC identity (`openid
/// profile`) and a refresh token (`offline_access`).
const LOGIN_SCOPE: &str = "https://storage.azure.com/.default offline_access openid profile";

/// Run an interactive device-code sign-in against `tenant` (`"organizations"`,
/// `"common"`, `"consumers"`, or a tenant id).
///
/// `on_prompt` is invoked once with the verification URL and user code so the
/// caller can show instructions; the call then blocks until the user completes
/// authentication or the flow times out. On success it persists a [`SignIn`],
/// makes it the current sign-in, caches the refresh token in `secrets`, and
/// returns the record.
pub async fn device_login<F>(
    store: &Store,
    secrets: &dyn CredentialStore,
    tenant: &str,
    on_prompt: F,
) -> Result<SignIn, Error>
where
    F: FnOnce(&DeviceCodeResponse),
{
    let http = reqwest::Client::new();
    let client_id = DEFAULT_CLIENT_ID;

    let dc = start_device_code(&http, tenant, client_id, LOGIN_SCOPE).await?;
    on_prompt(&dc);

    let token = poll_for_token(
        &http,
        tenant,
        client_id,
        &dc.device_code,
        Duration::from_secs(dc.interval),
        Duration::from_secs(dc.expires_in),
    )
    .await?;

    // Prefer identity claims from the id_token; fall back to the access token.
    let mut claims = token
        .id_token
        .as_deref()
        .map(parse_identity_claims)
        .unwrap_or_default();
    if claims.user_principal.is_none() {
        claims = parse_identity_claims(&token.access_token);
    }

    let tenant_id = claims
        .tenant_id
        .clone()
        .unwrap_or_else(|| tenant.to_string());
    let user_principal = claims
        .user_principal
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    // The object id is a stable per-user id; fall back to tenant:user.
    let sign_in_id = claims
        .object_id
        .clone()
        .unwrap_or_else(|| format!("{tenant_id}:{user_principal}"));

    if let Some(rt) = &token.refresh_token {
        RefreshCache::new(secrets).put(
            &sign_in_id,
            &CachedRefresh {
                refresh_token: rt.clone(),
                tenant: tenant_id.clone(),
                client_id: client_id.to_string(),
                scope: LOGIN_SCOPE.to_string(),
                obtained_at: OffsetDateTime::now_utc(),
            },
        )?;
    }

    let record = SignIn::now(
        &sign_in_id,
        &user_principal,
        &tenant_id,
        "azure",
        &user_principal,
    );
    // Re-signing in with the same identity refreshes the record.
    if store.sign_in_get(&sign_in_id)?.is_some() {
        store.sign_in_delete(&sign_in_id)?;
    }
    store.sign_in_insert(&record)?;
    store.context_set_sign_in(Some(&sign_in_id))?;
    Ok(record)
}
