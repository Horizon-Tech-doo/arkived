//! `PUT /{container}/{blob}` — block blob upload.
//!
//! Small streams use a single `Put Blob`; larger streams are staged with
//! `Put Block` and committed with `Put Block List` so callers can upload
//! without buffering the entire object in memory.
//!
//! **Policy gating:** calls `ctx.policy.confirm(...)` when the blob would be
//! overwritten.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobPath, ByteStream, WriteOpts, WriteResult};
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use bytes::{Bytes, BytesMut};
use futures::stream::StreamExt;
use reqwest::Method;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

const DEFAULT_BLOCK_SIZE: usize = 8 * 1024 * 1024;
const INLINE_UPLOAD_THRESHOLD: usize = DEFAULT_BLOCK_SIZE;
const MAX_BLOCKS: usize = 50_000;

impl AzureBlobBackend {
    /// Upload a block blob.
    ///
    /// If `opts.overwrite` is false and the blob exists, returns
    /// [`Error::Conflict`]. If overwrite is true, this method first calls
    /// `ctx.policy.confirm("overwrite_blob", ...)`.
    pub async fn write_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        mut body: ByteStream,
        opts: WriteOpts,
    ) -> crate::Result<WriteResult> {
        if opts.overwrite {
            let decision = ctx
                .policy
                .confirm(
                    &Action {
                        verb: "overwrite_blob".into(),
                        target: format!("{}/{}", path.container, path.blob),
                        summary: format!("overwrite {}/{}", path.container, path.blob),
                        reversible: false,
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
        }

        let block_size = normalized_block_size(opts.block_size)?;
        let mut buffered = BytesMut::new();
        let mut block_ids = Vec::new();
        let mut total_bytes = 0u64;

        while let Some(chunk) = body.next().await {
            let bytes = chunk?;
            total_bytes += bytes.len() as u64;

            if block_ids.is_empty() && buffered.len() + bytes.len() <= INLINE_UPLOAD_THRESHOLD {
                buffered.extend_from_slice(&bytes);
                continue;
            }

            if block_ids.is_empty() {
                stage_buffered_full_blocks(self, path, &mut block_ids, &mut buffered, block_size)
                    .await?;
            }
            append_and_stage_blocks(self, path, &mut block_ids, &mut buffered, bytes, block_size)
                .await?;
        }

        if block_ids.is_empty() {
            return self.put_blob(path, buffered.freeze(), &opts).await;
        }

        if !buffered.is_empty() {
            stage_block(self, path, &mut block_ids, buffered.freeze()).await?;
        }

        commit_block_list(self, path, &block_ids, total_bytes, &opts).await
    }

    async fn put_blob(
        &self,
        path: &BlobPath,
        bytes: Bytes,
        opts: &WriteOpts,
    ) -> crate::Result<WriteResult> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(None);

        let mut headers: Vec<(String, String)> = vec![
            ("x-ms-blob-type".into(), "BlockBlob".into()),
            ("content-length".into(), bytes.len().to_string()),
        ];
        if let Some(ct) = &opts.content_type {
            headers.push(("content-type".into(), ct.clone()));
        } else {
            headers.push(("content-type".into(), "application/octet-stream".into()));
        }
        if !opts.overwrite {
            headers.push(("if-none-match".into(), "*".into()));
        }
        if let Some(etag) = &opts.if_match {
            headers.push(("if-match".into(), etag.clone()));
        }
        for (k, v) in &opts.metadata {
            headers.push((format!("x-ms-meta-{k}"), v.clone()));
        }

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers,
                body: Body::Bytes(bytes),
            })
            .await?;

        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let last_modified = resp
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| OffsetDateTime::parse(s, &Rfc2822).ok());

        Ok(WriteResult {
            etag,
            last_modified,
            blob_type: "BlockBlob".into(),
        })
    }
}

async fn append_and_stage_blocks(
    backend: &AzureBlobBackend,
    path: &BlobPath,
    block_ids: &mut Vec<String>,
    buffered: &mut BytesMut,
    mut chunk: Bytes,
    block_size: usize,
) -> crate::Result<()> {
    if !buffered.is_empty() {
        let remaining = block_size - buffered.len();
        let take = remaining.min(chunk.len());
        let head = chunk.split_to(take);
        buffered.extend_from_slice(&head);
        if buffered.len() == block_size {
            let block = buffered.split_to(block_size).freeze();
            stage_block(backend, path, block_ids, block).await?;
        }
    }

    while chunk.len() >= block_size {
        let block = chunk.split_to(block_size);
        stage_block(backend, path, block_ids, block).await?;
    }

    if !chunk.is_empty() {
        buffered.extend_from_slice(&chunk);
    }

    Ok(())
}

async fn stage_buffered_full_blocks(
    backend: &AzureBlobBackend,
    path: &BlobPath,
    block_ids: &mut Vec<String>,
    buffered: &mut BytesMut,
    block_size: usize,
) -> crate::Result<()> {
    while buffered.len() >= block_size {
        let block = buffered.split_to(block_size).freeze();
        stage_block(backend, path, block_ids, block).await?;
    }
    Ok(())
}

