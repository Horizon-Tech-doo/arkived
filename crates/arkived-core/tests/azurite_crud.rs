//! Integration test against Azurite verifying our hand-rolled Blob REST
//! client works end-to-end through SharedKey auth.
//!
//! **Gating:** `#[ignore]` by default. Requires Azurite on 127.0.0.1:10000.
//!
//! ```text
//! docker run -d -p 10000:10000 mcr.microsoft.com/azure-storage/azurite \
//!     azurite-blob --blobHost 0.0.0.0 --silent
//! cargo test -p arkived-core --test azurite_crud -- --ignored
//! ```

use arkived_core::auth::{AuthProvider, AzuriteEmulatorProvider};
use arkived_core::backend::AzureBlobBackend;
use arkived_core::backend::{BlobEntry, BlobPath, DeleteOpts, WriteOpts};
use arkived_core::policy::AllowAllPolicy;
use arkived_core::progress::NoopSink;
use arkived_core::Ctx;
use bytes::Bytes;
use futures::stream::{self, StreamExt, TryStreamExt};
use std::sync::Arc;

const AZURITE_BLOB_ENDPOINT: &str = "http://127.0.0.1:10000/devstoreaccount1";

async fn backend() -> AzureBlobBackend {
    let provider = AzuriteEmulatorProvider::new();
    let cred = provider.resolve().await.unwrap();
    let url = url::Url::parse(AZURITE_BLOB_ENDPOINT).unwrap();
    AzureBlobBackend::new(url, cred).unwrap()
}

fn ctx() -> Ctx {
    let provider: Arc<dyn AuthProvider> = Arc::new(AzuriteEmulatorProvider::new());
    Ctx::new(provider, Arc::new(AllowAllPolicy)).with_progress(Arc::new(NoopSink))
}

#[tokio::test]
#[ignore]
async fn list_containers_against_azurite() {
    let b = backend().await;
    let page = b.list_containers(None).await.expect("azurite must be running");
    // We don't assert count — Azurite starts empty in CI but might have state on a dev box.
    println!("containers: {}", page.items.len());
}

#[tokio::test]
#[ignore]
async fn full_write_read_delete_cycle() {
    let b = backend().await;
    let c = ctx();

    // Ensure a container exists. Azurite doesn't auto-create; we cheat by
    // calling PUT container via reqwest directly for this test since
    // create_container isn't in v0.1.0 scope.
    ensure_container(&b, "arkivedci").await;

    let path = BlobPath::new("arkivedci", format!("test-{}", uuid::Uuid::new_v4()));
    let body_bytes = Bytes::from("hello-arkived");
    let body = stream::iter(vec![Ok::<_, arkived_core::Error>(body_bytes.clone())]).boxed();

    // WRITE
    let wr = b
        .write_blob(
            &c,
            &path,
            body,
            WriteOpts {
                overwrite: true,
                content_type: Some("text/plain".into()),
                ..Default::default()
            },
        )
        .await
        .expect("write_blob");
    assert!(!wr.etag.is_empty());

    // READ
    let stream = b.read_blob(&path, None).await.expect("read_blob");
    let collected: Bytes = stream
        .try_fold(bytes::BytesMut::new(), |mut acc, chunk| async move {
            acc.extend_from_slice(&chunk);
            Ok(acc)
        })
        .await
        .expect("stream")
        .freeze();
    assert_eq!(&collected[..], b"hello-arkived");

    // LIST
    let page = b.list_blobs("arkivedci", None, None, None).await.expect("list_blobs");
    assert!(page.items.iter().any(|e| matches!(e, BlobEntry::Blob { name, .. } if name == &path.blob)));

    // DELETE
    b.delete_blob(&c, &path, DeleteOpts::default()).await.expect("delete_blob");
}

/// Create a container if it doesn't already exist. Out-of-scope for v0.1.0
/// proper (container creation lands in the Backend-Depth plan), so we roll
/// it here as a test helper only.
async fn ensure_container(b: &AzureBlobBackend, name: &str) {
    let http = reqwest::Client::new();
    let mut url = b.endpoint().clone();
    url.set_path(&format!("/{}", name));
    url.set_query(Some("restype=container"));

    // Authorize the PUT manually using our signer.
    use arkived_core::auth::shared_key::{sign, SignRequest};
    use secrecy::SecretString;

    const AZURITE_KEY: &str =
        "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let headers = vec![
        ("x-ms-date".into(), date.clone()),
        ("x-ms-version".into(), "2022-11-02".into()),
    ];
    let auth = sign(
        "devstoreaccount1",
        &SecretString::new(AZURITE_KEY.into()),
        &SignRequest { method: "PUT", url: &url, headers: &headers },
    )
    .unwrap();
    let resp = http
        .put(url)
        .header("x-ms-date", &date)
        .header("x-ms-version", "2022-11-02")
        .header("authorization", &auth)
        .header("content-length", "0")
        .send()
        .await
        .expect("azurite PUT container");
    // 201 = created, 409 = ContainerAlreadyExists — both fine.
    assert!(
        resp.status().is_success() || resp.status().as_u16() == 409,
        "PUT container status: {}",
        resp.status()
    );
}
