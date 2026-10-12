//! CLI listing against loopback HTTP fixtures; no cloud account is required.
use mockito::Matcher;
use serde_json::{json, Value};

async fn run(server: &mockito::ServerGuard, args: &[&str]) -> std::process::Output {
    let config = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_arkived"));
    command.env_clear();
    // Windows networking needs SystemRoot even with an isolated environment.
    #[cfg(windows)]
    command.env(
        "SystemRoot",
        std::env::var_os("SystemRoot").expect("Windows must define SystemRoot"),
    );
    command
        .env("ARKIVED_CONFIG_DIR", config.path())
        .current_dir(config.path())
        .args([
            "--endpoint",
            &server.url(),
            "--sas",
            "sig=fixture",
            "--format",
            "json",
            "ls",
        ])
        .args(args)
        .output()
        .await
        .unwrap()
}
fn json_output(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn containers_follow_continuation_and_keep_both_pages() {
    let mut server = mockito::Server::new_async().await;
    let first = server.mock("GET", "/")
        .match_query(Matcher::Exact("comp=list&sig=fixture".into()))
        .with_body("<EnumerationResults><Containers><Container><Name>alpha</Name></Container></Containers><NextMarker>next page</NextMarker></EnumerationResults>")
        .expect(1).create_async().await;
    let second = server.mock("GET", "/")
        .match_query(Matcher::AllOf(vec![Matcher::UrlEncoded("comp".into(), "list".into()), Matcher::UrlEncoded("marker".into(), "next page".into())]))
        .with_body("<EnumerationResults><Containers><Container><Name>beta</Name></Container></Containers><NextMarker/></EnumerationResults>")
        .expect(1).create_async().await;
    let output = json_output(run(&server, &[]).await);
    assert_eq!(output.as_array().unwrap().len(), 2);
    assert_eq!(output[0]["name"], "alpha");
    assert_eq!(output[1]["name"], "beta");
    first.assert_async().await;
    second.assert_async().await;
}
#[tokio::test]
async fn prefix_listing_adds_slash_and_uses_directory_delimiter() {
    let mut server = mockito::Server::new_async().await;
    let request = server.mock("GET", "/container")
        .match_query(Matcher::Exact("restype=container&comp=list&prefix=dir%2F&delimiter=%2F&sig=fixture".into()))
        .with_body("<EnumerationResults><Blobs><BlobPrefix><Name>dir/sub/</Name></BlobPrefix></Blobs></EnumerationResults>")
        .expect(1).create_async().await;
    assert_eq!(
        json_output(run(&server, &["container/dir"]).await),
        json!([{"Prefix":{"name":"dir/sub/"}}])
    );
    request.assert_async().await;
}
#[tokio::test]
async fn recursive_listing_omits_delimiter_and_returns_blobs() {
    let mut server = mockito::Server::new_async().await;
    let request = server.mock("GET", "/container")
        .match_query(Matcher::Exact("restype=container&comp=list&prefix=dir%2F&sig=fixture".into()))
        .with_body("<EnumerationResults><Blobs><Blob><Name>dir/sub/file</Name><Properties><Content-Length>7</Content-Length></Properties></Blob></Blobs></EnumerationResults>")
        .expect(1).create_async().await;
    let output = json_output(run(&server, &["container/dir/", "--recursive"]).await);
    assert_eq!(output[0]["Blob"]["name"], "dir/sub/file");
    assert_eq!(output[0]["Blob"]["size"], 7);
    request.assert_async().await;
}
#[tokio::test]
async fn empty_container_list_is_a_json_array() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("GET", "/")
        .match_query(Matcher::Exact("comp=list&sig=fixture".into()))
        .with_body("<EnumerationResults><Containers/><NextMarker/></EnumerationResults>")
        .expect(1)
        .create_async()
        .await;
    assert_eq!(json_output(run(&server, &[]).await), json!([]));
    request.assert_async().await;
}
#[tokio::test]
async fn backend_denial_exits_unsuccessfully_without_printing_the_sas() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("GET", "/")
        .match_query(Matcher::Exact("comp=list&sig=fixture".into()))
        .with_status(403)
        .with_body(
            "<Error><Code>AuthorizationFailure</Code><Message>Fixture denied</Message></Error>",
        )
        .expect(1)
        .create_async()
        .await;
    let output = run(&server, &[]).await;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("AuthorizationFailure"), "{error}");
    assert!(error.contains("Fixture denied"), "{error}");
    assert!(!error.contains("sig=fixture"), "{error}");
    request.assert_async().await;
}

#[tokio::test]
async fn blob_pages_preserve_prefix_delimiter_and_encoded_marker() {
    let mut server = mockito::Server::new_async().await;
    let first=server.mock("GET", "/container")
        .match_query(Matcher::Exact("restype=container&comp=list&prefix=dir%2F&delimiter=%2F&sig=fixture".into()))
        .with_body("<EnumerationResults><Blobs><Blob><Name>dir/first</Name></Blob></Blobs><NextMarker>next+/page</NextMarker></EnumerationResults>")
        .expect(1).create_async().await;
    let second=server.mock("GET", "/container")
        .match_query(Matcher::AllOf(vec![Matcher::UrlEncoded("restype".into(),"container".into()),Matcher::UrlEncoded("comp".into(),"list".into()),Matcher::UrlEncoded("prefix".into(),"dir/".into()),Matcher::UrlEncoded("delimiter".into(),"/".into()),Matcher::UrlEncoded("marker".into(),"next+/page".into())]))
        .with_body("<EnumerationResults><Blobs><Blob><Name>dir/second</Name></Blob></Blobs><NextMarker/></EnumerationResults>")
        .expect(1).create_async().await;
    let output = json_output(run(&server, &["container/dir"]).await);
    assert_eq!(output.as_array().unwrap().len(), 2);
    assert_eq!(output[0]["Blob"]["name"], "dir/first");
    assert_eq!(output[1]["Blob"]["name"], "dir/second");
    first.assert_async().await;
    second.assert_async().await;
}
#[tokio::test]
async fn empty_blob_list_is_a_json_array() {
    let mut server = mockito::Server::new_async().await;
    let request = server
        .mock("GET", "/container")
        .match_query(Matcher::Exact(
            "restype=container&comp=list&delimiter=%2F&sig=fixture".into(),
        ))
        .with_body("<EnumerationResults><Blobs/><NextMarker/></EnumerationResults>")
        .expect(1)
        .create_async()
        .await;
    assert_eq!(json_output(run(&server, &["container"]).await), json!([]));
    request.assert_async().await;
}
