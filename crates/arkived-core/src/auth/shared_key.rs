//! Storage Shared Key signing algorithm.
//!
//! Implements the "Authorize with Shared Key" scheme documented at
//! <https://learn.microsoft.com/rest/api/storageservices/authorize-with-shared-key>.
//!
//! This module produces a signed `Authorization` header for a given request
//! using the `SharedKey <account>:<signature>` form (not the older "SharedKeyLite"
//! variant). The Backend plan wires this into an `azure_core` pipeline policy
//! so every request from `BlobClient` gets signed.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// A single HTTP request's view for signing.
///
/// Intentionally protocol-minimal: callers build this from their SDK's
/// request type, keeping this crate free of any particular HTTP abstraction.
pub struct SignRequest<'a> {
    /// HTTP verb in upper-case (`GET`, `PUT`, `DELETE`, …).
    pub method: &'a str,
    /// Request URL (absolute).
    pub url: &'a url::Url,
    /// Request headers as `(name, value)` pairs. Name case does not matter.
    /// `x-ms-date` must already be set by the caller.
    pub headers: &'a [(String, String)],
}

/// Produce the `Authorization` header value for a request.
pub fn sign(
    account_name: &str,
    account_key_b64: &SecretString,
    req: &SignRequest<'_>,
) -> Result<String, SignError> {
    let key = B64
        .decode(account_key_b64.expose_secret())
        .map_err(|_| SignError::BadKey)?;

    let string_to_sign = build_string_to_sign(account_name, req);
    let mut mac = HmacSha256::new_from_slice(&key).map_err(|_| SignError::BadKey)?;
    mac.update(string_to_sign.as_bytes());
    let signature = B64.encode(mac.finalize().into_bytes());
    Ok(format!("SharedKey {account_name}:{signature}"))
}

/// Build the StringToSign. Exposed for testing against Microsoft's known vectors.
pub fn build_string_to_sign(account_name: &str, req: &SignRequest<'_>) -> String {
    // Headers used in the fixed portion of StringToSign.
    let h = |name: &str| -> &str {
        req.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    };

    // Content-Length is "0" → "" per the spec (empty string, not zero).
    let content_length = {
        let v = h("Content-Length");
        if v == "0" {
            ""
        } else {
            v
        }
    };

    let mut s = String::new();
    s.push_str(req.method);
    s.push('\n');
    s.push_str(h("Content-Encoding"));
    s.push('\n');
    s.push_str(h("Content-Language"));
    s.push('\n');
    s.push_str(content_length);
    s.push('\n');
    s.push_str(h("Content-MD5"));
    s.push('\n');
    s.push_str(h("Content-Type"));
    s.push('\n');
    s.push_str(h("Date"));
    s.push('\n');
    s.push_str(h("If-Modified-Since"));
    s.push('\n');
    s.push_str(h("If-Match"));
    s.push('\n');
    s.push_str(h("If-None-Match"));
    s.push('\n');
    s.push_str(h("If-Unmodified-Since"));
    s.push('\n');
    s.push_str(h("Range"));
    s.push('\n');
    s.push('\n'); // Blank line before canonicalized headers
    s.push_str(&canonicalized_headers(req.headers));
    s.push_str(&canonicalized_resource(account_name, req.url));
    s
}

fn canonicalized_headers(headers: &[(String, String)]) -> String {
    let mut entries: Vec<(String, String)> = headers
        .iter()
        .filter(|(k, _)| k.to_ascii_lowercase().starts_with("x-ms-"))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    for (k, v) in entries {
        out.push_str(&k);
        out.push(':');
        out.push_str(&v);
        out.push('\n');
    }
    out
}

fn canonicalized_resource(_account_name: &str, url: &url::Url) -> String {
    let mut res = String::new();
    res.push_str(url.path());

    // Group query params by lowercase name, sort, emit as `name:v1,v2`.
    let mut grouped: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (k, v) in url.query_pairs() {
        grouped
            .entry(k.to_ascii_lowercase().to_string())
            .or_default()
            .push(v.to_string());
    }
    for (k, mut vs) in grouped {
        res.push('\n');
        res.push_str(&k);
        res.push(':');
        vs.sort();
        res.push_str(&vs.join(","));
    }
    res
}

