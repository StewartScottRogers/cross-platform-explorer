//! Authentication providers (CPE-817) plugging into the [`Authenticator`] plane trait.
//!
//! Three interchangeable, dependency-free providers prove the *full* AuthN stack and the
//! `any-passes` composition (accept **either** an API token **or** an OAuth identity while
//! both are registered):
//!
//! - [`ApiTokenAuthenticator`] — a presented bearer token, constant-time-matched against a
//!   token→[`Principal`] table.
//! - [`MtlsIdentityAuthenticator`] — the client-certificate subject the **transport** plane
//!   (CPE-818) verified and placed in [`SecurityContext::attributes`], mapped to a principal.
//! - [`OidcAuthenticator`] — an OAuth/OIDC bearer token verified through an injectable
//!   [`TokenVerifier`], so JWKS/JWT/HTTP machinery stays out of this core crate (a real
//!   verifier is wired at integration).
//!
//! **Composition contract:** a provider [`Abstain`](Verdict::Abstain)s when *its* kind of
//! credential is not presented, so an `AnyPasses` plane falls through to the next provider;
//! it only [`Deny`](Verdict::Deny)s when a credential *is* presented but fails to verify. A
//! plane where every provider abstains (no credential at all) hits the core's structural
//! default-deny.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{Authenticator, Principal, SecurityContext, Verdict};

/// Default context-attribute key an API token is presented under.
pub const ATTR_API_TOKEN: &str = "auth.api_token";
/// Default context-attribute key a verified client-cert subject is presented under (set by
/// the transport-security plane after a successful mTLS handshake).
pub const ATTR_CLIENT_CERT_SUBJECT: &str = "tls.client_cert.subject";
/// Default context-attribute key an OAuth/OIDC bearer token is presented under.
pub const ATTR_BEARER: &str = "auth.bearer";

/// Best-effort constant-time byte comparison, so token verification does not leak content
/// through timing. Compares over the max length to avoid an early-out on the *content* scan.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    // Fold any length difference to a single bit. Truncating `a.len() ^ b.len()` to `u8` (as an
    // earlier version did) let lengths differing by a multiple of 256 alias to 0, so e.g.
    // `ct_eq(b"A", &[b'A', 0, 0, …256 zeros])` wrongly compared equal. Branching on *length* (not
    // on secret byte content) leaks nothing sensitive; the content scan below stays branch-free.
    let mut diff: u8 = if a.len() == b.len() { 0 } else { 1 };
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// API token
// ---------------------------------------------------------------------------

/// Authenticates a request by matching a presented bearer token against a configured
/// token→[`Principal`] table. Abstains when no token is presented (so `any-passes` can try
/// another provider); denies when a token is presented but unknown.
pub struct ApiTokenAuthenticator {
    name: String,
    attribute: String,
    tokens: BTreeMap<String, Principal>,
}

impl ApiTokenAuthenticator {
    /// Build with a `token → principal` table, reading the token from [`ATTR_API_TOKEN`].
    pub fn new(tokens: BTreeMap<String, Principal>) -> Self {
        Self {
            name: "api_token".to_string(),
            attribute: ATTR_API_TOKEN.to_string(),
            tokens,
        }
    }

    /// Override which context attribute carries the presented token.
    pub fn with_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.attribute = attribute.into();
        self
    }
}

impl Authenticator for ApiTokenAuthenticator {
    fn name(&self) -> &str {
        &self.name
    }

    fn authenticate(&self, ctx: &mut SecurityContext) -> Verdict {
        let presented = match ctx.attributes.get(&self.attribute) {
            Some(t) => t.clone(),
            None => return Verdict::Abstain, // no API token on this request
        };
        // Constant-time compare against every known token; do not early-return on the first
        // mismatch (that would reintroduce a timing signal on which token matched).
        let mut matched: Option<&Principal> = None;
        for (token, principal) in &self.tokens {
            if ct_eq(token.as_bytes(), presented.as_bytes()) {
                matched = Some(principal);
            }
        }
        match matched {
            Some(principal) => {
                ctx.principal = principal.clone();
                Verdict::Allow
            }
            None => Verdict::deny("api_token: unrecognized token"),
        }
    }
}

// ---------------------------------------------------------------------------
// mTLS client-certificate identity
// ---------------------------------------------------------------------------

