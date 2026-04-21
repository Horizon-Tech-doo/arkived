//! XML DTOs for Azure Blob REST responses.
//!
//! These are the wire-level types; they're converted to the public
//! [`Container`](crate::backend::Container) / [`BlobEntry`](crate::backend::BlobEntry)
//! types in each op module.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ListContainersResult {
    #[serde(rename = "Containers", default)]
    pub containers: ContainerList,
    #[serde(rename = "NextMarker", default)]
    pub next_marker: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ContainerList {
    #[serde(rename = "Container", default)]
    pub items: Vec<XmlContainer>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlContainer {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Properties", default)]
    pub properties: ContainerProperties,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ContainerProperties {
    #[serde(rename = "Last-Modified", default)]
    pub last_modified: Option<String>,
    #[serde(rename = "Etag", default)]
    pub etag: Option<String>,
    #[serde(rename = "LeaseStatus", default)]
    pub lease_status: Option<String>,
    #[serde(rename = "LeaseState", default)]
    pub lease_state: Option<String>,
    #[serde(rename = "PublicAccess", default)]
    pub public_access: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListBlobsResult {
    #[serde(rename = "Prefix", default)]
    pub prefix: Option<String>,
    #[serde(rename = "Blobs", default)]
    pub blobs: BlobList,
    #[serde(rename = "NextMarker", default)]
    pub next_marker: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct BlobList {
    #[serde(rename = "Blob", default)]
    pub items: Vec<XmlBlob>,
    #[serde(rename = "BlobPrefix", default)]
    pub prefixes: Vec<XmlBlobPrefix>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlBlob {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Properties", default)]
    pub properties: BlobProperties,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct BlobProperties {
    #[serde(rename = "Last-Modified", default)]
    pub last_modified: Option<String>,
    #[serde(rename = "Etag", default)]
    pub etag: Option<String>,
    #[serde(rename = "Content-Length", default)]
    pub content_length: Option<u64>,
    #[serde(rename = "Content-Type", default)]
    pub content_type: Option<String>,
    #[serde(rename = "BlobType", default)]
    pub blob_type: Option<String>,
    #[serde(rename = "AccessTier", default)]
    pub access_tier: Option<String>,
    #[serde(rename = "LeaseState", default)]
    pub lease_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct XmlBlobPrefix {
    #[serde(rename = "Name")]
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::azure::xml::parse_xml;

    const LIST_CONTAINERS: &str = r#"<?xml version="1.0"?>
<EnumerationResults>
  <Containers>
    <Container>
      <Name>alpha</Name>
      <Properties>
        <Last-Modified>Mon, 21 Apr 2026 12:00:00 GMT</Last-Modified>
        <Etag>0x8DC</Etag>
        <LeaseStatus>unlocked</LeaseStatus>
        <LeaseState>available</LeaseState>
      </Properties>
    </Container>
    <Container>
      <Name>beta</Name>
      <Properties>
        <PublicAccess>blob</PublicAccess>
      </Properties>
    </Container>
  </Containers>
  <NextMarker/>
</EnumerationResults>"#;

    #[test]
    fn parse_list_containers() {
        let r: ListContainersResult = parse_xml(LIST_CONTAINERS).unwrap();
        assert_eq!(r.containers.items.len(), 2);
        assert_eq!(r.containers.items[0].name, "alpha");
        assert_eq!(
            r.containers.items[0].properties.lease_state.as_deref(),
            Some("available")
        );
        assert_eq!(
            r.containers.items[1].properties.public_access.as_deref(),
            Some("blob")
        );
    }

    const LIST_BLOBS: &str = r#"<?xml version="1.0"?>
<EnumerationResults ContainerName="device-twins">
  <Prefix>sync/</Prefix>
  <Blobs>
    <Blob>
      <Name>sync/part-00001.parquet</Name>
      <Properties>
        <Last-Modified>Mon, 21 Apr 2026 11:11:42 GMT</Last-Modified>
        <Etag>0x8DC7A9F</Etag>
        <Content-Length>14221000</Content-Length>
        <Content-Type>application/octet-stream</Content-Type>
        <BlobType>BlockBlob</BlobType>
        <AccessTier>Hot</AccessTier>
        <LeaseState>available</LeaseState>
      </Properties>
    </Blob>
    <BlobPrefix><Name>sync/2026-04/</Name></BlobPrefix>
  </Blobs>
  <NextMarker>AAAA</NextMarker>
</EnumerationResults>"#;

    #[test]
    fn parse_list_blobs() {
        let r: ListBlobsResult = parse_xml(LIST_BLOBS).unwrap();
        assert_eq!(r.blobs.items.len(), 1);
        assert_eq!(r.blobs.prefixes.len(), 1);
        assert_eq!(r.blobs.items[0].properties.content_length, Some(14221000));
        assert_eq!(r.blobs.prefixes[0].name, "sync/2026-04/");
        assert_eq!(r.next_marker.as_deref(), Some("AAAA"));
    }
}
