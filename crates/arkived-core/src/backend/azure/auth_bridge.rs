//! Apply a [`ResolvedCredential`] to an outgoing `reqwest::Request`.
//!
//! - **Entra**: add `Authorization: Bearer <token>`. Token acquired via
//!   the `TokenCredential` with scope `https://storage.azure.com/.default`.
//! - **SharedKey**: sign with HMAC-SHA256 via [`crate::auth::shared_key::sign`]
//!   and set `Authorization: SharedKey <account>:<sig>`. This needs to happen
//!   *after* headers are finalized, so this module exposes a post-build hook.
//! - **SAS**: merge the SAS query string into the URL. If both have a `sig=`
//!   the SAS wins.
//! - **Anonymous**: no-op.

use crate::auth::shared_key::{sign, SignRequest};
use crate::auth::ResolvedCredential;
use crate::Error;
use azure_core::credentials::TokenCredential;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use reqwest::{Request, Url};
use std::sync::Arc;

/// `x-ms-version` header value — pinned to a stable REST API version.
pub(crate) const MS_VERSION: &str = "2022-11-02";

/// Decorate the URL for SAS credentials (append the SAS query). Called
/// *before* the request is built so subsequent signing sees the final URL.
pub(crate) fn decorate_url(cred: &ResolvedCredential, url: &mut Url) {
    if let ResolvedCredential::Sas(sas) = cred {
        use secrecy::ExposeSecret;
        let sas_str = sas.expose_secret();
        // Merge SAS query params into the URL, preserving any that already exist.
        let existing = url.query().map(String::from);
        let merged = match existing {
            Some(q) if !q.is_empty() => format!("{q}&{sas_str}"),
            _ => sas_str.to_string(),
        };
        url.set_query(Some(&merged));
    }
}

/// Apply auth to a built request. This is the single choke point.
///
/// Sets `x-ms-date` and `x-ms-version` for all methods. For SharedKey,
/// signs after all headers are set. For Entra, acquires a token and
/// sets the Bearer header.
pub(crate) async fn apply_auth(
    cred: &ResolvedCredential,
    request: &mut Request,
) -> crate::Result<()> {
    // Always set the date + API version headers.
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    request.headers_mut().insert(
        HeaderName::from_static("x-ms-date"),
        HeaderValue::from_str(&date).map_err(|e| Error::Backend(format!("x-ms-date: {e}")))?,
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-ms-version"),
        HeaderValue::from_static(MS_VERSION),
    );

    match cred {
        ResolvedCredential::Anonymous | ResolvedCredential::Sas(_) => Ok(()),
        ResolvedCredential::Entra(tc) => apply_entra(tc.clone(), request).await,
        ResolvedCredential::SharedKey { account_name, key } => {
            apply_shared_key(account_name, key, request)
        }
    }
}

async fn apply_entra(
    tc: Arc<dyn TokenCredential>,
    request: &mut Request,
) -> crate::Result<()> {
    let token = tc
        .get_token(&["https://storage.azure.com/.default"], None)
        .await
        .map_err(|e| Error::AuthFailed(format!("get_token: {e}")))?;
    let header = format!("Bearer {}", token.token.secret());
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&header).map_err(|e| Error::AuthFailed(format!("bearer header: {e}")))?,
    );
    Ok(())
}

fn apply_shared_key(
    account_name: &str,
    key: &secrecy::SecretString,
    request: &mut Request,
) -> crate::Result<()> {
    let method = request.method().as_str().to_string();
    let url = request.url().clone();

    let header_pairs: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    let signed = sign(
        account_name,
        key,
        &SignRequest {
            method: &method,
            url: &url,
            headers: &header_pairs,
        },
    )
    .map_err(|e| Error::AuthFailed(format!("SharedKey sign: {e:?}")))?;

    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&signed).map_err(|e| Error::AuthFailed(format!("auth header: {e}")))?,
    );
    Ok(())
}

/// Copy a reqwest `HeaderMap` into the `Vec<(String, String)>` shape the
/// signer consumes. Exposed for testing.
#[cfg(test)]
pub(crate) fn headers_as_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn build_get(url: &str) -> Request {
        reqwest::Client::new().get(url).build().unwrap()
    }

    #[tokio::test]
    async fn anonymous_sets_date_and_version_only() {
        let mut req = build_get("https://example.com/container");
        apply_auth(&ResolvedCredential::Anonymous, &mut req).await.unwrap();
        assert!(req.headers().contains_key("x-ms-date"));
        assert_eq!(
            req.headers().get("x-ms-version").unwrap(),
            MS_VERSION
        );
        assert!(!req.headers().contains_key(AUTHORIZATION));
    }

    #[tokio::test]
    async fn shared_key_sets_signed_authorization() {
        let mut req = build_get("https://acme.blob.core.windows.net/?comp=list");
        apply_auth(
            &ResolvedCredential::SharedKey {
                account_name: "acme".into(),
                key: SecretString::new(
                    "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==".into(),
                ),
            },
            &mut req,
        )
        .await
        .unwrap();
        let auth = req.headers().get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(auth.starts_with("SharedKey acme:"));
    }

    #[test]
    fn sas_is_merged_into_url_query() {
        let mut url = Url::parse("https://acme.blob.core.windows.net/c/b").unwrap();
        let cred = ResolvedCredential::Sas(SecretString::new("sv=2022&sig=XYZ".into()));
        decorate_url(&cred, &mut url);
        assert_eq!(url.query(), Some("sv=2022&sig=XYZ"));

        let mut url2 = Url::parse("https://acme.blob.core.windows.net/c?existing=1").unwrap();
        decorate_url(&cred, &mut url2);
        assert_eq!(url2.query(), Some("existing=1&sv=2022&sig=XYZ"));
    }

    #[test]
    fn sas_decoration_is_noop_for_non_sas() {
        let mut url = Url::parse("https://acme.blob.core.windows.net/c/b").unwrap();
        decorate_url(&ResolvedCredential::Anonymous, &mut url);
        assert_eq!(url.query(), None);
    }

    #[test]
    fn headers_as_pairs_roundtrips() {
        let mut h = HeaderMap::new();
        h.insert("x-ms-date", "Mon".parse().unwrap());
        h.insert("x-ms-version", "2022-11-02".parse().unwrap());
        let pairs = headers_as_pairs(&h);
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().any(|(k, v)| k == "x-ms-date" && v == "Mon"));
    }
}
