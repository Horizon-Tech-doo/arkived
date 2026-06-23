//! Blob index tags — `GET/PUT {blob}?comp=tags`.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::xml::parse_xml;
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::BlobPath;
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use bytes::Bytes;
use reqwest::Method;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default)]
struct TagsResult {
    #[serde(rename = "TagSet", default)]
    tag_set: TagSet,
}

#[derive(Deserialize, Default)]
struct TagSet {
    #[serde(rename = "Tag", default)]
    tags: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct TagEntry {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Value")]
    value: String,
}

/// Escape a string for inclusion in XML character data.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

impl AzureBlobBackend {
    /// Read a blob's index tags (read-only).
    pub async fn get_tags(&self, path: &BlobPath) -> crate::Result<HashMap<String, String>> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=tags"));

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::GET,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await?;
        let body = resp
            .text()
            .await
            .map_err(|e| Error::Backend(format!("read tags body: {e}")))?;
        let parsed: TagsResult = parse_xml(&body)?;
        Ok(parsed
            .tag_set
            .tags
            .into_iter()
            .map(|t| (t.key, t.value))
            .collect())
    }

    /// Replace a blob's index tags. Policy-gated (it overwrites all existing tags).
    pub async fn set_tags(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        tags: &HashMap<String, String>,
    ) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "set_tags".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!(
                        "replace index tags of {}/{} ({} tags)",
                        path.container,
                        path.blob,
                        tags.len()
                    ),
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

        let mut body = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?><Tags><TagSet>");
        for (k, v) in tags {
            body.push_str(&format!(
                "<Tag><Key>{}</Key><Value>{}</Value></Tag>",
                xml_escape(k),
                xml_escape(v)
            ));
        }
        body.push_str("</TagSet></Tags>");
        let bytes = Bytes::from(body.into_bytes());

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=tags"));

        let headers = vec![
            ("content-length".to_string(), bytes.len().to_string()),
            ("content-type".to_string(), "application/xml".to_string()),
        ];

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers,
                body: Body::Bytes(bytes),
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
    async fn get_tags_parses_tagset() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/c/file.txt?comp=tags")
            .with_status(200)
            .with_body(
                "<?xml version=\"1.0\"?><Tags><TagSet>\
                 <Tag><Key>env</Key><Value>prod</Value></Tag>\
                 <Tag><Key>team</Key><Value>arkived</Value></Tag>\
                 </TagSet></Tags>",
            )
            .create_async()
            .await;

        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let tags = backend
            .get_tags(&BlobPath::new("c", "file.txt"))
            .await
            .unwrap();
        assert_eq!(tags.get("env").map(String::as_str), Some("prod"));
        assert_eq!(tags.get("team").map(String::as_str), Some("arkived"));
        assert_eq!(tags.len(), 2);
    }

    #[tokio::test]
    async fn set_tags_denied_short_circuits_before_http() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));
        let mut tags = HashMap::new();
        tags.insert("k".to_string(), "v".to_string());
        let err = backend
            .set_tags(&ctx, &BlobPath::new("c", "b"), &tags)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[test]
    fn xml_escape_handles_specials() {
        assert_eq!(xml_escape("a&b<c>\"'"), "a&amp;b&lt;c&gt;&quot;&apos;");
    }
}