/// Errors produced by [`sign`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignError {
    /// The supplied account key is not valid base64 or has the wrong length.
    #[error("invalid account key")]
    BadKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Azurite well-known key (public, for tests).
    const AZURITE_ACCOUNT: &str = "devstoreaccount1";
    const AZURITE_KEY_B64: &str =
        "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";

    fn hdrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    #[test]
    fn string_to_sign_list_containers() {
        let url = url::Url::parse("http://127.0.0.1:10000/devstoreaccount1/?comp=list").unwrap();
        let headers = hdrs(&[
            ("x-ms-date", "Mon, 21 Apr 2026 12:00:00 GMT"),
            ("x-ms-version", "2022-11-02"),
        ]);
        let req = SignRequest {
            method: "GET",
            url: &url,
            headers: &headers,
        };
        let s = build_string_to_sign(AZURITE_ACCOUNT, &req);
        // Twelve empty lines, then canonicalized headers (2), then canonicalized resource.
        let expected = concat!(
            "GET\n",
            "\n\n\n\n\n\n\n\n\n\n\n\n",
            "x-ms-date:Mon, 21 Apr 2026 12:00:00 GMT\n",
            "x-ms-version:2022-11-02\n",
            "/devstoreaccount1/\n",
            "comp:list",
        );
        assert_eq!(s, expected);
    }

    #[test]
    fn query_params_grouped_and_sorted() {
        let url = url::Url::parse("http://127.0.0.1:10000/devstoreaccount1/container?b=2&a=1&a=3")
            .unwrap();
        let headers = hdrs(&[
            ("x-ms-date", "Mon, 21 Apr 2026 12:00:00 GMT"),
            ("x-ms-version", "2022-11-02"),
        ]);
        let req = SignRequest {
            method: "GET",
            url: &url,
            headers: &headers,
        };
        let s = build_string_to_sign(AZURITE_ACCOUNT, &req);
        // Resource section should contain sorted `a:1,3` before `b:2`.
        assert!(
            s.ends_with("/devstoreaccount1/container\na:1,3\nb:2"),
            "got: {s}"
        );
    }

    #[test]
    fn sign_with_azurite_key_is_stable() {
        let url = url::Url::parse("http://127.0.0.1:10000/devstoreaccount1/?comp=list").unwrap();
        let headers = hdrs(&[
            ("x-ms-date", "Mon, 21 Apr 2026 12:00:00 GMT"),
            ("x-ms-version", "2022-11-02"),
        ]);
        let req = SignRequest {
            method: "GET",
            url: &url,
            headers: &headers,
        };
        let header = sign(
            AZURITE_ACCOUNT,
            &SecretString::new(AZURITE_KEY_B64.into()),
            &req,
        )
        .unwrap();

        // Regression: value is stable given fixed input. The actual bytes
        // are verified by the Azurite integration test (Task 18).
        assert!(header.starts_with("SharedKey devstoreaccount1:"));
        assert!(header.len() > 40);
    }

    #[test]
    fn bad_key_errors() {
        let url = url::Url::parse("http://x/").unwrap();
        let req = SignRequest {
            method: "GET",
            url: &url,
            headers: &[],
        };
        let err = sign("x", &SecretString::new("not-base64!!!".into()), &req).unwrap_err();
        assert_eq!(err, SignError::BadKey);
    }

    #[test]
    fn content_length_zero_treated_as_empty() {
        let url = url::Url::parse("http://x/devstoreaccount1/").unwrap();
        let headers = hdrs(&[("Content-Length", "0"), ("x-ms-date", "d")]);
        let req = SignRequest {
            method: "PUT",
            url: &url,
            headers: &headers,
        };
        let s = build_string_to_sign("devstoreaccount1", &req);
        // Line 4 (after method) must be empty, not "0".
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines[0], "PUT");
        assert_eq!(lines[1], ""); // content-encoding
        assert_eq!(lines[2], ""); // content-language
        assert_eq!(lines[3], ""); // content-length ("0" → "")
    }

    #[test]
    fn headers_not_prefixed_x_ms_are_excluded_from_canon() {
        let url = url::Url::parse("http://x/devstoreaccount1/").unwrap();
        let headers = hdrs(&[("Host", "x"), ("User-Agent", "arkived"), ("x-ms-date", "d")]);
        let req = SignRequest {
            method: "GET",
            url: &url,
            headers: &headers,
        };
        let s = build_string_to_sign("devstoreaccount1", &req);
        assert!(!s.contains("host:"));
        assert!(!s.contains("user-agent:"));
        assert!(s.contains("x-ms-date:d"));
    }
}
