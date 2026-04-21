//! The reqwest-backed request pipeline.
//!
//! Every request to Azure flows through [`HttpPipeline::send`]:
//! 1. Clone the URL and apply SAS decoration.
//! 2. Build the request with caller-supplied method/headers/body.
//! 3. Apply auth (sets `x-ms-date`, `x-ms-version`, `Authorization`).
//! 4. Send; on success, hand back the response.
//! 5. On transient error (throttle / 5xx / network), retry with backoff
//!    via [`super::retry::with_retries`].
//! 6. On terminal error, convert to `crate::Error` via [`super::error::map_rest_error`].

use crate::auth::ResolvedCredential;
use crate::backend::azure::auth_bridge::{apply_auth, decorate_url};
use crate::backend::azure::error::map_rest_error;
use crate::backend::azure::retry::with_retries;
use crate::Error;
use bytes::Bytes;
use reqwest::{Method, Response};

/// Per-request body: unit for no-body, Bytes for inline, stream handled by caller.
#[derive(Debug, Clone)]
pub(crate) enum Body {
    /// No request body.
    Empty,
    /// Inline bytes body (content-length is set automatically).
    #[allow(dead_code)]
    Bytes(Bytes),
}

/// A single request template. Cloned across retries.
#[derive(Debug, Clone)]
pub(crate) struct RequestTemplate {
    pub method: Method,
    pub url: url::Url,
    /// Name-value header pairs to set on the request (in addition to auth).
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

/// Outbound pipeline.
pub(crate) struct HttpPipeline<'a> {
    pub http: &'a reqwest::Client,
    pub credential: &'a ResolvedCredential,
}

impl<'a> HttpPipeline<'a> {
    /// Send a request with auth + retry, returning the raw response on 2xx.
    pub async fn send(&self, tmpl: RequestTemplate) -> crate::Result<Response> {
        let request_id = uuid::Uuid::new_v4();
        let span = tracing::info_span!(
            "azure_request",
            request_id = %request_id,
            method = %tmpl.method,
            host = %tmpl.url.host_str().unwrap_or("?"),
            path = %tmpl.url.path(),
            auth = ?self.credential.kind(),
        );
        let _enter = span.enter();
        let started = std::time::Instant::now();
        let result = with_retries(|| async { self.send_once(tmpl.clone()).await }).await;
        match &result {
            Ok(resp) => {
                tracing::info!(
                    status = %resp.status(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "azure_request ok"
                );
            }
            Err(e) => {
                tracing::warn!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    error = %e,
                    "azure_request failed"
                );
            }
        }
        result
    }

    async fn send_once(&self, mut tmpl: RequestTemplate) -> crate::Result<Response> {
        decorate_url(self.credential, &mut tmpl.url);

        let mut builder = self
            .http
            .request(tmpl.method.clone(), tmpl.url.clone());
        for (k, v) in &tmpl.headers {
            builder = builder.header(k, v);
        }
        builder = match tmpl.body {
            Body::Empty => builder.header("content-length", "0"),
            Body::Bytes(b) => builder.body(b),
        };

        let mut request = builder
            .build()
            .map_err(|e| Error::Backend(format!("build request: {e}")))?;

        apply_auth(self.credential, &mut request).await?;

        let resp = self
            .http
            .execute(request)
            .await
            .map_err(|e| Error::NetworkTransient(format!("execute: {e}")))?;

        if resp.status().is_success() {
            return Ok(resp);
        }

        // Pull headers + body for error mapping.
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.text().await.unwrap_or_default();
        Err(map_rest_error(status, &headers, &body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ResolvedCredential;

    #[tokio::test]
    async fn sends_anonymous_get_returns_response_on_2xx() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/hello")
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;

        let http = reqwest::Client::new();
        let cred = ResolvedCredential::Anonymous;
        let pipeline = HttpPipeline { http: &http, credential: &cred };

        let url = url::Url::parse(&format!("{}/hello", server.url())).unwrap();
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::GET,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn maps_404_to_not_found() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/missing")
            .with_status(404)
            .with_body(
                r#"<?xml version="1.0"?><Error><Code>BlobNotFound</Code><Message>gone</Message></Error>"#,
            )
            .create_async()
            .await;

        let http = reqwest::Client::new();
        let cred = ResolvedCredential::Anonymous;
        let pipeline = HttpPipeline { http: &http, credential: &cred };
        let url = url::Url::parse(&format!("{}/missing", server.url())).unwrap();
        let err = pipeline
            .send(RequestTemplate {
                method: Method::GET,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }));
    }
}
