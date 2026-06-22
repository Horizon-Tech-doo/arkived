//! Command handlers — each wires a CLI verb to `arkived-core`.

use crate::auth::AuthArgs;
use crate::output;
use crate::path::{classify_arg, parse_remote, CpArg};
use crate::policy::CliPolicy;
use anyhow::{bail, Context, Result};
use arkived_core::auth::AzuriteEmulatorProvider;
use arkived_core::config::{ConfirmMode, OutputFormat};
use arkived_core::{
    AzureBlobBackend, BlobEntry, BlobPath, BlobPropertiesUpdate, Container, Ctx, PublicAccess,
    SasOptions, SasProtocol, SasResource, Tier, WriteOpts,
};
use bytes::Bytes;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

/// Build an operation context with the CLI's confirmation policy. The auth
/// provider on the context is a placeholder — the backend already carries the
/// resolved credential — matching how the desktop app constructs its context.
pub fn make_ctx(mode: ConfirmMode, assume_yes: bool) -> Ctx {
    Ctx::new(
        Arc::new(AzuriteEmulatorProvider::new()),
        Arc::new(CliPolicy::new(mode, assume_yes)),
    )
}

/// `arkived ls [container[/prefix]]`
pub async fn ls(
    backend: &AzureBlobBackend,
    path: Option<String>,
    recursive: bool,
    format: OutputFormat,
) -> Result<()> {
    match path {
        None => {
            let containers = all_containers(backend).await?;
            output::print_containers(&containers, format)?;
        }
        Some(p) => {
            let remote = parse_remote(&p)?;
            // Prefix is everything after the container; ensure trailing '/' so
            // `ls c/dir` lists the contents of `dir/`.
            let prefix = remote
                .blob
                .map(|b| if b.ends_with('/') { b } else { format!("{b}/") });
            let delimiter = if recursive { None } else { Some("/") };
            let blobs = all_blobs(backend, &remote.container, prefix.as_deref(), delimiter).await?;
            output::print_blobs(&blobs, format)?;
        }
    }
    Ok(())
}

/// `arkived cat <container/blob>` — stream a blob to stdout.
pub async fn cat(backend: &AzureBlobBackend, path: String) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("cat needs a blob path like 'container/file.txt'")?;
    let mut stream = backend
        .read_blob(&BlobPath::new(remote.container, blob), None)
        .await?;

    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        lock.write_all(&chunk?)?;
    }
    lock.flush()?;
    Ok(())
}

/// `arkived cp <src> <dst>` — local↔remote and remote↔remote copies.
pub async fn cp(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    src: String,
    dst: String,
    force: bool,
) -> Result<()> {
    match (classify_arg(&src)?, classify_arg(&dst)?) {
        (CpArg::Local(local), CpArg::Remote(remote)) => {
            let filename = local
                .file_name()
                .and_then(|n| n.to_str())
                .context("source has no file name")?
                .to_string();
            let blob = match remote.blob {
                Some(b) if b.ends_with('/') => format!("{b}{filename}"),
                Some(b) => b,
                None => filename,
            };
            let path = BlobPath::new(remote.container, blob);
            let data = tokio::fs::read(&local)
                .await
                .with_context(|| format!("read {}", local.display()))?;
            let len = data.len();
            let bytes = Bytes::from(data);
            let body: arkived_core::ByteStream = Box::pin(futures::stream::once(async move {
                Ok::<_, arkived_core::Error>(bytes)
            }));
            let opts = WriteOpts {
                overwrite: force,
                ..Default::default()
            };
            let res = backend.write_blob(ctx, &path, body, opts).await?;
            eprintln!(
                "uploaded {len} bytes -> {}/{} (etag {})",
                path.container, path.blob, res.etag
            );
        }
        (CpArg::Remote(remote), CpArg::Local(local)) => {
            let blob = remote
                .blob
                .context("download source needs a blob path like 'container/file.txt'")?;
            let target = resolve_local_target(&local, &blob);
            if let Some(parent) = target.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await.ok();
                }
            }
            let mut stream = backend
                .read_blob(&BlobPath::new(remote.container, blob), None)
                .await?;
            let mut file = tokio::fs::File::create(&target)
                .await
                .with_context(|| format!("create {}", target.display()))?;
            let mut total = 0u64;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                total += chunk.len() as u64;
                file.write_all(&chunk).await?;
            }
            file.flush().await?;
            eprintln!("downloaded {total} bytes -> {}", target.display());
        }
        (CpArg::Remote(src), CpArg::Remote(dst)) => {
            let src_blob = src
                .blob
                .context("copy source needs a blob path like 'container/file.txt'")?;
            let dst_blob = match dst.blob {
                Some(b) if b.ends_with('/') => {
                    let base = src_blob.rsplit('/').next().unwrap_or(&src_blob);
                    format!("{b}{base}")
                }
                Some(b) => b,
                None => src_blob.clone(),
            };
            let endpoint = backend.endpoint().as_str().trim_end_matches('/');
            let source_url = format!("{endpoint}/{}/{}", src.container, src_blob);
            backend
                .copy_blob(
                    &source_url,
                    &BlobPath::new(dst.container.clone(), dst_blob.clone()),
                )
                .await?;
            eprintln!(
                "copied {}/{} -> {}/{}",
                src.container, src_blob, dst.container, dst_blob
            );
        }
        (CpArg::Local(_), CpArg::Local(_)) => {
            bail!("both paths are local — use your shell's copy command")
        }
    }
    Ok(())
}

