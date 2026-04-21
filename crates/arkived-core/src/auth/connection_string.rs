//! Azure Storage connection-string parser + `ConnectionStringProvider`.
//!
//! Connection strings are semicolon-delimited `key=value` pairs. Typical
//! shapes:
//!
//! ```text
//! DefaultEndpointsProtocol=https;AccountName=acme;AccountKey=...;EndpointSuffix=core.windows.net
//! BlobEndpoint=https://acme.blob.core.windows.net;SharedAccessSignature=sv=...&sig=...
//! DefaultEndpointsProtocol=http;AccountName=devstoreaccount1;AccountKey=Eby8vdM...;BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;
//! ```
//!
//! <https://learn.microsoft.com/azure/storage/common/storage-configure-connection-string>

use crate::Error;
use secrecy::ExposeSecret;
use secrecy::SecretString;
use std::collections::HashMap;

/// Parsed connection-string fields we care about for v0.1.0.
#[derive(Debug)]
pub struct ConnectionStringParts {
    /// Raw key=value pairs (preserved order lost; only final value per key retained).
    pub fields: HashMap<String, String>,
}

impl ConnectionStringParts {
    /// Parse a raw connection string.
    pub fn parse(raw: &str) -> crate::Result<Self> {
        let mut fields = HashMap::new();
        for pair in raw.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=').ok_or_else(|| {
                Error::AuthFailed(format!("connection string segment lacks '=': {pair}"))
            })?;
            fields.insert(k.trim().to_string(), v.trim().to_string());
        }
        if fields.is_empty() {
            return Err(Error::AuthFailed("empty connection string".into()));
        }
        Ok(Self { fields })
    }

    /// `AccountName` field (required for account-key flow).
    pub fn account_name(&self) -> Option<&str> {
        self.fields.get("AccountName").map(String::as_str)
    }

    /// `AccountKey` field (base64-encoded).
    pub fn account_key(&self) -> Option<&str> {
        self.fields.get("AccountKey").map(String::as_str)
    }

    /// `SharedAccessSignature` field (SAS query string).
    pub fn sas(&self) -> Option<&str> {
        self.fields.get("SharedAccessSignature").map(String::as_str)
    }

    /// Resolved blob endpoint — honors explicit `BlobEndpoint` first,
    /// otherwise synthesizes `https://<AccountName>.blob.<EndpointSuffix>`.
    pub fn blob_endpoint(&self) -> Option<String> {
        if let Some(e) = self.fields.get("BlobEndpoint") {
            return Some(e.clone());
        }
        let name = self.account_name()?;
        let suffix = self
            .fields
            .get("EndpointSuffix")
            .map(String::as_str)
            .unwrap_or("core.windows.net");
        let proto = self
            .fields
            .get("DefaultEndpointsProtocol")
            .map(String::as_str)
            .unwrap_or("https");
        Some(format!("{proto}://{name}.blob.{suffix}"))
    }

    /// Kind of auth this connection string uses.
    pub fn classify(&self) -> ConnectionStringKind {
        if self.sas().is_some() {
            ConnectionStringKind::Sas
        } else if self.account_key().is_some() && self.account_name().is_some() {
            ConnectionStringKind::AccountKey
        } else {
            ConnectionStringKind::Invalid
        }
    }

    /// Extract the SAS token (validates presence).
    pub fn into_sas(self) -> crate::Result<SecretString> {
        let s = self
            .fields
            .into_iter()
            .find(|(k, _)| k == "SharedAccessSignature")
            .ok_or_else(|| {
                Error::AuthFailed("no SharedAccessSignature in connection string".into())
            })?
            .1;
        Ok(SecretString::new(s))
    }

    /// Extract account-key auth material (validates presence).
    pub fn into_account_key(mut self) -> crate::Result<(String, SecretString)> {
        let name = self
            .fields
            .remove("AccountName")
            .ok_or_else(|| Error::AuthFailed("no AccountName in connection string".into()))?;
        let key = self
            .fields
            .remove("AccountKey")
            .ok_or_else(|| Error::AuthFailed("no AccountKey in connection string".into()))?;
        Ok((name, SecretString::new(key)))
    }
}

/// Classification of a parsed connection string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStringKind {
    /// `AccountName` + `AccountKey` present.
    AccountKey,
    /// `SharedAccessSignature` present.
    Sas,
    /// Neither — invalid.
    Invalid,
}

use crate::auth::{AuthProvider, ResolvedCredential};
use crate::types::{AuthKind, ResourceKind};
use async_trait::async_trait;

/// Auth provider that resolves from a connection string — resolves to either
/// a `SharedKey` or a `Sas` `ResolvedCredential` based on which fields are
/// present.
#[derive(Debug, Clone)]
pub struct ConnectionStringProvider {
    display_name: String,
    raw: SecretString,
}

impl ConnectionStringProvider {
    /// Construct from a raw connection string. Parses eagerly for validation
    /// but re-parses on each `resolve()` because the raw form is kept as the
    /// secret of record.
    pub fn new(display_name: impl Into<String>, raw: SecretString) -> crate::Result<Self> {
        // Validate up front.
        let parts = ConnectionStringParts::parse(raw.expose_secret())?;
        if parts.classify() == ConnectionStringKind::Invalid {
            return Err(Error::AuthFailed(
                "connection string contains neither AccountKey nor SharedAccessSignature".into(),
            ));
        }
        Ok(Self {
            display_name: display_name.into(),
            raw,
        })
    }
}

