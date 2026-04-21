//! Integration test against Azurite verifying that our SharedKey signer
//! produces signatures Azurite accepts.
//!
//! **Gating:** this test is `#[ignore]` by default because it requires
//! a running Azurite instance on `127.0.0.1:10000`. Run with:
//!
//! ```text
//! docker run -d -p 10000:10000 mcr.microsoft.com/azure-storage/azurite \
//!     azurite-blob --blobHost 0.0.0.0 --silent
//! cargo test -p arkived-core --test azurite_sharedkey -- --ignored
//! ```

use arkived_core::auth::azurite::{AZURITE_ACCOUNT, AZURITE_BLOB_ENDPOINT, AZURITE_KEY};
use arkived_core::auth::shared_key::{sign, SignRequest};
use secrecy::SecretString;

#[tokio::test]
#[ignore]
async fn signed_list_containers_against_azurite() {
    let client = reqwest::Client::new();
    let url_str = format!("{AZURITE_BLOB_ENDPOINT}?comp=list");
    let url = url::Url::parse(&url_str).unwrap();
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());

    let headers = vec![
        ("x-ms-date".into(), date.clone()),
        ("x-ms-version".into(), "2022-11-02".into()),
    ];
    let req = SignRequest {
        method: "GET",
        url: &url,
        headers: &headers,
    };
    let auth = sign(
        AZURITE_ACCOUNT,
        &SecretString::new(AZURITE_KEY.into()),
        &req,
    )
    .unwrap();

    let resp = client
        .get(&url_str)
        .header("x-ms-date", &date)
        .header("x-ms-version", "2022-11-02")
        .header("authorization", &auth)
        .send()
        .await
        .expect("Azurite must be running on 127.0.0.1:10000 for this test");

    assert!(
        resp.status().is_success(),
        "expected 2xx from Azurite, got {}: {}",
        resp.status(),
        resp.text().await.unwrap_or_default()
    );
}
