//! `PUT /{container}/{blob}` with `x-ms-copy-source` -- server-side blob copy.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::BlobPath;
use reqwest::Method;

impl AzureBlobBackend {
    /// Start a same-account or cross-account server-side blob copy.
    pub async fn copy_blob(&self, source_url: &str, destination: &BlobPath) -> crate::Result<()> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", destination.container, destination.blob));
        url.set_query(None);

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers: vec![("x-ms-copy-source".into(), source_url.into())],
                body: Body::Empty,
            })
            .await?;
        Ok(())
    }
}
