//! Blob system properties — `HEAD {blob}` to read, `PUT {blob}?comp=properties`
//! to set (policy-gated).

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobPath, BlobProperties, BlobPropertiesUpdate};
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use reqwest::header::HeaderMap;
use reqwest::Method;

/// Read a header as an owned `String`, if present and valid UTF-8.
fn header(map: &HeaderMap, name: &str) -> Option<String> {
    map.get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

impl AzureBlobBackend {
    /// Read a blob's system (HTTP) properties via `HEAD`.
    pub async fn get_properties(&self, path: &BlobPath) -> crate::Result<BlobProperties> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(None);

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::HEAD,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await?;

        let h = resp.headers();
        Ok(BlobProperties {
            content_length: header(h, "content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            content_type: header(h, "content-type"),
            content_encoding: header(h, "content-encoding"),
            content_language: header(h, "content-language"),
            cache_control: header(h, "cache-control"),
            content_disposition: header(h, "content-disposition"),
            content_md5: header(h, "content-md5"),
            etag: header(h, "etag"),
            blob_type: header(h, "x-ms-blob-type"),
            access_tier: header(h, "x-ms-access-tier"),
            lease_state: header(h, "x-ms-lease-state"),
            lease_status: header(h, "x-ms-lease-status"),
        })
    }

    /// Replace a blob's system (HTTP) properties. Policy-gated (it mutates the
    /// blob and clears any property left unset — see [`BlobPropertiesUpdate`]).
    pub async fn set_properties(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        update: &BlobPropertiesUpdate,
    ) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "set_properties".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!("set properties of {}/{}", path.container, path.blob),
                    reversible: true,
                },
                &ActionContext {
                    item_count: Some(1),
                    ..Default::default()
                },
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=properties"));

        let mut headers = Vec::<(String, String)>::new();
        let mut set = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                headers.push((k.to_string(), v.clone()));
            }
        };
        set("x-ms-blob-content-type", &update.content_type);
        set("x-ms-blob-content-encoding", &update.content_encoding);
        set("x-ms-blob-content-language", &update.content_language);
        set("x-ms-blob-cache-control", &update.cache_control);
        set("x-ms-blob-content-disposition", &update.content_disposition);
        set("x-ms-blob-content-md5", &update.content_md5);

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers,
                body: Body::Empty,
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ResolvedCredential;
    use crate::policy::DenyAllPolicy;
    use crate::progress::NoopSink;
    use crate::types::{AuthKind, ResourceKind};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct FakeAuth;
    #[async_trait]
    impl crate::auth::AuthProvider for FakeAuth {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
        fn display_name(&self) -> &str {
            "fake"
        }
        async fn resolve(&self) -> crate::Result<ResolvedCredential> {
            Ok(ResolvedCredential::Anonymous)
        }
        fn supports(&self, _: ResourceKind) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn get_properties_parses_response_headers() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("HEAD", "/c/file.txt")
            .with_status(200)
            .with_header("Content-Length", "1234")
            .with_header("Content-Type", "application/json")
            .with_header("ETag", "\"0x8DABC\"")
            .with_header("x-ms-blob-type", "BlockBlob")
            .with_header("x-ms-access-tier", "Cool")
            .with_header("x-ms-lease-state", "available")
            .create_async()
            .await;

        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();

        let props = backend
            .get_properties(&BlobPath::new("c", "file.txt"))
            .await
            .unwrap();
        assert_eq!(props.content_length, 1234);
        assert_eq!(props.content_type.as_deref(), Some("application/json"));
        assert_eq!(props.etag.as_deref(), Some("\"0x8DABC\""));
        assert_eq!(props.blob_type.as_deref(), Some("BlockBlob"));
        assert_eq!(props.access_tier.as_deref(), Some("Cool"));
        assert_eq!(props.lease_state.as_deref(), Some("available"));
        assert_eq!(props.content_encoding, None);
    }

    #[tokio::test]
    async fn set_properties_denied_short_circuits_before_http() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));

        let err = backend
            .set_properties(
                &ctx,
                &BlobPath::new("c", "b"),
                &BlobPropertiesUpdate {
                    content_type: Some("text/plain".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }
}
