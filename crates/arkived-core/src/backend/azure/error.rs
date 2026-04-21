//! Azure REST error → `crate::Error` mapping.
//!
//! Azure returns errors as an XML body `<Error><Code>...</Code><Message>...</Message></Error>`
//! with a status code. We map well-known codes (`BlobNotFound`,
//! `ContainerNotFound`, `ConditionNotMet`, `ServerBusy`, …) to specific
//! `crate::Error` variants; everything else becomes `Error::Backend(msg)`.

use crate::Error;
use quick_xml::de::from_str;
use serde::Deserialize;
use std::time::Duration;

/// Parsed Azure error body.
#[derive(Debug, Deserialize)]
pub(crate) struct AzureError {
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

/// Map a non-success HTTP response to a `crate::Error`.
///
/// Takes the response status and body text and dispatches to the right
/// variant. Also reads the `x-ms-retry-after` header for `Throttled`.
pub(crate) fn map_rest_error(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &str,
) -> Error {
    // Parse body as Azure error; if that fails, fall through to Backend with the raw text.
    let parsed: Option<AzureError> = from_str(body).ok();
    let code = parsed
        .as_ref()
        .map(|e| e.code.as_str())
        .unwrap_or("Unknown");
    let message = parsed.as_ref().map(|e| e.message.as_str()).unwrap_or(body);

    match (status.as_u16(), code) {
        (404, _) => Error::NotFound {
            resource: format!("{code}: {message}"),
        },
        (403, _) => Error::AuthFailed(format!("{code}: {message}")),
        (401, _) => Error::AuthExpired,
        (409, "BlobAlreadyExists") | (412, _) => Error::Conflict {
            detail: format!("{code}: {message}"),
            etag: headers
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string()),
        },
        (503 | 429, _) => {
            let retry_after = headers
                .get("x-ms-retry-after")
                .or_else(|| headers.get("retry-after"))
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(3));
            Error::Throttled { retry_after }
        }
        (500..=599, _) => Error::Backend(format!("server error {status}: {code}: {message}")),
        _ => Error::Backend(format!("HTTP {status}: {code}: {message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::{header::HeaderMap, StatusCode};

    fn hdr(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    const BLOB_NOT_FOUND: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Error><Code>BlobNotFound</Code><Message>The specified blob does not exist.</Message></Error>"#;

    #[test]
    fn map_404_to_not_found() {
        let err = map_rest_error(StatusCode::NOT_FOUND, &hdr(&[]), BLOB_NOT_FOUND);
        assert!(matches!(err, Error::NotFound { .. }));
    }

    #[test]
    fn map_401_to_auth_expired() {
        let err = map_rest_error(StatusCode::UNAUTHORIZED, &hdr(&[]), "");
        assert!(matches!(err, Error::AuthExpired));
    }

    #[test]
    fn map_403_to_auth_failed() {
        let err = map_rest_error(StatusCode::FORBIDDEN, &hdr(&[]), BLOB_NOT_FOUND);
        assert!(matches!(err, Error::AuthFailed(_)));
    }

    #[test]
    fn map_503_to_throttled_with_retry_after() {
        let err = map_rest_error(
            StatusCode::SERVICE_UNAVAILABLE,
            &hdr(&[("x-ms-retry-after", "7")]),
            "",
        );
        match err {
            Error::Throttled { retry_after } => assert_eq!(retry_after, Duration::from_secs(7)),
            other => panic!("expected Throttled, got {other:?}"),
        }
    }

    #[test]
    fn map_412_to_conflict_with_etag() {
        let err = map_rest_error(
            StatusCode::PRECONDITION_FAILED,
            &hdr(&[("etag", "0x8DCABCDEF")]),
            r#"<Error><Code>ConditionNotMet</Code><Message>.</Message></Error>"#,
        );
        match err {
            Error::Conflict { etag, .. } => assert_eq!(etag.as_deref(), Some("0x8DCABCDEF")),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn malformed_body_becomes_backend_error() {
        let err = map_rest_error(StatusCode::IM_A_TEAPOT, &hdr(&[]), "not xml!");
        assert!(matches!(err, Error::Backend(_)));
    }
}