/// Authenticates a request from the **already-verified** client-certificate subject that the
/// transport plane placed in the context. This provider does not perform the TLS handshake
/// (that is the transport-security plane's job, CPE-818) — it trusts the verified subject
/// attribute and maps it to a principal.
pub struct MtlsIdentityAuthenticator {
    name: String,
    subject_attribute: String,
    /// When `Some`, only these subjects are accepted (mapped to the given principal). When
    /// `None`, any verified subject is accepted, with the subject as the principal id.
    allowed: Option<BTreeMap<String, Principal>>,
}

impl MtlsIdentityAuthenticator {
    /// Accept any verified client-cert subject, using the subject string as the principal id.
    pub fn accept_any_verified() -> Self {
        Self {
            name: "mtls_identity".to_string(),
            subject_attribute: ATTR_CLIENT_CERT_SUBJECT.to_string(),
            allowed: None,
        }
    }

    /// Accept only the given `subject → principal` mappings.
    pub fn with_allowlist(allowed: BTreeMap<String, Principal>) -> Self {
        Self {
            name: "mtls_identity".to_string(),
            subject_attribute: ATTR_CLIENT_CERT_SUBJECT.to_string(),
            allowed: Some(allowed),
        }
    }

    /// Override which context attribute carries the verified subject.
    pub fn with_subject_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.subject_attribute = attribute.into();
        self
    }
}

impl Authenticator for MtlsIdentityAuthenticator {
    fn name(&self) -> &str {
        &self.name
    }

    fn authenticate(&self, ctx: &mut SecurityContext) -> Verdict {
        let subject = match ctx.attributes.get(&self.subject_attribute) {
            Some(s) => s.clone(),
            None => return Verdict::Abstain, // no verified client cert on this channel
        };
        match &self.allowed {
            Some(map) => match map.get(&subject) {
                Some(principal) => {
                    ctx.principal = principal.clone();
                    Verdict::Allow
                }
                None => Verdict::deny("mtls_identity: client-certificate subject not allowed"),
            },
            None => {
                ctx.principal = Principal {
                    id: subject.clone(),
                    display_name: Some(subject),
                };
                Verdict::Allow
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OAuth / OIDC
// ---------------------------------------------------------------------------

/// The claims a [`TokenVerifier`] returns after validating an OAuth/OIDC token. Only the
/// fields this layer maps to a principal are modeled here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaims {
    /// The `sub` claim — the stable subject identifier.
    pub subject: String,
    /// Optional human-facing name (`preferred_username` / `name`).
    pub preferred_username: Option<String>,
    /// The `iss` claim — the issuer that minted the token.
    pub issuer: String,
}

/// Verifies an OAuth/OIDC token and returns its [`VerifiedClaims`], or an error string on any
/// failure (bad signature, expiry, malformed). Injected so the concrete JWKS/JWT/HTTP
/// machinery lives outside this core crate — a fake verifier drives the unit tests, and a
/// real one is wired at integration (CPE-820).
pub trait TokenVerifier: Send + Sync {
    fn verify(&self, token: &str) -> Result<VerifiedClaims, String>;
}

/// Authenticates a request by verifying an OAuth/OIDC bearer token through a [`TokenVerifier`]
/// and mapping the verified claims to a principal. Abstains when no bearer token is presented;
/// denies when a token is presented but fails verification or comes from an untrusted issuer.
pub struct OidcAuthenticator {
    name: String,
    token_attribute: String,
    verifier: Arc<dyn TokenVerifier>,
    /// When `Some`, only tokens from these issuers are accepted.
    accepted_issuers: Option<Vec<String>>,
}

impl OidcAuthenticator {
    /// Build with a token verifier, reading the bearer token from [`ATTR_BEARER`].
    pub fn new(verifier: Arc<dyn TokenVerifier>) -> Self {
        Self {
            name: "oidc".to_string(),
            token_attribute: ATTR_BEARER.to_string(),
            verifier,
            accepted_issuers: None,
        }
    }

    /// Restrict accepted tokens to the given issuers (`iss` allowlist).
    pub fn with_accepted_issuers(mut self, issuers: Vec<String>) -> Self {
        self.accepted_issuers = Some(issuers);
        self
    }

    /// Override which context attribute carries the presented bearer token.
    pub fn with_token_attribute(mut self, attribute: impl Into<String>) -> Self {
        self.token_attribute = attribute.into();
        self
    }
}

impl Authenticator for OidcAuthenticator {
    fn name(&self) -> &str {
        &self.name
    }

    fn authenticate(&self, ctx: &mut SecurityContext) -> Verdict {
        let token = match ctx.attributes.get(&self.token_attribute) {
            Some(t) => t.clone(),
            None => return Verdict::Abstain, // no bearer token on this request
        };
        let claims = match self.verifier.verify(&token) {
            Ok(c) => c,
            Err(e) => return Verdict::deny(format!("oidc: {e}")),
        };
        if let Some(issuers) = &self.accepted_issuers {
            if !issuers.iter().any(|i| i == &claims.issuer) {
                return Verdict::deny("oidc: untrusted issuer");
            }
        }
        ctx.principal = Principal {
            id: claims.subject,
            display_name: claims.preferred_username,
        };
        Verdict::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AuditSink, CombinePolicy, Decision, MemoryAudit, PlaneConfig, Plane, ProviderRegistry,
        SecurityConfig, PASSTHROUGH,
    };
    use std::sync::Arc;

    fn principal(id: &str) -> Principal {
        Principal {
            id: id.into(),
            display_name: None,
        }
    }

    fn token_table() -> BTreeMap<String, Principal> {
        let mut m = BTreeMap::new();
        m.insert("secret-abc".to_string(), principal("alice"));
        m
    }

    // ---- API token ----

    #[test]
    fn api_token_allows_known_denies_unknown_abstains_absent() {
        let a = ApiTokenAuthenticator::new(token_table());

        // known token → allow + principal set
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_API_TOKEN, "secret-abc");
        assert_eq!(a.authenticate(&mut ctx), Verdict::Allow);
        assert_eq!(ctx.principal.id, "alice");

        // unknown token → deny
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_API_TOKEN, "nope");
        assert!(matches!(a.authenticate(&mut ctx), Verdict::Deny(_)));

        // no token → abstain (lets any-passes try another provider)
        let mut ctx = SecurityContext::new(Principal::local(), "op");
        assert_eq!(a.authenticate(&mut ctx), Verdict::Abstain);
    }

    // ---- mTLS identity ----

    #[test]
    fn mtls_allowlist_and_accept_any() {
        let mut allow = BTreeMap::new();
        allow.insert("CN=alice".to_string(), principal("alice"));
        let strict = MtlsIdentityAuthenticator::with_allowlist(allow);

        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_CLIENT_CERT_SUBJECT, "CN=alice");
        assert_eq!(strict.authenticate(&mut ctx), Verdict::Allow);
        assert_eq!(ctx.principal.id, "alice");

        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_CLIENT_CERT_SUBJECT, "CN=mallory");
        assert!(matches!(strict.authenticate(&mut ctx), Verdict::Deny(_)));

