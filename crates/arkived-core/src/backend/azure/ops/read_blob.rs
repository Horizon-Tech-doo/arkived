//! `GET /{container}/{blob}` — read a blob as a byte stream.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobPath, ByteStream, Range};
use crate::Error;
use futures::stream::{StreamExt, TryStreamExt};
use reqwest::Method;

impl AzureBlobBackend {
    /// Stream a blob's bytes. Supports HTTP ranged reads via `range`.
    pub async fn read_blob(
        &self,
        path: &BlobPath,
        range: Option<Range>,
    ) -> crate::Result<ByteStream> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(None);

        let mut headers = Vec::<(String, String)>::new();
        if let Some(r) = range {
            let range_header = match r.end {
                Some(end) => format!("bytes={}-{}", r.start, end),
                None => format!("bytes={}-", r.start),
            };
            headers.push(("x-ms-range".into(), range_header));
        }

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::GET,
                url,
                headers,
                body: Body::Empty,
            })
            .await?;

        let stream = resp
            .bytes_stream()
            .map_err(|e| Error::NetworkTransient(format!("read_blob stream: {e}")))
            .boxed();
        Ok(stream)
    }
}
