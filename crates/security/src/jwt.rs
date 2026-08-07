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

/// Adversarial fuzz/property battery for `HmacJwtVerifier::verify` (CPE-1399). `verify` parses a
/// **fully attacker-controlled** bearer token (base64 → JSON → HMAC chain) at the security boundary, so
/// this battery proves the two properties that matter for a hostile input: it never panics, and it never
/// accepts anything short of a genuinely HMAC-signed, well-formed token. Test-only — does not touch
/// `verify`'s production logic.
///
/// Follows the table-driven `catch_unwind` pattern from `crates/server/tests/parser_panic_safety.rs`
/// (inlined here rather than shared, since this crate has no `tests/` integration-test convention yet —
/// its existing tests all live in `#[cfg(test)] mod tests` blocks like the one above). Uses a tiny seeded
/// LCG for "random bytes" instead of the `rand` crate, so the battery adds no new dependency and stays
/// deterministic across runs/machines.
#[cfg(test)]
#[cfg(feature = "jwt")]
mod adversarial_battery {
    use super::*;
    use serde_json::json;
    use std::panic::{self, AssertUnwindSafe};

    const SECRET: &[u8] = b"super-secret-signing-key-cpe1399-battery";

    // -----------------------------------------------------------------------------------------
    // Small local helpers (deliberately duplicated from the `tests` module above rather than
    // shared, so this battery reads standalone and doesn't need to expose test-only helpers as
    // `pub(crate)` off the production module).
    // -----------------------------------------------------------------------------------------

