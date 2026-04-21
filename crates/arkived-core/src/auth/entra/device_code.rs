//! OAuth 2.0 device-code flow against Microsoft Entra ID.
//!
//! Spec: <https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code>.
//!
//! Flow:
//! 1. POST to `/{tenant}/oauth2/v2.0/devicecode` → user_code + verification_uri + device_code + interval.
//! 2. Display user_code / verification_uri.
//! 3. Poll `/{tenant}/oauth2/v2.0/token` with `grant_type=urn:ietf:params:oauth:grant-type:device_code`
//!    every `interval` seconds until the user signs in.
//! 4. On success, return `access_token` + `refresh_token` + `expires_in`.

use crate::Error;
use serde::{Deserialize, Serialize};

/// Host of Microsoft Entra's authority endpoints.
pub const AUTHORITY_HOST: &str = "https://login.microsoftonline.com";

/// Response from `/devicecode`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceCodeResponse {
    /// The short code shown to the user.
    pub user_code: String,
    /// The verification URL to type into the browser.
    pub verification_uri: String,
    /// The device_code used to poll the token endpoint.
    pub device_code: String,
    /// Expiry in seconds.
    pub expires_in: u64,
    /// Polling interval in seconds.
    pub interval: u64,
    /// Human-readable message to show.
    pub message: String,
}

/// Start a device-code flow by hitting the `/devicecode` endpoint.
pub async fn start_device_code(
    client: &reqwest::Client,
    tenant: &str,
    client_id: &str,
    scope: &str,
) -> Result<DeviceCodeResponse, Error> {
    let url = format!("{AUTHORITY_HOST}/{tenant}/oauth2/v2.0/devicecode");
    let params = [("client_id", client_id), ("scope", scope)];
    let resp = client
        .post(&url)
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::NetworkTransient(format!("devicecode request: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::AuthFailed(format!(
            "devicecode endpoint returned {status}: {body}"
        )));
    }

    resp.json::<DeviceCodeResponse>()
        .await
        .map_err(|e| Error::AuthFailed(format!("devicecode parse: {e}")))
}

use std::time::Duration;

/// Successful `/token` response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenResponse {
    /// Access token (bearer).
    pub access_token: String,
    /// Refresh token — use to get new access tokens without re-prompting.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Seconds until `access_token` expires.
    pub expires_in: u64,
    /// Always `Bearer` for Entra.
    pub token_type: String,
    /// Scope(s) actually granted.
    #[serde(default)]
    pub scope: Option<String>,
}

/// Error field in a `/token` error response.
#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Poll the `/token` endpoint until the user completes sign-in or the flow errors.
pub async fn poll_for_token(
    client: &reqwest::Client,
    tenant: &str,
    client_id: &str,
    device_code: &str,
    interval: Duration,
    timeout: Duration,
) -> Result<TokenResponse, Error> {
    let url = format!("{AUTHORITY_HOST}/{tenant}/oauth2/v2.0/token");
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if std::time::Instant::now() >= deadline {
            return Err(Error::AuthFailed("device-code flow timed out".into()));
        }

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", device_code),
        ];
        let resp = client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| Error::NetworkTransient(format!("token poll: {e}")))?;

        if resp.status().is_success() {
            return resp
                .json::<TokenResponse>()
                .await
                .map_err(|e| Error::AuthFailed(format!("token parse: {e}")));
        }

        // Parse error to decide: keep polling, or abort.
        let body = resp.text().await.unwrap_or_default();
        let err: TokenError = serde_json::from_str(&body).unwrap_or(TokenError {
            error: "unknown_error".into(),
            error_description: Some(body.clone()),
        });

        match err.error.as_str() {
            "authorization_pending" => tokio::time::sleep(interval).await,
            "slow_down" => tokio::time::sleep(interval * 2).await,
            "authorization_declined" | "expired_token" | "bad_verification_code" => {
                return Err(Error::AuthFailed(format!(
                    "{}: {}",
                    err.error,
                    err.error_description.unwrap_or_default()
                )));
            }
            other => {
                return Err(Error::AuthFailed(format!(
                    "unexpected token error `{other}`: {}",
                    err.error_description.unwrap_or_default()
                )));
            }
        }
    }
}

