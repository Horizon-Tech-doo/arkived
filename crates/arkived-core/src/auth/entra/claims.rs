//! Reading identity claims from an Entra JWT.
//!
//! This does **not** verify the token signature — it only decodes the payload
//! of a token the authority just issued to us, to label a sign-in with the
//! user's principal name and tenant. Never use this for authorization.

use base64::Engine;
use serde::Deserialize;

/// Identity fields extracted from a token, all best-effort.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IdentityClaims {
    /// Tenant id (`tid`).
    pub tenant_id: Option<String>,
    /// User principal — `preferred_username`, else `upn`/`unique_name`/`email`.
    pub user_principal: Option<String>,
    /// Stable user object id (`oid`).
    pub object_id: Option<String>,
    /// Display name (`name`).
    pub name: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawClaims {
    tid: Option<String>,
    oid: Option<String>,
    name: Option<String>,
    preferred_username: Option<String>,
    upn: Option<String>,
    unique_name: Option<String>,
    email: Option<String>,
}

/// Parse identity claims from a JWT's payload segment.
///
/// Returns defaults if the token isn't a well-formed three-part JWT or the
/// payload isn't valid base64url-encoded JSON.
pub fn parse_identity_claims(jwt: &str) -> IdentityClaims {
    let Some(payload) = jwt.split('.').nth(1) else {
        return IdentityClaims::default();
    };
    let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return IdentityClaims::default();
    };
    let Ok(raw) = serde_json::from_slice::<RawClaims>(&bytes) else {
        return IdentityClaims::default();
    };
    IdentityClaims {
        tenant_id: raw.tid,
        user_principal: raw
            .preferred_username
            .or(raw.upn)
            .or(raw.unique_name)
            .or(raw.email),
        object_id: raw.oid,
        name: raw.name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fake JWT (`header.payload.sig`) whose payload is `json`.
    fn jwt_with_payload(json: &str) -> String {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
        format!("aGVhZGVy.{payload}.c2ln")
    }

    #[test]
    fn extracts_preferred_username_and_tenant() {
        let jwt = jwt_with_payload(
            r#"{"tid":"tenant-1","preferred_username":"hamza@horizon-tech.io","oid":"obj-9","name":"Hamza"}"#,
        );
        let c = parse_identity_claims(&jwt);
        assert_eq!(c.tenant_id.as_deref(), Some("tenant-1"));
        assert_eq!(c.user_principal.as_deref(), Some("hamza@horizon-tech.io"));
        assert_eq!(c.object_id.as_deref(), Some("obj-9"));
        assert_eq!(c.name.as_deref(), Some("Hamza"));
    }

    #[test]
    fn falls_back_to_upn_then_unique_name() {
        let upn = jwt_with_payload(r#"{"upn":"u@x.io"}"#);
        assert_eq!(
            parse_identity_claims(&upn).user_principal.as_deref(),
            Some("u@x.io")
        );
        let uniq = jwt_with_payload(r#"{"unique_name":"legacy@x.io"}"#);
        assert_eq!(
            parse_identity_claims(&uniq).user_principal.as_deref(),
            Some("legacy@x.io")
        );
    }

    #[test]
    fn malformed_tokens_yield_defaults() {
        assert_eq!(parse_identity_claims(""), IdentityClaims::default());
        assert_eq!(
            parse_identity_claims("not-a-jwt"),
            IdentityClaims::default()
        );
        assert_eq!(
            parse_identity_claims("a.!!notbase64!!.c"),
            IdentityClaims::default()
        );
        assert_eq!(
            parse_identity_claims(&jwt_with_payload("not json")),
            IdentityClaims::default()
        );
    }
}
