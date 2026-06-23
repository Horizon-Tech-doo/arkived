//! Account-key (Service) SAS generation — policy-gated.
//!
//! Builds a Shared Access Signature for a container or blob, signed with the
//! storage account key (HMAC-SHA256) per the
//! [Create a service SAS](https://learn.microsoft.com/rest/api/storageservices/create-service-sas)
//! specification, API version `2022-11-02`.
//!
//! User-delegation (AAD-signed) SAS is intentionally out of scope here — see the
//! completion design spec. When the active credential is not key-based, this
//! returns a clear error explaining that account-key SAS needs the storage key.

use crate::auth::ResolvedCredential;
use crate::backend::azure::auth_bridge::MS_VERSION;
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{SasOptions, SasResource};
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretString};
use sha2::Sha256;
use time::OffsetDateTime;

type HmacSha256 = Hmac<Sha256>;

/// Canonical order Azure requires for blob/container SAS permission letters.
/// Letters supplied out of this order produce an invalid signature.
const SAS_PERM_ORDER: &str = "racwdxltmeop";

impl AzureBlobBackend {
    /// Generate an account-key Service SAS URL for a container or blob.
    ///
    /// Synchronous — this is pure local signing, no network call. The operation
    /// is policy-gated because a SAS grants standing access (often writable) to
    /// the resource; `ctx.policy.confirm("generate_sas", ...)` runs first.
    pub async fn generate_sas(
        &self,
        ctx: &Ctx,
        resource: &SasResource,
        opts: &SasOptions,
    ) -> crate::Result<String> {
        let (account_name, key) = match self.credential.as_ref() {
            ResolvedCredential::SharedKey { account_name, key } => (account_name.clone(), key),
            other => {
                return Err(Error::Backend(format!(
                    "account-key SAS requires the storage account key, but the active \
                     credential is {:?}; user-delegation SAS (AAD-signed) is not yet supported",
                    other.kind()
                )));
            }
        };

        let target = match resource {
            SasResource::Container(c) => c.clone(),
            SasResource::Blob(p) => format!("{}/{}", p.container, p.blob),
        };
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "generate_sas".into(),
                    target: target.clone(),
                    summary: format!(
                        "generate SAS for {} (perms='{}', expires {})",
                        target,
                        opts.permissions,
                        fmt_sas_time(opts.expiry)
                    ),
                    reversible: false,
                },
                &ActionContext::default(),
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let token = build_sas_token(&account_name, key, resource, opts)?;

        // Build the full resource URL and attach the SAS query.
        let mut url = self.endpoint.clone();
        let path = match resource {
            SasResource::Container(c) => format!("/{c}"),
            SasResource::Blob(p) => format!("/{}/{}", p.container, p.blob),
        };
        url.set_path(&path);
        url.set_query(Some(&token));
        Ok(url.to_string())
    }
}

/// Reorder permission letters into Azure's canonical order, dropping unknowns.
fn canonical_permissions(perms: &str) -> String {
    let lower = perms.to_ascii_lowercase();
    SAS_PERM_ORDER
        .chars()
        .filter(|c| lower.contains(*c))
        .collect()
}

/// Format a timestamp as ISO-8601 UTC without fractional seconds (`se`/`st`).
fn fmt_sas_time(dt: OffsetDateTime) -> String {
    let dt = dt.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

/// The signed-resource code (`sr`): `c` for container, `b` for blob.
fn signed_resource(resource: &SasResource) -> &'static str {
    match resource {
        SasResource::Container(_) => "c",
        SasResource::Blob(_) => "b",
    }
}

/// `/blob/{account}/{container}[/{blob}]` — the canonicalized resource string.
fn canonicalized_resource(account_name: &str, resource: &SasResource) -> String {
    match resource {
        SasResource::Container(c) => format!("/blob/{account_name}/{c}"),
        SasResource::Blob(p) => format!("/blob/{account_name}/{}/{}", p.container, p.blob),
    }
}