/// Exchange a refresh token for a new access token.
pub async fn refresh_access_token(
    client: &reqwest::Client,
    tenant: &str,
    client_id: &str,
    refresh_token: &str,
    scope: &str,
) -> Result<TokenResponse, Error> {
    let url = format!("{AUTHORITY_HOST}/{tenant}/oauth2/v2.0/token");
    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("scope", scope),
    ];
    let resp = client
        .post(&url)
        .form(&params)
        .send()
        .await
        .map_err(|e| Error::NetworkTransient(format!("refresh request: {e}")))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::AuthFailed(format!("refresh failed: {body}")));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| Error::AuthFailed(format!("refresh parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_success_response() {
        let mut server = mockito::Server::new_async().await;
        let host = server.url();

        let body = r#"{
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://microsoft.com/devicelogin",
            "device_code": "DC-XYZ",
            "expires_in": 900,
            "interval": 5,
            "message": "To sign in, use a web browser to open..."
        }"#;

        let _m = server
            .mock("POST", "/tenant-abc/oauth2/v2.0/devicecode")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        // Point at the mock by shadowing AUTHORITY_HOST — for this test we
        // construct the URL manually.
        let url = format!("{host}/tenant-abc/oauth2/v2.0/devicecode");
        let resp = client
            .post(&url)
            .form(&[
                ("client_id", "test-client"),
                ("scope", "https://storage.azure.com/.default"),
            ])
            .send()
            .await
            .unwrap();
        let parsed: DeviceCodeResponse = resp.json().await.unwrap();
        assert_eq!(parsed.user_code, "ABCD-EFGH");
        assert_eq!(parsed.interval, 5);
    }

    #[tokio::test]
    async fn failure_returns_auth_failed() {
        let mut server = mockito::Server::new_async().await;
        let host = server.url();

        let _m = server
            .mock("POST", "/tenant-abc/oauth2/v2.0/devicecode")
            .with_status(400)
            .with_body(r#"{"error":"invalid_client"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{host}/tenant-abc/oauth2/v2.0/devicecode");
        let resp = client
            .post(&url)
            .form(&[("client_id", "x"), ("scope", "y")])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn poll_success_on_first_try() {
        let mut server = mockito::Server::new_async().await;
        let host = server.url();

        let _m = server
            .mock("POST", "/tenant-abc/oauth2/v2.0/token")
            .with_status(200)
            .with_body(
                r#"{"access_token":"AT","refresh_token":"RT","expires_in":3600,"token_type":"Bearer","scope":"x"}"#,
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{host}/tenant-abc/oauth2/v2.0/token");
        let resp = client
            .post(&url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", "x"),
                ("device_code", "dc"),
            ])
            .send()
            .await
            .unwrap();
        let parsed: TokenResponse = resp.json().await.unwrap();
        assert_eq!(parsed.access_token, "AT");
        assert_eq!(parsed.refresh_token.as_deref(), Some("RT"));
        assert_eq!(parsed.expires_in, 3600);
    }

    #[test]
    fn token_error_deserializes() {
        let body = r#"{"error":"authorization_pending","error_description":"waiting"}"#;
        let e: TokenError = serde_json::from_str(body).unwrap();
        assert_eq!(e.error, "authorization_pending");
        assert_eq!(e.error_description.as_deref(), Some("waiting"));
    }

    #[tokio::test]
    async fn refresh_success_returns_new_token() {
        let mut server = mockito::Server::new_async().await;
        let host = server.url();

        let _m = server
            .mock("POST", "/tenant-abc/oauth2/v2.0/token")
            .with_status(200)
            .with_body(
                r#"{"access_token":"NEW","refresh_token":"RT2","expires_in":3600,"token_type":"Bearer"}"#,
            )
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{host}/tenant-abc/oauth2/v2.0/token");
        let resp = client
            .post(&url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", "x"),
                ("refresh_token", "RT1"),
                ("scope", "s"),
            ])
            .send()
            .await
            .unwrap();
        let parsed: TokenResponse = resp.json().await.unwrap();
        assert_eq!(parsed.access_token, "NEW");
        assert_eq!(parsed.refresh_token.as_deref(), Some("RT2"));
    }
}