    fn enc(b: &[u8]) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
    }

    /// Mint a genuinely-signed HS256 JWT over a caller-supplied header — the only way to reach the
    /// payload-parsing code past the HMAC gate, and how the positive control proves the battery isn't
    /// trivially passing by rejecting everything.
    fn mint_with_header(secret: &[u8], header_json: &[u8], claims: Value) -> String {
        let header = enc(header_json);
        let payload = enc(&serde_json::to_vec(&claims).unwrap());
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(format!("{header}.{payload}").as_bytes());
        let sig = enc(&mac.finalize().into_bytes());
        format!("{header}.{payload}.{sig}")
    }

    /// Mint with the standard `{"alg":"HS256","typ":"JWT"}` header.
    fn mint(secret: &[u8], claims: Value) -> String {
        mint_with_header(secret, br#"{"alg":"HS256","typ":"JWT"}"#, claims)
    }

    fn split3(tok: &str) -> (&str, &str, &str) {
        let mut it = tok.split('.');
        (it.next().unwrap(), it.next().unwrap(), it.next().unwrap())
    }

    /// Flip the last character of a base64url segment to a different valid base64url character, so
    /// tampering is "one byte changed" rather than "shape destroyed".
    fn flip_one_char(s: &str) -> String {
        let mut chars: Vec<char> = s.chars().collect();
        if let Some(last) = chars.last_mut() {
            *last = if *last == 'A' { 'B' } else { 'A' };
        }
        chars.into_iter().collect()
    }

    /// A tiny seeded linear-congruential generator (Numerical Recipes' constants) — NOT
    /// cryptographic, just a reproducible byte stream so "random bytes" is deterministic across
    /// runs/machines without pulling in the `rand` crate (no new dependency). Mirrors
    /// `crates/server/tests/common/mod.rs::lcg_bytes`.
    fn lcg_bytes(seed: u64, len: usize) -> Vec<u8> {
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) as u8
            })
            .collect()
    }

    /// Run `f` under `catch_unwind`; any panic — the verifier's own, or an assertion failing inside
    /// `f` — is re-raised naming the adversarial case that triggered it.
    fn assert_no_panic(label: &str, f: impl FnOnce()) {
        if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(f)) {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            panic!("HmacJwtVerifier::verify panicked on adversarial case `{label}`: {msg}");
        }
    }

    // -----------------------------------------------------------------------------------------
    // Positive control — proves the battery below isn't trivially passing by rejecting everything.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn positive_control_a_genuinely_valid_token_verifies_ok() {
        let tok = mint(SECRET, json!({ "sub": "dave", "iss": "https://issuer", "aud": "cpe", "exp": 2_000_000u64 }));
        let v = HmacJwtVerifier::new(SECRET).with_clock(|| 1_000_000).with_audience("cpe");
        let claims = v.verify(&tok).expect("a genuinely HMAC-signed, well-formed token must verify");
        assert_eq!(claims.subject, "dave");
        assert_eq!(claims.issuer, "https://issuer");
    }

    // -----------------------------------------------------------------------------------------
    // Never panics — empty/random/malformed/huge/deeply-nested input.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn never_panics_on_adversarial_tokens() {
        let v = HmacJwtVerifier::new(SECRET).with_clock(|| 1_000_000_000);
        // Fixed cases first: empty/dot-shape/garbage-base64. Wrong segment counts covered: 0 dots, 1
        // dot, (2 dots is the well-shaped-but-garbage case), 3 dots, 4 dots.
        let mut cases: Vec<(String, String)> = vec![
            ("empty".into(), String::new()),
            ("just_a_dot".into(), ".".into()),
            ("many_dots".into(), ".".repeat(50)),
            ("zero_dots".into(), "justsomerandomtoken".into()),
            ("one_dot".into(), "abc.def".into()),
            ("three_dots".into(), "a.b.c.d".into()),
            ("four_dots".into(), "a.b.c.d.e".into()),
            ("two_dots_garbage".into(), "abc.def.ghi".into()),
            ("bad_base64_chars".into(), "!!!.@@@.###".into()),
            ("base64_with_padding_rejected".into(), "YQ==.YQ==.YQ==".into()),
        ];

        // Random raw bytes lossily converted to `str` (simulates attacker-controlled bytes hitting a
        // text boundary), at sizes from 0 up to 1 MB.
        for &n in &[0usize, 1, 2, 8, 64, 1024, 64 * 1024, 1024 * 1024] {
            let raw = lcg_bytes(0x00C0_FFEE ^ n as u64, n);
            let lossy = String::from_utf8_lossy(&raw).into_owned();
            cases.push((format!("random_bytes_lossy_utf8_{n}"), lossy));
        }

        // Well-shaped (3 segments), each segment independently random base64-encoded garbage, at
        // sizes up to 1 MB per segment (covers "huge header" / "huge payload" / "huge signature").
        for &n in &[0usize, 1, 16, 256, 4096, 1024 * 1024] {
            let h = enc(&lcg_bytes(0xAAAA ^ n as u64, n));
            let p = enc(&lcg_bytes(0xBBBB ^ n as u64, n));
            let s = enc(&lcg_bytes(0xCCCC ^ n as u64, n));
            cases.push((format!("random_segments_{n}"), format!("{h}.{p}.{s}")));
        }

        // Valid base64, odd/invalid JSON payloads in each position.
        for bad_json in [
            &b""[..],
            b"not json",
            b"[1,2,3]",
            b"\"just a string\"",
            b"12345",
            b"null",
            b"true",
            b"{",
        ] {
            let bad = enc(bad_json);
            cases.push((format!("bad_header_json_{}b", bad_json.len()), format!("{bad}.{}.{}", enc(b"{}"), enc(b"sig"))));
            cases.push((
                format!("bad_payload_json_{}b", bad_json.len()),
                format!("{}.{bad}.{}", enc(br#"{"alg":"HS256"}"#), enc(b"sig")),
            ));
        }

        // Deeply-nested JSON header — reached regardless of signature validity, since the header is
        // parsed before the HMAC check. Depth kept conservative (well below any realistic stack-depth
        // for a legitimate header) because `cargo test` runs each test on its own thread with a small
        // (often ~2 MB) stack, and debug-build recursive-parser stack frames are larger than release.
        let mut nested = json!({"alg": "HS256"});
        for _ in 0..500 {
            nested = json!({ "nest": nested });
        }
        let nested_header = enc(&serde_json::to_vec(&nested).unwrap());
        cases.push(("deeply_nested_header".into(), format!("{nested_header}.{}.{}", enc(b"{}"), enc(b"sig"))));

        for (label, token) in &cases {
            assert_no_panic(label, || {
                let _ = v.verify(token);
            });
        }

        // Deeply-nested *payload* — only reachable past the HMAC gate, so it must be genuinely signed.
        let mut nested_claims = json!({"sub": "x"});
        for _ in 0..500 {
            nested_claims = json!({ "nest": nested_claims });
        }
        let nested_tok = mint(SECRET, nested_claims);
        assert_no_panic("deeply_nested_payload_signed", || {
            let _ = v.verify(&nested_tok);
        });

        // Genuinely-signed huge (multi-MB) payload — must not panic; since it's a real signature over
        // real (if bulky) claims, it should also still verify (proves a legitimate large token isn't
        // itself a panic/DoS vector).
        let huge_claims = json!({ "sub": "bulk", "pad": "a".repeat(3 * 1024 * 1024) });
        let huge_tok = mint(SECRET, huge_claims);
        assert_no_panic("huge_signed_payload_multi_mb", || {
            let r = v.verify(&huge_tok);
            assert!(r.is_ok(), "a genuinely-signed multi-MB payload should still verify: {r:?}");
        });
    }

    // -----------------------------------------------------------------------------------------
    // Never accepts a forged/tampered token.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn never_accepts_a_forged_or_tampered_token() {
        let clock = || 1_000_000_000u64;
        let v = HmacJwtVerifier::new(SECRET).with_clock(clock);

        let valid = mint(SECRET, json!({ "sub": "carol", "iss": "https://issuer" }));
        let (h, p, s) = split3(&valid);

        let tamper_cases: Vec<(&str, String)> = vec![
            ("tampered_payload_byte", format!("{h}.{}.{s}", flip_one_char(p))),
            ("tampered_header_byte", format!("{}.{p}.{s}", flip_one_char(h))),
            ("garbage_signature_same_length", format!("{h}.{p}.{}", enc(&[0u8; 32]))),
            ("empty_signature", format!("{h}.{p}.")),
            ("truncated_signature", format!("{h}.{p}.{}", &s[..s.len() / 2])),
        ];
        for (label, token) in &tamper_cases {
            let r = v.verify(token);
            assert!(r.is_err(), "forged/tampered token unexpectedly verified OK: case=`{label}`");
        }

        // Right shape, wrong key.
        let wrong_keys: [&[u8]; 5] = [b"", b"x", b"other-key", b"super-secret-signing-key-cpe1399-battery-X", &[0u8; 64]];
        for wrong_key in wrong_keys {
            let r = HmacJwtVerifier::new(wrong_key.to_vec()).with_clock(clock).verify(&valid);
            assert!(r.is_err(), "token verified under the wrong key ({} bytes)", wrong_key.len());
        }

        // `alg` downgrade / confusion attempts — even paired with a real HMAC signature computed over
        // that exact header+payload, so the failure is provably the alg gate, not an incidental bad
        // signature.
        let alg_variants: [&[u8]; 8] = [
            br#"{"alg":"none"}"#,
            br#"{"alg":"None"}"#,
            br#"{"alg":"NONE"}"#,
            br#"{"alg":"hs256"}"#, // wrong case must not match
            br#"{"alg":"RS256"}"#,
            br#"{}"#,             // missing alg entirely
            br#"{"alg":256}"#,    // alg as a non-string
            br#"{"alg":null}"#,
        ];
        for header_json in alg_variants {
            let header = enc(header_json);
            let payload = enc(br#"{"sub":"eve"}"#);
            let mut mac = HmacSha256::new_from_slice(SECRET).unwrap();
            mac.update(format!("{header}.{payload}").as_bytes());
            let sig = enc(&mac.finalize().into_bytes());
            let tok = format!("{header}.{payload}.{sig}");
            let r = HmacJwtVerifier::new(SECRET).with_clock(clock).verify(&tok);
            assert!(
                r.is_err(),
                "alg-downgrade token unexpectedly verified: header={}",
                String::from_utf8_lossy(header_json)
            );
        }
        // The classic exploit shape: `alg:none` with an empty signature (no HMAC computed at all).
        let none_tok = format!("{}.{}.", enc(br#"{"alg":"none"}"#), enc(br#"{"sub":"eve"}"#));
        assert!(v.verify(&none_tok).is_err(), "alg:none with empty signature unexpectedly verified");

        // Expired (exp far in the past, beyond leeway).
        let expired = mint(SECRET, json!({ "sub": "a", "exp": 1u64 }));
        assert!(v.verify(&expired).is_err(), "expired token unexpectedly verified");

        // Not yet valid (nbf far in the future).
        let not_yet = mint(SECRET, json!({ "sub": "a", "nbf": 9_999_999_999u64 }));
        assert!(v.verify(&not_yet).is_err(), "not-yet-valid token unexpectedly verified");

        // Missing / wrong-typed `sub`.
        let no_sub = mint(SECRET, json!({ "iss": "https://issuer" }));
        assert!(v.verify(&no_sub).is_err(), "token missing `sub` unexpectedly verified");
        let numeric_sub = mint(SECRET, json!({ "sub": 12345 }));
        assert!(v.verify(&numeric_sub).is_err(), "token with non-string `sub` unexpectedly verified");

        // Cut-and-paste / signature-confusion: splice the header of one validly-signed token with the
        // payload of another, keeping either original signature. The two tokens are minted with
        // deliberately *distinguishable* headers (a "kid"-style extra field) so `ha != hb` — otherwise,
        // since `mint`'s header is byte-identical across calls, `ha.pb.sb` would just reconstruct
        // `tok_b` verbatim (a real token, not a forgery) rather than testing a genuine splice. With
        // distinct headers, neither signature covers this new header.payload combination, so both
        // splices must fail.
        let tok_a = mint_with_header(SECRET, br#"{"alg":"HS256","typ":"JWT","kid":"a"}"#, json!({ "sub": "alice" }));
        let tok_b =
            mint_with_header(SECRET, br#"{"alg":"HS256","typ":"JWT","kid":"b"}"#, json!({ "sub": "bob", "admin": true }));
        let (ha, _pa, sa) = split3(&tok_a);
        let (hb, pb, sb) = split3(&tok_b);
        assert_ne!(ha, hb, "test setup bug: splice headers must differ for this to be a genuine forgery attempt");
        assert!(v.verify(&format!("{ha}.{pb}.{sa}")).is_err(), "spliced token (sig A) unexpectedly verified");
        assert!(v.verify(&format!("{ha}.{pb}.{sb}")).is_err(), "spliced token (sig B) unexpectedly verified");
    }
}