/// `arkived rm <container/blob>` — delete a blob (policy-gated).
pub async fn rm(backend: &AzureBlobBackend, ctx: &Ctx, path: String) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("rm needs a blob path like 'container/file.txt'")?;
    backend
        .delete_blob(
            ctx,
            &BlobPath::new(remote.container, blob),
            Default::default(),
        )
        .await?;
    eprintln!("deleted {path}");
    Ok(())
}

/// `arkived sas <container[/blob]>` — generate an account-key Service SAS URL.
pub async fn sas(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    path: String,
    permissions: String,
    expiry_hours: i64,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let resource = match remote.blob {
        Some(b) => SasResource::Blob(BlobPath::new(remote.container, b)),
        None => SasResource::Container(remote.container),
    };
    let expiry = time::OffsetDateTime::now_utc() + time::Duration::hours(expiry_hours);
    let opts = SasOptions {
        permissions,
        expiry,
        start: None,
        protocol: SasProtocol::HttpsOnly,
        ip: None,
    };
    let url = backend.generate_sas(ctx, &resource, &opts).await?;
    println!("{url}");
    Ok(())
}

/// `arkived set-tier <container/blob> <hot|cool|cold|archive>`
pub async fn set_tier(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    path: String,
    tier: String,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("set-tier needs a blob path like 'container/file.txt'")?;
    let tier = Tier::parse(&tier)
        .with_context(|| format!("invalid tier '{tier}' (use hot|cool|cold|archive)"))?;
    backend
        .set_access_tier(ctx, &BlobPath::new(remote.container, blob), tier)
        .await?;
    eprintln!("access tier set to {}", tier.as_str());
    Ok(())
}

/// Parse a `private|blob|container` access level argument.
fn parse_public_access(s: &str) -> Result<PublicAccess> {
    PublicAccess::parse(s)
        .with_context(|| format!("invalid access level '{s}' (use private|blob|container)"))
}

/// `arkived container create <name> [--public-access ...]`
pub async fn container_create(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    name: String,
    public_access: Option<String>,
) -> Result<()> {
    let access = match public_access {
        Some(s) => parse_public_access(&s)?,
        None => PublicAccess::Private,
    };
    backend.create_container(ctx, &name, access).await?;
    eprintln!("created container {name}");
    Ok(())
}

/// `arkived container delete <name>` — policy-gated.
pub async fn container_delete(backend: &AzureBlobBackend, ctx: &Ctx, name: String) -> Result<()> {
    backend.delete_container(ctx, &name).await?;
    eprintln!("deleted container {name}");
    Ok(())
}

/// `arkived container set-access <name> <private|blob|container>` — policy-gated.
pub async fn container_set_access(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    name: String,
    access: String,
) -> Result<()> {
    let access = parse_public_access(&access)?;
    backend
        .set_container_public_access(ctx, &name, access)
        .await?;
    eprintln!("set public access of {name}");
    Ok(())
}