/// Build the Service SAS string-to-sign (API version 2022-11-02 layout).
///
/// Exposed within the crate for deterministic unit testing against a known
/// vector. The 16 fields are joined by `\n`; trailing optional fields are empty.
pub(crate) fn build_sas_string_to_sign(
    account_name: &str,
    resource: &SasResource,
    opts: &SasOptions,
) -> String {
    let fields = [
        canonical_permissions(&opts.permissions),         // sp
        opts.start.map(fmt_sas_time).unwrap_or_default(), // st
        fmt_sas_time(opts.expiry),                        // se
        canonicalized_resource(account_name, resource),   // canonicalizedResource
        String::new(),                                    // signedIdentifier
        opts.ip.clone().unwrap_or_default(),              // sip
        opts.protocol.as_str().to_string(),               // spr
        MS_VERSION.to_string(),                           // sv
        signed_resource(resource).to_string(),            // sr
        String::new(),                                    // signedSnapshotTime
        String::new(),                                    // signedEncryptionScope
        String::new(),                                    // rscc (Cache-Control)
        String::new(),                                    // rscd (Content-Disposition)
        String::new(),                                    // rsce (Content-Encoding)
        String::new(),                                    // rscl (Content-Language)
        String::new(),                                    // rsct (Content-Type)
    ];
    fields.join("\n")
}

