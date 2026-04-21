//! `GET /{container}?restype=container&comp=list` — list blobs in a container.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::models::ListBlobsResult;
use crate::backend::azure::xml::parse_xml;
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobEntry, Page};
use reqwest::Method;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

impl AzureBlobBackend {
    /// GET /{container}?restype=container&comp=list — return a page of blobs.
    ///
    /// - `prefix`: only list blobs whose name starts with this string.
    /// - `delimiter`: if `Some("/")`, returns virtual directory prefixes as
    ///   [`BlobEntry::Prefix`] entries instead of full blob names.
    /// - `continuation`: opaque marker from a prior call's
    ///   [`Page::continuation`].
    pub async fn list_blobs(
        &self,
        container: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        continuation: Option<String>,
    ) -> crate::Result<Page<BlobEntry>> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{container}"));
        let mut query = String::from("restype=container&comp=list");
        if let Some(p) = prefix {
            query.push_str(&format!("&prefix={}", urlencoding::encode(p)));
        }
        if let Some(d) = delimiter {
            query.push_str(&format!("&delimiter={}", urlencoding::encode(d)));
        }
        if let Some(m) = &continuation {
            query.push_str(&format!("&marker={}", urlencoding::encode(m)));
        }
        url.set_query(Some(&query));

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
            .map_err(|e| crate::Error::Backend(format!("read list_blobs body: {e}")))?;
        let parsed: ListBlobsResult = parse_xml(&body)?;

        let mut items: Vec<BlobEntry> = Vec::new();

        for b in parsed.blobs.items {
            items.push(BlobEntry::Blob {
                name: b.name,
                size: b.properties.content_length.unwrap_or(0),
                blob_type: b.properties.blob_type.unwrap_or_else(|| "BlockBlob".into()),
                tier: b.properties.access_tier,
                etag: b.properties.etag,
                content_type: b.properties.content_type,
                last_modified: b
                    .properties
                    .last_modified
                    .as_deref()
                    .and_then(|s| OffsetDateTime::parse(s, &Rfc2822).ok()),
                lease_state: b.properties.lease_state,
            });
        }
        for pfx in parsed.blobs.prefixes {
            items.push(BlobEntry::Prefix { name: pfx.name });
        }

        Ok(Page {
            items,
            continuation: parsed.next_marker.filter(|s| !s.is_empty()),
        })
    }
}
