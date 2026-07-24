//! Concrete **HS256 (HMAC-SHA256) JWT** [`TokenVerifier`](crate::authn::TokenVerifier) (CPE-965, epic
//! CPE-810) — the "real verifier wired at integration" the [`OidcAuthenticator`](crate::authn::
//! OidcAuthenticator) (CPE-817) was built to accept, so an OAuth/OIDC bearer token can be verified
//! end-to-end instead of only with a test fake.
//!
//! Feature-gated (`jwt`) and OFF by default, so the security CORE keeps its dependency-light,
//! machinery-out-of-the-core invariant — the crypto is strictly opt-in.
//!
//! Scope: symmetric **HS256** against a shared secret — the self-hosted-issuer common case. Asymmetric
//! RS256 + JWKS (fetch issuer public keys over HTTP) is a further provider; this proves the trait
//! end-to-end without an HTTP stack. The clock is injectable so verification stays pure + deterministic.

use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::authn::{TokenVerifier, VerifiedClaims};

type HmacSha256 = Hmac<Sha256>;

/// Verifies compact HS256 JWTs against a shared `secret`: the signature over `header.payload`, the `exp`
/// and `nbf` claims (with a small clock leeway), and — when configured — the `aud`.
pub struct HmacJwtVerifier {
    secret: Vec<u8>,
    expected_audience: Option<String>,
    leeway_secs: u64,
    now: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl HmacJwtVerifier {
    /// A verifier over the shared HMAC `secret`, 30s clock leeway, no audience restriction.
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
            expected_audience: None,
            leeway_secs: 30,
            now: Box::new(unix_now),
        }
    }

    /// Require the token's `aud` to include this audience.
    pub fn with_audience(mut self, aud: impl Into<String>) -> Self {
        self.expected_audience = Some(aud.into());
        self
    }

    /// Clock skew tolerated on `exp`/`nbf`, in seconds (default 30).
    pub fn with_leeway_secs(mut self, secs: u64) -> Self {
        self.leeway_secs = secs;
        self
    }

    /// Inject the current-time source (unix seconds) — for deterministic tests.
    pub fn with_clock(mut self, now: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.now = Box::new(now);
        self
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|_| "invalid base64url".to_string())
}

