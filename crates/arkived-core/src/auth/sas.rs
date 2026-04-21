//! Shared access signature (SAS) auth.
//!
//! Accepts either a full SAS URL (`https://acme.blob.core.windows.net/c?sv=...&sig=...`)
//! or a bare query-string form (`sv=...&sig=...`). Normalizes internally to
//! the bare query form — the backend re-attaches it to every request URL.

use crate::auth::{AuthProvider, ResolvedCredential};
use crate::types::{AuthKind, ResourceKind};
use crate::Error;
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};

/// SAS-token provider.
#[derive(Debug, Clone)]
pub struct SasTokenProvider {
    display_name: String,
    /// Stored normalized: query-string without leading `?`.
    token: SecretString,
}

impl SasTokenProvider {
    /// Construct from a raw SAS URL or bare SAS query string.
    ///
    /// Accepted forms:
    /// - `https://acme.blob.core.windows.net/container?sv=...&sig=...`
    /// - `?sv=...&sig=...`
    /// - `sv=...&sig=...`
    pub fn new(display_name: impl Into<String>, raw: SecretString) -> crate::Result<Self> {
        let normalized = normalize_sas(raw.expose_secret())?;
        Ok(Self {
            display_name: display_name.into(),
            token: SecretString::new(normalized),
        })
    }
}

#[async_trait]
impl AuthProvider for SasTokenProvider {
    fn kind(&self) -> AuthKind {
        AuthKind::SasToken
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
    async fn resolve(&self) -> crate::Result<ResolvedCredential> {
        Ok(ResolvedCredential::Sas(self.token.clone()))
    }
    fn supports(&self, resource: ResourceKind) -> bool {
        // SAS can target any resource the SAS was generated for — the provider
        // doesn't know the scope, so we answer `true` and let the server reject.
        !matches!(resource, ResourceKind::StorageAccount)
    }
}

fn normalize_sas(raw: &str) -> crate::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::AuthFailed("empty SAS token".into()));
    }

    // URL form: extract the query string after the first '?'.
    if let Some(idx) = trimmed.find('?') {
        let after = &trimmed[idx + 1..];
        if after.is_empty() {
            return Err(Error::AuthFailed("SAS URL has no query string".into()));
        }
        validate_shape(after)?;
        return Ok(after.to_string());
    }

    // Bare query form.
    validate_shape(trimmed)?;
    Ok(trimmed.to_string())
}

fn validate_shape(q: &str) -> crate::Result<()> {
    // Sanity: must contain either `sig=` or `ske=` (required SAS fields).
    if !q.contains("sig=") && !q.contains("ske=") {
        return Err(Error::AuthFailed(
            "SAS token missing required `sig=` or `ske=` parameter".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "sv=2022-11-02&ss=b&srt=sco&sp=rwdlac&se=2099-01-01T00:00Z&sig=ABC";

    #[tokio::test]
    async fn accepts_bare_query() {
        let p = SasTokenProvider::new("dev", SecretString::new(SAMPLE.into())).unwrap();
        match p.resolve().await.unwrap() {
            ResolvedCredential::Sas(s) => assert_eq!(s.expose_secret(), SAMPLE),
            other => panic!("expected Sas, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn accepts_url_form_and_strips_prefix() {
        let url = format!("https://acme.blob.core.windows.net/container?{SAMPLE}");
        let p = SasTokenProvider::new("dev", SecretString::new(url)).unwrap();
        match p.resolve().await.unwrap() {
            ResolvedCredential::Sas(s) => assert_eq!(s.expose_secret(), SAMPLE),
            other => panic!("expected Sas, got {other:?}"),
        }
    }

    #[test]
    fn accepts_leading_question_mark() {
        let with_q = format!("?{SAMPLE}");
        let p = SasTokenProvider::new("dev", SecretString::new(with_q)).unwrap();
        assert_eq!(p.display_name(), "dev");
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(
            SasTokenProvider::new("x", SecretString::new("".into())),
            Err(Error::AuthFailed(_))
        ));
    }

    #[test]
    fn rejects_missing_signature() {
        assert!(matches!(
            SasTokenProvider::new("x", SecretString::new("sv=2022&sp=r".into())),
            Err(Error::AuthFailed(_))
        ));
    }

    #[test]
    fn rejects_url_with_no_query() {
        assert!(matches!(
            SasTokenProvider::new(
                "x",
                SecretString::new("https://acme.blob.core.windows.net/".into())
            ),
            Err(Error::AuthFailed(_))
        ));
    }

    #[test]
    fn supports_resources_except_storage_account_scope() {
        let p = SasTokenProvider::new("x", SecretString::new(SAMPLE.into())).unwrap();
        assert!(p.supports(ResourceKind::BlobContainer));
        assert!(p.supports(ResourceKind::Queue));
        assert!(!p.supports(ResourceKind::StorageAccount));
    }
}