/// `arkived set-meta <container/blob> key=value...` — replaces all metadata.
pub async fn set_meta(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    path: String,
    pairs: Vec<String>,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("set-meta needs a blob path like 'container/file.txt'")?;
    let mut metadata = HashMap::new();
    for pair in &pairs {
        let (k, v) = pair
            .split_once('=')
            .with_context(|| format!("metadata must be key=value, got '{pair}'"))?;
        metadata.insert(k.trim().to_string(), v.to_string());
    }
    backend
        .set_metadata(ctx, &BlobPath::new(remote.container, blob), &metadata)
        .await?;
    eprintln!("set {} metadata entries on {path}", pairs.len());
    Ok(())
}

/// `arkived set-props <container/blob> [--content-type ...] ...` — read-modify-write
/// so unspecified system properties are preserved (Azure otherwise clears them).
#[allow(clippy::too_many_arguments)]
pub async fn set_props(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    path: String,
    content_type: Option<String>,
    cache_control: Option<String>,
    content_encoding: Option<String>,
    content_language: Option<String>,
    content_disposition: Option<String>,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("set-props needs a blob path like 'container/file.txt'")?;
    let blob_path = BlobPath::new(remote.container, blob);
    let current = backend.get_properties(&blob_path).await?;
    let update = BlobPropertiesUpdate {
        content_type: content_type.or(current.content_type),
        content_encoding: content_encoding.or(current.content_encoding),
        content_language: content_language.or(current.content_language),
        cache_control: cache_control.or(current.cache_control),
        content_disposition: content_disposition.or(current.content_disposition),
        content_md5: current.content_md5,
    };
    backend.set_properties(ctx, &blob_path, &update).await?;
    eprintln!("updated properties on {path}");
    Ok(())
}

/// `arkived tags <container/blob>` — show a blob's index tags.
pub async fn tags(backend: &AzureBlobBackend, path: String, format: OutputFormat) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("tags needs a blob path like 'container/file.txt'")?;
    let tags = backend
        .get_tags(&BlobPath::new(remote.container, blob))
        .await?;
    output::emit_serialized(&tags, format)
}

/// `arkived set-tags <container/blob> key=value...` — replaces all index tags.
pub async fn set_tags(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    path: String,
    pairs: Vec<String>,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("set-tags needs a blob path like 'container/file.txt'")?;
    let mut tags = HashMap::new();
    for pair in &pairs {
        let (k, v) = pair
            .split_once('=')
            .with_context(|| format!("tags must be key=value, got '{pair}'"))?;
        tags.insert(k.trim().to_string(), v.to_string());
    }
    backend
        .set_tags(ctx, &BlobPath::new(remote.container, blob), &tags)
        .await?;
    eprintln!("set {} index tags on {path}", pairs.len());
    Ok(())
}

/// `arkived snapshot <container/blob>` — create a snapshot, print its id.
pub async fn snapshot(backend: &AzureBlobBackend, path: String) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("snapshot needs a blob path like 'container/file.txt'")?;
    let id = backend
        .create_snapshot(&BlobPath::new(remote.container, blob))
        .await?;
    println!("{id}");
    Ok(())
}

/// `arkived undelete <container/blob>` — restore a soft-deleted blob.
pub async fn undelete(backend: &AzureBlobBackend, path: String) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("undelete needs a blob path like 'container/file.txt'")?;
    backend
        .undelete_blob(&BlobPath::new(remote.container, blob))
        .await?;
    eprintln!("undeleted {path}");
    Ok(())
}

/// `arkived rehydrate <container/blob> <tier> [--high]`
pub async fn rehydrate(
    backend: &AzureBlobBackend,
    ctx: &Ctx,
    path: String,
    tier: String,
    high: bool,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("rehydrate needs a blob path like 'container/file.txt'")?;
    let tier =
        Tier::parse(&tier).with_context(|| format!("invalid tier '{tier}' (use hot|cool|cold)"))?;
    backend
        .rehydrate_blob(ctx, &BlobPath::new(remote.container, blob), tier, high)
        .await?;
    eprintln!("rehydration to {} requested", tier.as_str());
    Ok(())
}

/// `arkived lease acquire <container/blob> [--duration N]`
pub async fn lease_acquire(backend: &AzureBlobBackend, path: String, duration: i32) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("lease needs a blob path like 'container/file.txt'")?;
    let id = backend
        .acquire_lease(&BlobPath::new(remote.container, blob), duration)
        .await?;
    println!("{id}");
    Ok(())
}

