//! MCP server for Arkived — exposes Azure Blob operations as MCP tools.
//!
//! Read-only tools (list/read/properties/metadata) run without confirmation.
//! Destructive or elevated tools (write/delete/copy/sas/set-tier) route through
//! an MCP **elicitation**: the server asks the connected client's human to
//! approve before the operation runs. If the client cannot elicit, or the human
//! declines, the operation is refused. This mirrors the `Policy` gate that the
//! CLI and desktop app enforce.

use arkived_core::auth::AzuriteEmulatorProvider;
use arkived_core::policy::AllowAllPolicy;
use arkived_core::{
    AzureBlobBackend, BlobEntry, BlobPath, ConnectionParts, Ctx, SasOptions, SasProtocol,
    SasResource, Tier, WriteOpts,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use bytes::Bytes;
use futures::StreamExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::ErrorData;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// The Arkived MCP server. Holds a connected backend shared across tool calls.
#[derive(Clone)]
pub struct ArkivedServer {
    backend: Arc<AzureBlobBackend>,
    tool_router: ToolRouter<Self>,
}

// ---- Tool argument types -------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
struct Empty {}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListBlobsArg {
    /// Container name.
    container: String,
    /// Optional blob-name prefix (virtual directory).
    prefix: Option<String>,
    /// List recursively (no virtual-directory grouping). Default false.
    recursive: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct BlobArg {
    /// Container name.
    container: String,
    /// Blob name (slash-delimited path allowed).
    blob: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WriteBlobArg {
    /// Container name.
    container: String,
    /// Blob name.
    blob: String,
    /// UTF-8 text content to upload.
    content: String,
    /// Optional Content-Type.
    content_type: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CopyBlobArg {
    /// Source container.
    source_container: String,
    /// Source blob.
    source_blob: String,
    /// Destination container.
    dest_container: String,
    /// Destination blob.
    dest_blob: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SasArg {
    /// Container name.
    container: String,
    /// Optional blob name; omit for a container-scoped SAS.
    blob: Option<String>,
    /// Permission letters (r,w,d,l,a,c). Default "r".
    permissions: Option<String>,
    /// Hours until the SAS expires. Default 1.
    expiry_hours: Option<i64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SetTagsArg {
    /// Container name.
    container: String,
    /// Blob name.
    blob: String,
    /// Index tags to set (replaces all existing tags).
    tags: HashMap<String, String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SetTierArg {
    /// Container name.
    container: String,
    /// Blob name.
    blob: String,
    /// Target tier: hot, cool, cold, or archive.
    tier: String,
}

/// Schema for the human-in-the-loop confirmation elicitation.
#[derive(Deserialize, serde::Serialize, schemars::JsonSchema)]
struct Confirm {
    /// Set to true to approve the destructive operation.
    confirm: bool,
}
rmcp::elicit_safe!(Confirm);

// ---- Tool result views ---------------------------------------------------

#[derive(Serialize, schemars::JsonSchema)]
struct ContainerView {
    name: String,
    public_access: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct BlobView {
    name: String,
    kind: String,
    size: Option<u64>,
    tier: Option<String>,
    blob_type: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ReadView {
    container: String,
    blob: String,
    size: u64,
    /// "utf8" if the content was valid UTF-8, otherwise "base64".
    encoding: String,
    content: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PropertiesView {
    content_length: u64,
    content_type: Option<String>,
    etag: Option<String>,
    blob_type: Option<String>,
    access_tier: Option<String>,
    lease_state: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct MetadataView {
    metadata: HashMap<String, String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct WriteResultView {
    container: String,
    blob: String,
    etag: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct OkView {
    ok: bool,
    detail: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SasView {
    url: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SnapshotView {
    snapshot: String,
}

#[tool_router]
impl ArkivedServer {
    /// Build a server from a connected backend.
    pub fn new(backend: AzureBlobBackend) -> Self {
        Self {
            backend: Arc::new(backend),
            tool_router: Self::tool_router(),
        }
    }

    // -- Read-only tools (no confirmation) --------------------------------

    #[tool(description = "List all containers in the storage account (read-only).")]
    async fn list_containers(
        &self,
        Parameters(Empty {}): Parameters<Empty>,
    ) -> Result<Json<Vec<ContainerView>>, ErrorData> {
        let mut out = Vec::new();
        let mut marker = None;
        loop {
            let page = self.backend.list_containers(marker).await.map_err(op_err)?;
            out.extend(page.items.into_iter().map(|c| ContainerView {
                name: c.name,
                public_access: c.public_access,
            }));
            match page.continuation {
                Some(m) => marker = Some(m),
                None => break,
            }
        }
        Ok(Json(out))
    }

    #[tool(description = "List blobs in a container, optionally under a prefix (read-only).")]
    async fn list_blobs(
        &self,
        Parameters(arg): Parameters<ListBlobsArg>,
    ) -> Result<Json<Vec<BlobView>>, ErrorData> {
        let delimiter = if arg.recursive.unwrap_or(false) {
            None
        } else {
            Some("/")
        };
        let mut out = Vec::new();
        let mut marker = None;
        loop {
            let page = self
                .backend
                .list_blobs(&arg.container, arg.prefix.as_deref(), delimiter, marker)
                .await
                .map_err(op_err)?;
            out.extend(page.items.into_iter().map(blob_view));
            match page.continuation {
                Some(m) => marker = Some(m),
                None => break,
            }
        }
        Ok(Json(out))
    }

    #[tool(
        description = "Read a blob's content (read-only). Text is returned as UTF-8; binary as base64."
    )]
    async fn read_blob(
        &self,
        Parameters(arg): Parameters<BlobArg>,
    ) -> Result<Json<ReadView>, ErrorData> {
        let mut stream = self
            .backend
            .read_blob(
                &BlobPath::new(arg.container.clone(), arg.blob.clone()),
                None,
            )
            .await
            .map_err(op_err)?;
        let mut buf = Vec::new();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk.map_err(op_err)?);
        }
        let size = buf.len() as u64;
        let (encoding, content) = match String::from_utf8(buf) {
            Ok(s) => ("utf8".to_string(), s),
            Err(e) => ("base64".to_string(), B64.encode(e.as_bytes())),
        };
        Ok(Json(ReadView {
            container: arg.container,
            blob: arg.blob,
            size,
            encoding,
            content,
        }))
    }

    #[tool(description = "Get a blob's system (HTTP) properties (read-only).")]
    async fn get_properties(
        &self,
        Parameters(arg): Parameters<BlobArg>,
    ) -> Result<Json<PropertiesView>, ErrorData> {
        let p = self
            .backend
            .get_properties(&BlobPath::new(arg.container, arg.blob))
            .await
            .map_err(op_err)?;
        Ok(Json(PropertiesView {
            content_length: p.content_length,
            content_type: p.content_type,
            etag: p.etag,
            blob_type: p.blob_type,
            access_tier: p.access_tier,
            lease_state: p.lease_state,
        }))
    }

    #[tool(description = "Get a blob's user-defined metadata (read-only).")]
    async fn get_metadata(
        &self,
        Parameters(arg): Parameters<BlobArg>,
    ) -> Result<Json<MetadataView>, ErrorData> {
        let metadata = self
            .backend
            .get_metadata(&BlobPath::new(arg.container, arg.blob))
            .await
            .map_err(op_err)?;
        Ok(Json(MetadataView { metadata }))
    }

    #[tool(description = "Get a blob's index tags (read-only).")]
    async fn get_tags(
        &self,
        Parameters(arg): Parameters<BlobArg>,
    ) -> Result<Json<MetadataView>, ErrorData> {
        let metadata = self
            .backend
            .get_tags(&BlobPath::new(arg.container, arg.blob))
            .await
            .map_err(op_err)?;
        Ok(Json(MetadataView { metadata }))
    }

    // -- Destructive / elevated tools (require elicitation) ---------------

    #[tool(description = "DESTRUCTIVE: Write (upload) a text blob. Requires human approval.")]
    async fn write_blob(
        &self,
        Parameters(arg): Parameters<WriteBlobArg>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<WriteResultView>, ErrorData> {
        approve(&ctx, &format!("write blob {}/{}", arg.container, arg.blob)).await?;
        let path = BlobPath::new(arg.container.clone(), arg.blob.clone());
        let bytes = Bytes::from(arg.content.into_bytes());
        let body: arkived_core::ByteStream = Box::pin(futures::stream::once(async move {
            Ok::<_, arkived_core::Error>(bytes)
        }));
        let opts = WriteOpts {
            overwrite: true,
            content_type: arg.content_type,
            ..Default::default()
        };
        let res = self
            .backend
            .write_blob(&allow_ctx(), &path, body, opts)
            .await
            .map_err(op_err)?;
        Ok(Json(WriteResultView {
            container: arg.container,
            blob: arg.blob,
            etag: res.etag,
        }))
    }

    #[tool(description = "DESTRUCTIVE: Delete a blob. Requires human approval.")]
    async fn delete_blob(
        &self,
        Parameters(arg): Parameters<BlobArg>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<OkView>, ErrorData> {
        approve(&ctx, &format!("delete blob {}/{}", arg.container, arg.blob)).await?;
        self.backend
            .delete_blob(
                &allow_ctx(),
                &BlobPath::new(arg.container.clone(), arg.blob.clone()),
                Default::default(),
            )
            .await
            .map_err(op_err)?;
        Ok(Json(OkView {
            ok: true,
            detail: format!("deleted {}/{}", arg.container, arg.blob),
        }))
    }

    #[tool(description = "DESTRUCTIVE: Server-side copy a blob. Requires human approval.")]
    async fn copy_blob(
        &self,
        Parameters(arg): Parameters<CopyBlobArg>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<OkView>, ErrorData> {
        approve(
            &ctx,
            &format!(
                "copy {}/{} -> {}/{}",
                arg.source_container, arg.source_blob, arg.dest_container, arg.dest_blob
            ),
        )
        .await?;
        let endpoint = self.backend.endpoint().as_str().trim_end_matches('/');
        let source_url = format!("{endpoint}/{}/{}", arg.source_container, arg.source_blob);
        self.backend
            .copy_blob(
                &source_url,
                &BlobPath::new(arg.dest_container.clone(), arg.dest_blob.clone()),
            )
            .await
            .map_err(op_err)?;
        Ok(Json(OkView {
            ok: true,
            detail: format!("copied to {}/{}", arg.dest_container, arg.dest_blob),
        }))
    }

    #[tool(description = "ELEVATED: Generate an account-key SAS URL. Requires human approval.")]
    async fn generate_sas(
        &self,
        Parameters(arg): Parameters<SasArg>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SasView>, ErrorData> {
        let resource = match &arg.blob {
            Some(b) => SasResource::Blob(BlobPath::new(arg.container.clone(), b.clone())),
            None => SasResource::Container(arg.container.clone()),
        };
        let perms = arg.permissions.clone().unwrap_or_else(|| "r".into());
        approve(
            &ctx,
            &format!("generate SAS for {} (perms='{perms}')", arg.container),
        )
        .await?;
        let expiry =
            time::OffsetDateTime::now_utc() + time::Duration::hours(arg.expiry_hours.unwrap_or(1));
        let opts = SasOptions {
            permissions: perms,
            expiry,
            start: None,
            protocol: SasProtocol::HttpsOnly,
            ip: None,
        };
        let url = self
            .backend
            .generate_sas(&allow_ctx(), &resource, &opts)
            .await
            .map_err(op_err)?;
        Ok(Json(SasView { url }))
    }

    #[tool(
        description = "ELEVATED: Set a blob's access tier (hot|cool|cold|archive). Requires human approval."
    )]
    async fn set_access_tier(
        &self,
        Parameters(arg): Parameters<SetTierArg>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<OkView>, ErrorData> {
        let tier = Tier::parse(&arg.tier).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("invalid tier '{}' (use hot|cool|cold|archive)", arg.tier),
                None,
            )
        })?;
        approve(
            &ctx,
            &format!(
                "set tier of {}/{} to {}",
                arg.container,
                arg.blob,
                tier.as_str()
            ),
        )
        .await?;
        self.backend
            .set_access_tier(
                &allow_ctx(),
                &BlobPath::new(arg.container.clone(), arg.blob.clone()),
                tier,
            )
            .await
            .map_err(op_err)?;
        Ok(Json(OkView {
            ok: true,
            detail: format!("tier set to {}", tier.as_str()),
        }))
    }

    #[tool(description = "DESTRUCTIVE: Replace a blob's index tags. Requires human approval.")]
    async fn set_tags(
        &self,
        Parameters(arg): Parameters<SetTagsArg>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<OkView>, ErrorData> {
        approve(
            &ctx,
            &format!("replace index tags of {}/{}", arg.container, arg.blob),
        )
        .await?;
        self.backend
            .set_tags(
                &allow_ctx(),
                &BlobPath::new(arg.container.clone(), arg.blob.clone()),
                &arg.tags,
            )
            .await
            .map_err(op_err)?;
        Ok(Json(OkView {
            ok: true,
            detail: format!(
                "set {} tags on {}/{}",
                arg.tags.len(),
                arg.container,
                arg.blob
            ),
        }))
    }

    #[tool(description = "Create a read-only snapshot of a blob. Returns the snapshot id.")]
    async fn create_snapshot(
        &self,
        Parameters(arg): Parameters<BlobArg>,
    ) -> Result<Json<SnapshotView>, ErrorData> {
        let snapshot = self
            .backend
            .create_snapshot(&BlobPath::new(arg.container, arg.blob))
            .await
            .map_err(op_err)?;
        Ok(Json(SnapshotView { snapshot }))
    }

    #[tool(description = "Restore a soft-deleted blob (recovery; not destructive).")]
    async fn undelete_blob(
        &self,
        Parameters(arg): Parameters<BlobArg>,
    ) -> Result<Json<OkView>, ErrorData> {
        self.backend
            .undelete_blob(&BlobPath::new(arg.container.clone(), arg.blob.clone()))
            .await
            .map_err(op_err)?;
        Ok(Json(OkView {
            ok: true,
            detail: format!("undeleted {}/{}", arg.container, arg.blob),
        }))
    }
}

#[tool_handler]
impl ServerHandler for ArkivedServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "arkived".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "Arkived MCP server for Microsoft Azure Blob Storage. Read-only tools \
                 (list_containers, list_blobs, read_blob, get_properties, get_metadata) run \
                 without confirmation. Destructive or elevated tools (write_blob, delete_blob, \
                 copy_blob, generate_sas, set_access_tier) require human approval via an \
                 elicitation prompt before they run."
                    .into(),
            ),
            ..Default::default()
        }
    }
}

fn blob_view(entry: BlobEntry) -> BlobView {
    match entry {
        BlobEntry::Blob {
            name,
            size,
            blob_type,
            tier,
            ..
        } => BlobView {
            name,
            kind: "blob".into(),
            size: Some(size),
            tier,
            blob_type: Some(blob_type),
        },
        BlobEntry::Prefix { name } => BlobView {
            name,
            kind: "directory".into(),
            size: None,
            tier: None,
            blob_type: None,
        },
    }
}

/// Operation context for confirmed operations. Confirmation already happened at
/// the MCP layer via elicitation, so the core policy is `AllowAll` here.
fn allow_ctx() -> Ctx {
    Ctx::new(
        Arc::new(AzuriteEmulatorProvider::new()),
        Arc::new(AllowAllPolicy),
    )
}

/// Ask the connected client's human to approve a destructive action. Returns
/// `Ok(())` only if they explicitly confirm; otherwise an error that refuses
/// the operation.
async fn approve(ctx: &RequestContext<RoleServer>, summary: &str) -> Result<(), ErrorData> {
    let message =
        format!("Arkived wants to {summary}. Set confirm=true to approve, or decline to refuse.");
    match ctx.peer.elicit::<Confirm>(message).await {
        Ok(Some(Confirm { confirm: true })) => Ok(()),
        Ok(_) => Err(ErrorData::invalid_request(
            format!("operation not approved: {summary}"),
            None,
        )),
        Err(_) => Err(ErrorData::invalid_request(
            format!(
                "destructive operation requires confirmation, but the client could not elicit \
                 approval: {summary}"
            ),
            None,
        )),
    }
}

fn op_err(e: arkived_core::Error) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

/// Resolve a backend from the environment and run the MCP server over stdio
/// until the client disconnects.
pub async fn run() -> anyhow::Result<()> {
    use rmcp::transport::stdio;
    use rmcp::ServiceExt;

    let parts = ConnectionParts::from_env();
    if parts.is_empty() {
        anyhow::bail!(
            "no credentials in environment. Set ARKIVED_CONNECTION_STRING, ARKIVED_SAS \
             (+ ARKIVED_ACCOUNT), ARKIVED_ACCOUNT_KEY (+ ARKIVED_ACCOUNT), or ARKIVED_AZURITE."
        );
    }
    let backend = parts.resolve().await?;
    tracing::info!(connection = %parts.describe(), "arkived-mcp starting");
    let server = ArkivedServer::new(backend);

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServerHandler;

    #[test]
    fn blob_view_maps_blob_and_prefix() {
        let blob = blob_view(BlobEntry::Blob {
            name: "a/b.txt".into(),
            size: 42,
            blob_type: "BlockBlob".into(),
            tier: Some("Hot".into()),
            etag: None,
            content_type: None,
            last_modified: None,
            lease_state: None,
        });
        assert_eq!(blob.kind, "blob");
        assert_eq!(blob.size, Some(42));
        assert_eq!(blob.tier.as_deref(), Some("Hot"));

        let dir = blob_view(BlobEntry::Prefix { name: "a/".into() });
        assert_eq!(dir.kind, "directory");
        assert_eq!(dir.size, None);
    }

    #[test]
    fn server_advertises_tools_and_arkived_identity() {
        let backend = arkived_core::ConnectionParts {
            azurite: true,
            ..Default::default()
        };
        // Build a backend synchronously via a tiny runtime.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let backend = rt.block_on(backend.resolve()).unwrap();
        let server = ArkivedServer::new(backend);
        let info = server.get_info();
        assert_eq!(info.server_info.name, "arkived");
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
    }
}
