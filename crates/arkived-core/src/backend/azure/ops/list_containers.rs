//! `GET /?comp=list` — list containers.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::models::ListContainersResult;
use crate::backend::azure::xml::parse_xml;
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{Container, Page};
use reqwest::Method;
use time::format_description::well_known::Rfc2822;
use time::OffsetDateTime;

impl AzureBlobBackend {
    /// GET /?comp=list — return a page of containers.
    pub async fn list_containers(
        &self,
        continuation: Option<String>,
    ) -> crate::Result<Page<Container>> {
        let mut url = self.endpoint.clone();
        // Service-level list: path is "/", query comp=list.
        url.set_path("/");
        let mut query = String::from("comp=list");
        if let Some(marker) = &continuation {
            query.push_str(&format!("&marker={}", urlencoding::encode(marker)));
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
            .map_err(|e| crate::Error::Backend(format!("read list_containers body: {e}")))?;
        let parsed: ListContainersResult = parse_xml(&body)?;

        let items: Vec<Container> = parsed
            .containers
            .items
            .into_iter()
            .map(|x| Container {
                name: x.name,
                last_modified: x
                    .properties
                    .last_modified
                    .as_deref()
                    .and_then(|s| OffsetDateTime::parse(s, &Rfc2822).ok()),
                etag: x.properties.etag,
                lease_status: x.properties.lease_status,
                lease_state: x.properties.lease_state,
                public_access: x.properties.public_access,
            })
            .collect();

        Ok(Page {
            items,
            continuation: parsed.next_marker.filter(|s| !s.is_empty()),
        })
    }
}