impl TokenVerifier for HmacJwtVerifier {
    fn verify(&self, token: &str) -> Result<VerifiedClaims, String> {
        // Exactly three dot-separated segments: header.payload.signature.
        let mut parts = token.split('.');
        let (h, p, sig) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(h), Some(p), Some(s), None) => (h, p, s),
            _ => return Err("malformed JWT (expected header.payload.signature)".into()),
        };

        // Header — algorithm must be HS256 (never trust `alg: none`).
        let header: Value = serde_json::from_slice(&b64url_decode(h)?).map_err(|_| "invalid header json".to_string())?;
        if header.get("alg").and_then(|a| a.as_str()) != Some("HS256") {
            return Err("unsupported alg (only HS256 accepted)".into());
        }

        // Signature — HMAC-SHA256 over "header.payload". `verify_slice` is constant-time.
        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|_| "invalid HMAC key".to_string())?;
        mac.update(format!("{h}.{p}").as_bytes());
        mac.verify_slice(&b64url_decode(sig)?).map_err(|_| "signature mismatch".to_string())?;

        // Claims — validate time window + optional audience.
        let claims: Value = serde_json::from_slice(&b64url_decode(p)?).map_err(|_| "invalid payload json".to_string())?;
        let now = (self.now)();
        if let Some(exp) = claims.get("exp").and_then(|v| v.as_u64()) {
            if now > exp.saturating_add(self.leeway_secs) {
                return Err("token expired".into());
            }
        }
        if let Some(nbf) = claims.get("nbf").and_then(|v| v.as_u64()) {
            if now.saturating_add(self.leeway_secs) < nbf {
                return Err("token not yet valid".into());
            }
        }
        if let Some(want) = &self.expected_audience {
            let ok = match claims.get("aud") {
                Some(Value::String(s)) => s == want,
                Some(Value::Array(a)) => a.iter().any(|v| v.as_str() == Some(want.as_str())),
                _ => false,
            };
            if !ok {
                return Err("audience mismatch".into());
            }
        }

        let subject = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing `sub` claim".to_string())?
            .to_string();
        let issuer = claims.get("iss").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let preferred_username = claims
            .get("preferred_username")
            .or_else(|| claims.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(VerifiedClaims { subject, preferred_username, issuer })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Mint a signed HS256 JWT with the given secret + claims, for the tests.
    fn mint(secret: &[u8], claims: Value) -> String {
        let enc = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
        let header = enc(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = enc(&serde_json::to_vec(&claims).unwrap());
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(format!("{header}.{payload}").as_bytes());
        let sig = enc(&mac.finalize().into_bytes());
        format!("{header}.{payload}.{sig}")
    }

    const SECRET: &[u8] = b"super-secret-signing-key";

    #[test]
    fn verifies_a_valid_token_and_maps_claims() {
        let tok = mint(SECRET, json!({ "sub": "alice", "iss": "https://issuer", "preferred_username": "Alice", "exp": 4_000_000_000u64 }));
        let v = HmacJwtVerifier::new(SECRET).with_clock(|| 1_000_000_000);
        let c = v.verify(&tok).unwrap();
        assert_eq!(c.subject, "alice");
        assert_eq!(c.issuer, "https://issuer");
        assert_eq!(c.preferred_username.as_deref(), Some("Alice"));
    }

    #[test]
    fn rejects_a_tampered_signature() {
        let tok = mint(SECRET, json!({ "sub": "alice" }));
        let bad = format!("{}x", &tok[..tok.len() - 1]); // mangle the last sig char
        assert!(HmacJwtVerifier::new(SECRET).verify(&bad).is_err());
        // Wrong secret also fails.
        assert!(HmacJwtVerifier::new(b"other-key".to_vec()).verify(&tok).is_err());
    }

    #[test]
    fn rejects_expired_and_wrong_alg_and_bad_audience() {
        let v = HmacJwtVerifier::new(SECRET).with_clock(|| 2_000);
        // Expired (exp far in the past, beyond leeway).
        let expired = mint(SECRET, json!({ "sub": "a", "exp": 1_000u64 }));
        assert_eq!(v.verify(&expired).unwrap_err(), "token expired");
        // Audience mismatch.
        let vv = HmacJwtVerifier::new(SECRET).with_clock(|| 2_000).with_audience("cpe");
        let wrong_aud = mint(SECRET, json!({ "sub": "a", "aud": "other" }));
        assert_eq!(vv.verify(&wrong_aud).unwrap_err(), "audience mismatch");
        // `alg: none` (unsigned) must be refused even with an empty signature.
        let enc = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
        let none = format!("{}.{}.", enc(br#"{"alg":"none"}"#), enc(br#"{"sub":"a"}"#));
        assert!(v.verify(&none).is_err());
    }

    #[test]
    fn drives_the_oidc_authenticator_end_to_end() {
        use crate::authn::{OidcAuthenticator, ATTR_BEARER};
        use crate::{Authenticator, SecurityContext, Verdict};
        use std::sync::Arc;

        let tok = mint(SECRET, json!({ "sub": "bob", "iss": "https://issuer" }));
        let auth = OidcAuthenticator::new(Arc::new(HmacJwtVerifier::new(SECRET).with_clock(|| 1_000)))
            .with_accepted_issuers(vec!["https://issuer".into()]);

        let mut ctx = SecurityContext::local("read");
        ctx.attributes.insert(ATTR_BEARER.to_string(), tok);
        assert_eq!(auth.authenticate(&mut ctx), Verdict::Allow);
        assert_eq!(ctx.principal.id, "bob");
    }
}
