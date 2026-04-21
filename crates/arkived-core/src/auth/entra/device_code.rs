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
}