#[async_trait]
impl AuthProvider for ConnectionStringProvider {
    fn kind(&self) -> AuthKind {
        AuthKind::ConnectionString
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
    async fn resolve(&self) -> crate::Result<ResolvedCredential> {
        let parts = ConnectionStringParts::parse(self.raw.expose_secret())?;
        match parts.classify() {
            ConnectionStringKind::AccountKey => {
                let (name, key) = parts.into_account_key()?;
                Ok(ResolvedCredential::SharedKey {
                    account_name: name,
                    key,
                })
            }
            ConnectionStringKind::Sas => Ok(ResolvedCredential::Sas(parts.into_sas()?)),
            ConnectionStringKind::Invalid => Err(Error::AuthFailed(
                "connection string missing both AccountKey and SharedAccessSignature".into(),
            )),
        }
    }
    fn supports(&self, resource: ResourceKind) -> bool {
        // Connection strings can carry any auth method — we accept everything.
        // The wrapped AccountKey/Sas variants have their own coarser filters;
        // the server is the final arbiter of what works.
        let _ = resource;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn parses_account_key_form() {
        let s = "DefaultEndpointsProtocol=https;AccountName=acme;AccountKey=dGVzdA==;EndpointSuffix=core.windows.net";
        let p = ConnectionStringParts::parse(s).unwrap();
        assert_eq!(p.account_name(), Some("acme"));
        assert_eq!(p.account_key(), Some("dGVzdA=="));
        assert_eq!(p.classify(), ConnectionStringKind::AccountKey);
        assert_eq!(
            p.blob_endpoint(),
            Some("https://acme.blob.core.windows.net".into())
        );
    }

    #[test]
    fn parses_sas_form() {
        let s =
            "BlobEndpoint=https://acme.blob.core.windows.net;SharedAccessSignature=sv=2022&sig=ABC";
        let p = ConnectionStringParts::parse(s).unwrap();
        assert_eq!(p.sas(), Some("sv=2022&sig=ABC"));
        assert_eq!(p.classify(), ConnectionStringKind::Sas);
        assert_eq!(
            p.blob_endpoint(),
            Some("https://acme.blob.core.windows.net".into())
        );
    }

    #[test]
    fn parses_azurite_form() {
        let s = concat!(
            "DefaultEndpointsProtocol=http;",
            "AccountName=devstoreaccount1;",
            "AccountKey=Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==;",
            "BlobEndpoint=http://127.0.0.1:10000/devstoreaccount1;"
        );
        let p = ConnectionStringParts::parse(s).unwrap();
        assert_eq!(p.account_name(), Some("devstoreaccount1"));
        assert_eq!(p.classify(), ConnectionStringKind::AccountKey);
        assert_eq!(
            p.blob_endpoint(),
            Some("http://127.0.0.1:10000/devstoreaccount1".into())
        );
    }

    #[test]
    fn trailing_semicolons_are_ignored() {
        let s = "AccountName=a;AccountKey=k;;;;";
        let p = ConnectionStringParts::parse(s).unwrap();
        assert_eq!(p.fields.len(), 2);
    }

    #[test]
    fn rejects_segment_without_equals() {
        let s = "AccountName=a;garbage;AccountKey=k";
        assert!(matches!(
            ConnectionStringParts::parse(s),
            Err(Error::AuthFailed(_))
        ));
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(
            ConnectionStringParts::parse(""),
            Err(Error::AuthFailed(_))
        ));
        assert!(matches!(
            ConnectionStringParts::parse(";;;"),
            Err(Error::AuthFailed(_))
        ));
    }

    #[test]
    fn classify_invalid_when_neither_present() {
        let p = ConnectionStringParts::parse("BlobEndpoint=https://x").unwrap();
        assert_eq!(p.classify(), ConnectionStringKind::Invalid);
    }

    #[test]
    fn into_account_key_extracts_both_fields() {
        let p = ConnectionStringParts::parse("AccountName=acme;AccountKey=dGVzdA==").unwrap();
        let (name, key) = p.into_account_key().unwrap();
        assert_eq!(name, "acme");
        assert_eq!(key.expose_secret(), "dGVzdA==");
    }

    #[test]
    fn into_sas_extracts_token() {
        let p = ConnectionStringParts::parse("SharedAccessSignature=sv=2022&sig=ABC").unwrap();
        let s = p.into_sas().unwrap();
        assert_eq!(s.expose_secret(), "sv=2022&sig=ABC");
    }

    #[tokio::test]
    async fn provider_account_key_path() {
        let p = ConnectionStringProvider::new(
            "acme",
            SecretString::new("AccountName=acme;AccountKey=dGVzdA==".into()),
        )
        .unwrap();
        match p.resolve().await.unwrap() {
            ResolvedCredential::SharedKey { account_name, key } => {
                assert_eq!(account_name, "acme");
                assert_eq!(key.expose_secret(), "dGVzdA==");
            }
            other => panic!("expected SharedKey, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn provider_sas_path() {
        let p = ConnectionStringProvider::new(
            "dev",
            SecretString::new("BlobEndpoint=https://acme.blob.core.windows.net;SharedAccessSignature=sv=2022&sig=ABC".into()),
        )
        .unwrap();
        match p.resolve().await.unwrap() {
            ResolvedCredential::Sas(s) => assert_eq!(s.expose_secret(), "sv=2022&sig=ABC"),
            other => panic!("expected Sas, got {other:?}"),
        }
    }

    #[test]
    fn provider_rejects_invalid_at_construction() {
        let bad = SecretString::new("BlobEndpoint=https://acme.blob".into());
        assert!(matches!(
            ConnectionStringProvider::new("bad", bad),
            Err(Error::AuthFailed(_))
        ));
    }
}