/// Build the full SAS query string (no leading `?`), including the signature.
pub(crate) fn build_sas_token(
    account_name: &str,
    key_b64: &SecretString,
    resource: &SasResource,
    opts: &SasOptions,
) -> crate::Result<String> {
    let string_to_sign = build_sas_string_to_sign(account_name, resource, opts);

    let key = B64
        .decode(key_b64.expose_secret())
        .map_err(|_| Error::AuthFailed("invalid account key (not base64)".into()))?;
    let mut mac = HmacSha256::new_from_slice(&key)
        .map_err(|_| Error::AuthFailed("invalid account key".into()))?;
    mac.update(string_to_sign.as_bytes());
    let signature = B64.encode(mac.finalize().into_bytes());

    let mut q: Vec<(&str, String)> = vec![
        ("sv", MS_VERSION.to_string()),
        ("sr", signed_resource(resource).to_string()),
        ("sp", canonical_permissions(&opts.permissions)),
        ("se", fmt_sas_time(opts.expiry)),
        ("spr", opts.protocol.as_str().to_string()),
    ];
    if let Some(start) = opts.start {
        q.push(("st", fmt_sas_time(start)));
    }
    if let Some(ip) = &opts.ip {
        q.push(("sip", ip.clone()));
    }
    q.push(("sig", signature));

    let token: String = q
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::types::{BlobPath, SasProtocol};
    use crate::policy::{AllowAllPolicy, DenyAllPolicy};
    use crate::progress::NoopSink;
    use crate::types::{AuthKind, ResourceKind};
    use async_trait::async_trait;
    use std::sync::Arc;

    const AZURITE_KEY_B64: &str =
        "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";

    fn expiry_fixture() -> OffsetDateTime {
        let date = time::Date::from_calendar_date(2026, time::Month::June, 21).unwrap();
        let t = time::Time::from_hms(12, 0, 0).unwrap();
        time::PrimitiveDateTime::new(date, t).assume_utc()
    }

    struct FakeAuth;
    #[async_trait]
    impl crate::auth::AuthProvider for FakeAuth {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
        fn display_name(&self) -> &str {
            "fake"
        }
        async fn resolve(&self) -> crate::Result<ResolvedCredential> {
            Ok(ResolvedCredential::Anonymous)
        }
        fn supports(&self, _: ResourceKind) -> bool {
            true
        }
    }

    fn shared_key_backend() -> AzureBlobBackend {
        let endpoint = url::Url::parse("https://devstoreaccount1.blob.core.windows.net/").unwrap();
        AzureBlobBackend::new(
            endpoint,
            ResolvedCredential::SharedKey {
                account_name: "devstoreaccount1".into(),
                key: SecretString::new(AZURITE_KEY_B64.into()),
            },
        )
        .unwrap()
    }

    #[test]
    fn string_to_sign_for_blob_matches_spec_layout() {
        let opts = SasOptions {
            permissions: "r".into(),
            expiry: expiry_fixture(),
            start: None,
            protocol: SasProtocol::HttpsOnly,
            ip: None,
        };
        let s = build_sas_string_to_sign(
            "devstoreaccount1",
            &SasResource::Blob(BlobPath::new("mycontainer", "file.txt")),
            &opts,
        );
        // Built explicitly (not via the implementation's join) to catch layout drift.
        let expected = concat!(
            "r\n",                                           // sp
            "\n",                                            // st (empty)
            "2026-06-21T12:00:00Z\n",                        // se
            "/blob/devstoreaccount1/mycontainer/file.txt\n", // canonicalized resource
            "\n",                                            // signed identifier
            "\n",                                            // sip (empty)
            "https\n",                                       // spr
            "2022-11-02\n",                                  // sv
            "b\n",                                           // sr
            "\n",                                            // snapshot
            "\n",                                            // encryption scope
            "\n",                                            // rscc
            "\n",                                            // rscd
            "\n",                                            // rsce
            "\n",                                            // rscl
            "",                                              // rsct (no trailing newline)
        );
        assert_eq!(s, expected);
    }

    #[test]
    fn permissions_are_reordered_to_canonical() {
        // "wr" -> "rw", "dwr" -> "rwd".
        assert_eq!(canonical_permissions("wr"), "rw");
        assert_eq!(canonical_permissions("dwr"), "rwd");
        assert_eq!(canonical_permissions("LR"), "rl");
    }

    #[tokio::test]
    async fn token_contains_required_params() {
        let backend = shared_key_backend();
        let ctx = Ctx::new(Arc::new(FakeAuth), Arc::new(AllowAllPolicy))
            .with_progress(Arc::new(NoopSink));
        let opts = SasOptions {
            permissions: "r".into(),
            expiry: expiry_fixture(),
            start: None,
            protocol: SasProtocol::HttpsOnly,
            ip: None,
        };
        let url = backend
            .generate_sas(
                &ctx,
                &SasResource::Blob(BlobPath::new("mycontainer", "file.txt")),
                &opts,
            )
            .await
            .unwrap();
        assert!(url.contains("/mycontainer/file.txt?"));
        assert!(url.contains("sv=2022-11-02"));
        assert!(url.contains("sr=b"));
        assert!(url.contains("sp=r"));
        assert!(url.contains("se=2026-06-21T12%3A00%3A00Z"));
        assert!(url.contains("sig="));
    }

    #[tokio::test]
    async fn deny_all_policy_short_circuits() {
        let backend = shared_key_backend();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));
        let opts = SasOptions {
            permissions: "r".into(),
            expiry: expiry_fixture(),
            start: None,
            protocol: SasProtocol::HttpsOnly,
            ip: None,
        };
        let err = backend
            .generate_sas(&ctx, &SasResource::Container("c".into()), &opts)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn non_key_credential_is_rejected() {
        let endpoint = url::Url::parse("https://acme.blob.core.windows.net/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx = Ctx::new(Arc::new(FakeAuth), Arc::new(AllowAllPolicy))
            .with_progress(Arc::new(NoopSink));
        let opts = SasOptions {
            permissions: "r".into(),
            expiry: expiry_fixture(),
            start: None,
            protocol: SasProtocol::HttpsOnly,
            ip: None,
        };
        let err = backend
            .generate_sas(&ctx, &SasResource::Container("c".into()), &opts)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Backend(_)));
    }
}