async fn stage_block(
    backend: &AzureBlobBackend,
    path: &BlobPath,
    block_ids: &mut Vec<String>,
    bytes: Bytes,
) -> crate::Result<()> {
    if block_ids.len() >= MAX_BLOCKS {
        return Err(Error::Backend(format!(
            "upload exceeds Azure block blob limit of {MAX_BLOCKS} blocks"
        )));
    }

    let block_id = make_block_id(block_ids.len());
    let mut url = backend.endpoint.clone();
    url.set_path(&format!("/{}/{}", path.container, path.blob));
    url.query_pairs_mut()
        .append_pair("comp", "block")
        .append_pair("blockid", &block_id);

    let pipeline = HttpPipeline {
        http: &backend.http,
        credential: &backend.credential,
    };
    pipeline
        .send(RequestTemplate {
            method: Method::PUT,
            url,
            headers: vec![("content-length".into(), bytes.len().to_string())],
            body: Body::Bytes(bytes),
        })
        .await?;
    block_ids.push(block_id);
    Ok(())
}

async fn commit_block_list(
    backend: &AzureBlobBackend,
    path: &BlobPath,
    block_ids: &[String],
    total_bytes: u64,
    opts: &WriteOpts,
) -> crate::Result<WriteResult> {
    let xml = block_list_xml(block_ids);
    let bytes = Bytes::from(xml);
    let mut url = backend.endpoint.clone();
    url.set_path(&format!("/{}/{}", path.container, path.blob));
    url.set_query(Some("comp=blocklist"));

    let mut headers: Vec<(String, String)> = vec![
        ("content-length".into(), bytes.len().to_string()),
        ("content-type".into(), "application/xml".into()),
        (
            "x-ms-blob-content-type".into(),
            opts.content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into()),
        ),
    ];
    if !opts.overwrite {
        headers.push(("if-none-match".into(), "*".into()));
    }
    if let Some(etag) = &opts.if_match {
        headers.push(("if-match".into(), etag.clone()));
    }
    for (k, v) in &opts.metadata {
        headers.push((format!("x-ms-meta-{k}"), v.clone()));
    }

    let pipeline = HttpPipeline {
        http: &backend.http,
        credential: &backend.credential,
    };
    let resp = pipeline
        .send(RequestTemplate {
            method: Method::PUT,
            url,
            headers,
            body: Body::Bytes(bytes),
        })
        .await?;

    let etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let last_modified = resp
        .headers()
        .get("last-modified")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| OffsetDateTime::parse(s, &Rfc2822).ok());

    tracing::info!(
        block_count = block_ids.len(),
        total_bytes,
        "committed block blob upload"
    );

    Ok(WriteResult {
        etag,
        last_modified,
        blob_type: "BlockBlob".into(),
    })
}

fn normalized_block_size(value: Option<usize>) -> crate::Result<usize> {
    let block_size = value.unwrap_or(DEFAULT_BLOCK_SIZE);
    if block_size == 0 {
        return Err(Error::Backend(
            "upload block size must be greater than 0".into(),
        ));
    }
    Ok(block_size)
}

fn make_block_id(index: usize) -> String {
    B64.encode(format!("arkived-block-{index:08}"))
}

fn block_list_xml(block_ids: &[String]) -> String {
    let mut xml = String::from(r#"<?xml version="1.0" encoding="utf-8"?><BlockList>"#);
    for block_id in block_ids {
        xml.push_str("<Latest>");
        xml.push_str(block_id);
        xml.push_str("</Latest>");
    }
    xml.push_str("</BlockList>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_ids_have_stable_encoded_width() {
        assert_eq!(make_block_id(0).len(), make_block_id(49_999).len());
        assert_ne!(make_block_id(0), make_block_id(1));
    }

    #[test]
    fn block_list_xml_uses_latest_entries() {
        let xml = block_list_xml(&[make_block_id(0), make_block_id(1)]);
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="utf-8"?><BlockList>"#));
        assert_eq!(xml.matches("<Latest>").count(), 2);
        assert!(xml.ends_with("</BlockList>"));
    }

    use crate::auth::ResolvedCredential;
    use crate::backend::AzureBlobBackend;
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
    async fn overwrite_with_deny_all_policy_blocks_before_http() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));

        let chunks: Vec<crate::Result<Bytes>> = vec![Ok(Bytes::from("data"))];
        let stream = futures::stream::iter(chunks).boxed();

        let err = backend
            .write_blob(
                &ctx,
                &BlobPath::new("c", "b"),
                stream,
                WriteOpts {
                    overwrite: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn non_overwrite_write_does_not_invoke_policy() {
        // With overwrite=false we never call policy.confirm, so DenyAllPolicy
        // is irrelevant. This test only verifies the code path compiles and
        // runs up to the HTTP call (which will fail on the unreachable URL,
        // but that's a NetworkTransient rather than a PolicyDenied).
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));

        let chunks: Vec<crate::Result<Bytes>> = vec![Ok(Bytes::from("data"))];
        let stream = futures::stream::iter(chunks).boxed();

        let err = backend
            .write_blob(
                &ctx,
                &BlobPath::new("c", "b"),
                stream,
                WriteOpts {
                    overwrite: false,
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        // Should NOT be PolicyDenied — we skipped the policy check.
        assert!(!matches!(err, Error::PolicyDenied(_)));
    }
}