/// `arkived lease release <container/blob> <lease-id>`
pub async fn lease_release(
    backend: &AzureBlobBackend,
    path: String,
    lease_id: String,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("lease needs a blob path like 'container/file.txt'")?;
    backend
        .release_lease(&BlobPath::new(remote.container, blob), &lease_id)
        .await?;
    eprintln!("released lease on {path}");
    Ok(())
}

/// `arkived lease break <container/blob>` — policy-gated.
pub async fn lease_break(backend: &AzureBlobBackend, ctx: &Ctx, path: String) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("lease needs a blob path like 'container/file.txt'")?;
    backend
        .break_lease(ctx, &BlobPath::new(remote.container, blob))
        .await?;
    eprintln!("broke lease on {path}");
    Ok(())
}

/// `arkived properties <container/blob>` — show a blob's system properties.
pub async fn properties(
    backend: &AzureBlobBackend,
    path: String,
    format: OutputFormat,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("properties needs a blob path like 'container/file.txt'")?;
    let props = backend
        .get_properties(&BlobPath::new(remote.container, blob))
        .await?;
    output::emit_serialized(&props, format)
}

/// `arkived meta <container/blob>` — show a blob's user-defined metadata.
pub async fn metadata(
    backend: &AzureBlobBackend,
    path: String,
    format: OutputFormat,
) -> Result<()> {
    let remote = parse_remote(&path)?;
    let blob = remote
        .blob
        .context("meta needs a blob path like 'container/file.txt'")?;
    let md = backend
        .get_metadata(&BlobPath::new(remote.container, blob))
        .await?;
    output::emit_serialized(&md, format)
}

/// `arkived doctor` — verify the resolved connection can reach the account.
pub async fn doctor(auth: &AuthArgs) -> Result<()> {
    println!("connection: {}", auth.describe());
    let backend = match auth.resolve_backend().await {
        Ok(b) => b,
        Err(e) => {
            println!("credentials: FAILED — {e}");
            bail!("doctor: could not build a backend");
        }
    };
    println!("endpoint:   {}", backend.endpoint());
    match backend.list_containers(None).await {
        Ok(page) => println!("connectivity: OK ({} containers visible)", page.items.len()),
        Err(e) => {
            println!("connectivity: FAILED — {e}");
            bail!("doctor: connectivity check failed");
        }
    }
    Ok(())
}

/// If `local` is an existing directory or ends with a separator, place the blob
/// under it using the blob's base name; otherwise treat `local` as the target file.
fn resolve_local_target(local: &Path, blob: &str) -> PathBuf {
    let ends_with_sep = local
        .as_os_str()
        .to_str()
        .map(|s| s.ends_with('/') || s.ends_with('\\'))
        .unwrap_or(false);
    if local.is_dir() || ends_with_sep {
        let base = blob.rsplit('/').next().unwrap_or(blob);
        local.join(base)
    } else {
        local.to_path_buf()
    }
}

async fn all_containers(backend: &AzureBlobBackend) -> Result<Vec<Container>> {
    let mut out = Vec::new();
    let mut marker = None;
    loop {
        let page = backend.list_containers(marker).await?;
        out.extend(page.items);
        match page.continuation {
            Some(m) => marker = Some(m),
            None => break,
        }
    }
    Ok(out)
}

async fn all_blobs(
    backend: &AzureBlobBackend,
    container: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
) -> Result<Vec<BlobEntry>> {
    let mut out = Vec::new();
    let mut marker = None;
    loop {
        let page = backend
            .list_blobs(container, prefix, delimiter, marker)
            .await?;
        out.extend(page.items);
        match page.continuation {
            Some(m) => marker = Some(m),
            None => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_target_into_existing_dir_uses_basename() {
        let dir = tempfile::tempdir().unwrap();
        let target = resolve_local_target(dir.path(), "reports/q1.pdf");
        assert_eq!(target, dir.path().join("q1.pdf"));
    }

    #[test]
    fn local_target_as_file_is_used_verbatim() {
        let target = resolve_local_target(Path::new("out.bin"), "c/data");
        assert_eq!(target, PathBuf::from("out.bin"));
    }

    #[test]
    fn local_target_trailing_slash_uses_basename() {
        let target = resolve_local_target(Path::new("downloads/"), "a/b/file.txt");
        assert_eq!(target, PathBuf::from("downloads/").join("file.txt"));
    }
}