        // no client cert on the channel → abstain
        let mut ctx = SecurityContext::new(Principal::local(), "op");
        assert_eq!(strict.authenticate(&mut ctx), Verdict::Abstain);

        // accept-any maps the subject straight to a principal id
        let any = MtlsIdentityAuthenticator::accept_any_verified();
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_CLIENT_CERT_SUBJECT, "CN=carol");
        assert_eq!(any.authenticate(&mut ctx), Verdict::Allow);
        assert_eq!(ctx.principal.id, "CN=carol");
    }

    // ---- OIDC ----

    struct FakeVerifier;
    impl TokenVerifier for FakeVerifier {
        fn verify(&self, token: &str) -> Result<VerifiedClaims, String> {
            // token format: "good:<sub>:<iss>" is valid; anything else is rejected.
            // splitn(3) so an issuer URL's own colons stay in the issuer part.
            let parts: Vec<&str> = token.splitn(3, ':').collect();
            if parts.len() == 3 && parts[0] == "good" {
                Ok(VerifiedClaims {
                    subject: parts[1].to_string(),
                    preferred_username: Some(parts[1].to_string()),
                    issuer: parts[2].to_string(),
                })
            } else {
                Err("invalid token".into())
            }
        }
    }

    #[test]
    fn oidc_allows_valid_denies_invalid_and_untrusted_issuer_abstains_absent() {
        let oidc = OidcAuthenticator::new(Arc::new(FakeVerifier))
            .with_accepted_issuers(vec!["https://issuer.example".into()]);

        // valid token from a trusted issuer → allow + principal from claims
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_BEARER, "good:bob:https://issuer.example");
        assert_eq!(oidc.authenticate(&mut ctx), Verdict::Allow);
        assert_eq!(ctx.principal.id, "bob");
        assert_eq!(ctx.principal.display_name.as_deref(), Some("bob"));

        // valid token but untrusted issuer → deny
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_BEARER, "good:bob:https://evil.example");
        assert!(matches!(oidc.authenticate(&mut ctx), Verdict::Deny(_)));

        // malformed token → deny
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_BEARER, "garbage");
        assert!(matches!(oidc.authenticate(&mut ctx), Verdict::Deny(_)));

        // no bearer → abstain
        let mut ctx = SecurityContext::new(Principal::local(), "op");
        assert_eq!(oidc.authenticate(&mut ctx), Verdict::Abstain);
    }

    // ---- any-passes composition (the headline AC) ----

    fn any_passes_chain() -> (crate::SecurityChain, Arc<MemoryAudit>) {
        let mut reg = ProviderRegistry::with_builtins();
        reg.register_authn("api_token", || {
            Box::new(ApiTokenAuthenticator::new(token_table()))
        });
        reg.register_authn("oidc", || {
            Box::new(
                OidcAuthenticator::new(Arc::new(FakeVerifier))
                    .with_accepted_issuers(vec!["https://issuer.example".into()]),
            )
        });
        let config = SecurityConfig {
            transport: PlaneConfig {
                policy: CombinePolicy::FirstMatch,
                providers: vec![PASSTHROUGH.into()],
            },
            authentication: PlaneConfig {
                // Accept a token OR an OIDC identity: any-passes over both providers.
                policy: CombinePolicy::AnyPasses,
                providers: vec!["api_token".into(), "oidc".into()],
            },
            authorization: PlaneConfig {
                policy: CombinePolicy::FirstMatch,
                providers: vec![PASSTHROUGH.into()],
            },
        };
        let audit = Arc::new(MemoryAudit::new());
        let audit_clone = audit.clone();
        struct ArcAudit(Arc<MemoryAudit>);
        impl AuditSink for ArcAudit {
            fn record(&self, d: &crate::AuditDecision) {
                self.0.record(d);
            }
        }
        let chain = reg
            .build(&config, Box::new(ArcAudit(audit_clone)))
            .unwrap();
        (chain, audit)
    }

    #[test]
    fn any_passes_authenticates_with_either_credential() {
        // Only an API token presented → authenticates as alice.
        let (chain, _) = any_passes_chain();
        let mut ctx = SecurityContext::new(Principal::local(), "list_dir")
            .with_attribute(ATTR_API_TOKEN, "secret-abc");
        match chain.evaluate(&mut ctx) {
            Decision::Allow(p) => assert_eq!(p.id, "alice"),
            other => panic!("token should authenticate: {other:?}"),
        }

        // Only an OIDC bearer presented → authenticates as bob.
        let (chain, _) = any_passes_chain();
        let mut ctx = SecurityContext::new(Principal::local(), "list_dir")
            .with_attribute(ATTR_BEARER, "good:bob:https://issuer.example");
        match chain.evaluate(&mut ctx) {
            Decision::Allow(p) => assert_eq!(p.id, "bob"),
            other => panic!("oidc should authenticate: {other:?}"),
        }
    }

    #[test]
    fn no_credential_is_denied_by_default_and_audited() {
        let (chain, audit) = any_passes_chain();
        let mut ctx = SecurityContext::new(Principal::local(), "delete");
        match chain.evaluate(&mut ctx) {
            Decision::Deny(d) => assert_eq!(d.plane, Plane::Authentication),
            other => panic!("no credential must deny: {other:?}"),
        }
        // Failure is audited.
        assert_eq!(audit.len(), 1);
        assert!(!audit.decisions()[0].allowed);
        assert_eq!(audit.decisions()[0].plane, Some(Plane::Authentication));
    }

    #[test]
    fn ct_eq_matches_only_equal_bytes() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"", b"x"));
    }

    #[test]
    fn ct_eq_does_not_alias_lengths_differing_by_a_multiple_of_256() {
        // Regression: the old `(a.len() ^ b.len()) as u8` truncation made a 256-length delta
        // vanish, so a short token + zero-padding wrongly compared equal. `"A"` vs `"A" + 256
        // zero bytes` (len 1 vs 257, 1 ^ 257 = 256 → 0 as u8) must NOT be equal.
        let mut padded = vec![b'A'];
        padded.extend(std::iter::repeat(0u8).take(256));
        assert!(!ct_eq(b"A", &padded), "length delta of 256 must not alias to equal");
        // End-to-end: with a 1-byte known token "A", presenting "A"+256 NULs must NOT authenticate
        // (against the old truncation it aliased to a match).
        let mut table = BTreeMap::new();
        table.insert("A".to_string(), principal("alice"));
        let a = ApiTokenAuthenticator::new(table);
        let mut ctx = SecurityContext::new(Principal::local(), "op")
            .with_attribute(ATTR_API_TOKEN, String::from_utf8(padded).unwrap());
        assert!(matches!(a.authenticate(&mut ctx), Verdict::Deny(_)), "padded token must not authenticate");
    }
}
