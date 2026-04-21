//! `PUT /{container}/{blob}` — single-shot block-blob upload.
//!
//! Azure allows block blobs up to 5000 MiB via a single `PUT Blob` request.
//! For now we buffer the stream into memory before uploading. Chunked
//! (Put Block + Put Block List) flows are a Backend-Depth plan follow-up.
//!
//! **Policy gating:** calls `ctx.policy.confirm(...)` when the blob would be
//! overwritten.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobPath, ByteStream, WriteOpts, WriteResult};
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use bytes::{Bytes, BytesMut};
use futures::stream::StreamExt;
use reqwest::Method;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

impl AzureBlobBackend {
    /// Upload a block blob in one request.
    ///
    /// If `opts.overwrite` is false and the blob exists, returns
    /// [`Error::Conflict`]. If overwrite is true AND the blob exists, this
    /// method first calls `ctx.policy.confirm("overwrite_blob", ...)`.
    pub async fn write_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        body: ByteStream,
        opts: WriteOpts,
    ) -> crate::Result<WriteResult> {
        // Buffer the stream. For v0.1.0 we cap bodies at 256 MiB in memory;
        // larger uploads get the chunked flow in the depth plan.
        const MAX_INLINE_BYTES: usize = 256 * 1024 * 1024;
        let bytes = collect_bytes(body, MAX_INLINE_BYTES).await?;

        // Policy gate on potential overwrite.
        if opts.overwrite {
            let decision = ctx
                .policy
                .confirm(
                    &Action {
                        verb: "overwrite_blob".into(),
                        target: format!("{}/{}", path.container, path.blob),
                        summary: format!(
                            "overwrite {}/{} with {} bytes",
                            path.container,
                            path.blob,
                            bytes.len()
                        ),
                        reversible: false,
                    },
                    &ActionContext {
                        item_count: Some(1),
                        ..Default::default()
                    },
                )
                .await;
            match decision {
                PolicyDecision::Allow | PolicyDecision::AllowAlways { .. } => {}
                PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
            }
        }

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

async fn collect_bytes(mut stream: ByteStream, max: usize) -> crate::Result<Bytes> {
    let mut buf = BytesMut::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        if buf.len() + bytes.len() > max {
            return Err(Error::Backend(format!(
                "upload body exceeds {max} bytes (chunked upload lands in Backend-Depth plan)"
            )));
        }
        buf.extend_from_slice(&bytes);
    }
    Ok(buf.freeze())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn collect_bytes_returns_full_body() {
        let chunks: Vec<crate::Result<Bytes>> =
            vec![Ok(Bytes::from("hello ")), Ok(Bytes::from("world"))];
        let s = stream::iter(chunks).boxed();
        let body = collect_bytes(s, 1024).await.unwrap();
        assert_eq!(&body[..], b"hello world");
    }

    #[tokio::test]
    async fn collect_bytes_errors_above_limit() {
        let chunks: Vec<crate::Result<Bytes>> =
            vec![Ok(Bytes::from(vec![0u8; 600])), Ok(Bytes::from(vec![0u8; 600]))];
        let s = stream::iter(chunks).boxed();
        let err = collect_bytes(s, 1000).await.unwrap_err();
        assert!(matches!(err, Error::Backend(_)));
    }
}
