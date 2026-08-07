//! JWT preview decoder (CPE-1418, epic CPE-1417 "Crypto/security file viewers"): a read-only VIEWER for
//! JSON Web Tokens — split the 3 dot-separated segments, base64url-decode the header + payload, and
//! surface them as pretty JSON plus a few humanized well-known claims (`exp`/`iat`/`nbf`).
//!
//! **This is NOT the security-crate verifier.** It never checks a signature, never trusts a claim, and
//! never needs a key — it just decodes what's already in the token so a user can *look at* it, exactly
//! like `binary_preview::pe_info` decodes a PE header without executing anything. Keeping it out of the
//! security crate (which does real signature verification elsewhere) avoids any risk of the two being
//! confused for one another.
//!
//! Reuses the crate's existing `base64` + `serde_json` dependencies — no JWT crate, the split+decode is
//! hand-rolled (it's three lines of work: `split('.')`, base64url-decode, `serde_json::from_slice`).
//!
//! Never panics: malformed input (wrong segment count, bad base64, bad JSON, a non-object payload) is
//! reported via [`JwtPreview::error`] rather than an `Err`/`Option`, so the preview pane can always render
//! *something* — a header-only preview when only the payload is broken, for instance — rather than an
//! all-or-nothing failure. Covered by this module's own unit tests and wired into the crate's panic-safety
//! battery (`tests/parser_panic_safety.rs`).

use serde::{Deserialize, Serialize};

/// A decoded `exp`/`iat`/`nbf` claim: the raw Unix-epoch seconds plus a humanized UTC timestamp.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JwtClaimTime {
    /// Raw claim value, seconds since the Unix epoch (as declared — may be negative or absurdly large;
    /// [`unix_to_rfc3339`] renders whatever it's given).
    pub raw: i64,
    /// `raw` rendered as an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
    pub rfc3339: String,
}

/// A JWT preview: header/payload decoded to pretty JSON plus a few humanized well-known claims. Every
/// field is optional except `signature_present`/`signature_len` (always computable) — a malformed token
/// still returns as much as could be decoded, with `error` describing what wasn't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JwtPreview {
    /// Header `alg` (e.g. `"HS256"`, `"none"`).
    pub alg: Option<String>,
    /// Header `typ` (e.g. `"JWT"`).
    pub typ: Option<String>,
    /// Header `kid`, if present.
    pub kid: Option<String>,
    /// The full header, pretty-printed JSON.
    pub header_json: Option<String>,
    /// The full payload (every claim, not just the well-known ones below), pretty-printed JSON.
    pub payload_json: Option<String>,
    /// The `exp` claim, humanized.
    pub exp: Option<JwtClaimTime>,
    /// The `iat` claim, humanized.
    pub iat: Option<JwtClaimTime>,
    /// The `nbf` claim, humanized.
    pub nbf: Option<JwtClaimTime>,
    /// `true` if `exp` is present and is in the past (compared to wall-clock time when decoded).
    pub expired: Option<bool>,
    /// `true` if `nbf` is present and is in the future (compared to wall-clock time when decoded).
    pub not_yet_valid: Option<bool>,
    /// `false` for an unsigned token (`alg: none`, empty signature segment) or a malformed one.
    pub signature_present: bool,
    /// Decoded byte length of the signature segment (0 if absent/malformed).
    pub signature_len: usize,
    /// Set when some part of the token couldn't be decoded. Other fields still carry whatever *could*
    /// be decoded (e.g. a broken payload still leaves the header fields populated).
    pub error: Option<String>,
}

/// Reject a segment before even attempting to base64-decode it, if it's implausibly large — a single
/// dot-segment this big can only be adversarial (real JWT headers/payloads are bytes to low-kilobytes),
/// and decoding + `serde_json` parsing a many-megabyte segment is needless work for a *preview*. 8 MiB of
/// base64url comfortably covers any legitimate JWT (even ones embedding a small certificate chain in
/// `x5c`) while capping the worst case.
const MAX_SEGMENT_CHARS: usize = 8 * 1024 * 1024;

/// Decode a JWT for preview purposes: split the 3 dot-separated segments, base64url-decode the header +
/// payload, parse each as JSON, and surface the result. Never panics — every failure mode (wrong segment
/// count, bad base64, bad JSON, a non-object header/payload) is reported via [`JwtPreview::error`].
pub fn jwt_preview(token: &str) -> JwtPreview {
    let token = token.trim();
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return JwtPreview {
            error: Some(format!(
                "malformed JWT: expected 3 dot-separated segments (header.payload.signature), found {}",
                parts.len()
            )),
            ..Default::default()
        };
    }

    let mut out = JwtPreview::default();
    let mut errors: Vec<String> = Vec::new();

    match decode_segment_object(parts[0]) {
        Ok(header) => {
            out.alg = header.get("alg").and_then(|v| v.as_str()).map(str::to_string);
            out.typ = header.get("typ").and_then(|v| v.as_str()).map(str::to_string);
            out.kid = header.get("kid").and_then(|v| v.as_str()).map(str::to_string);
            out.header_json = serde_json::to_string_pretty(&header).ok();
        }
        Err(e) => errors.push(format!("header: {e}")),
    }

    match decode_segment_object(parts[1]) {
        Ok(payload) => {
            out.payload_json = serde_json::to_string_pretty(&payload).ok();
            let now = now_unix();
            if let Some(exp) = json_number_to_i64(payload.get("exp")) {
                out.expired = Some(now > exp);
                out.exp = Some(JwtClaimTime { raw: exp, rfc3339: unix_to_rfc3339(exp) });
            }
            if let Some(iat) = json_number_to_i64(payload.get("iat")) {
                out.iat = Some(JwtClaimTime { raw: iat, rfc3339: unix_to_rfc3339(iat) });
            }
            if let Some(nbf) = json_number_to_i64(payload.get("nbf")) {
                out.not_yet_valid = Some(now < nbf);
                out.nbf = Some(JwtClaimTime { raw: nbf, rfc3339: unix_to_rfc3339(nbf) });
            }
        }
        Err(e) => errors.push(format!("payload: {e}")),
    }

    let sig = parts[2];
    out.signature_present = !sig.is_empty();
    out.signature_len = if sig.len() > MAX_SEGMENT_CHARS { 0 } else { base64_url_decode(sig).map(|b| b.len()).unwrap_or(0) };

    if !errors.is_empty() {
        out.error = Some(errors.join("; "));
    }
    out
}

/// Base64url-decode one dot-segment and parse it as a JSON object. `Err` on anything that isn't a
/// well-formed base64url-encoded JSON object — oversized input, invalid base64, invalid UTF-8/JSON, or
/// valid JSON that isn't an object (JWT header/payload are always objects per RFC 7519).
fn decode_segment_object(segment: &str) -> Result<serde_json::Value, String> {
    if segment.len() > MAX_SEGMENT_CHARS {
        return Err(format!("segment too large ({} chars, cap {MAX_SEGMENT_CHARS})", segment.len()));
    }
    let bytes = base64_url_decode(segment).map_err(|e| format!("base64url decode failed: {e}"))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON: {e}"))?;
    if !value.is_object() {
        return Err("expected a JSON object".to_string());
    }
    Ok(value)
}

fn base64_url_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.decode(s)
}

/// A JSON number claim (`exp`/`iat`/`nbf`) as `i64` — JWT claims are technically `NumericDate` (a JSON
/// number, per RFC 7519 §2), so accept both integer and float encodings, truncating a float toward zero.
fn json_number_to_i64(v: Option<&serde_json::Value>) -> Option<i64> {
    let v = v?;
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

use crate::fsutil::unix_to_rfc3339;

#[cfg(test)]
mod tests {
    use super::*;

    fn b64url(bytes: &[u8]) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn make_token(header: &str, payload: &str, sig: &[u8]) -> String {
        format!("{}.{}.{}", b64url(header.as_bytes()), b64url(payload.as_bytes()), b64url(sig))
    }

    #[test]
    fn epoch_claim_renders_as_rfc3339() {
        // unix_to_rfc3339's own correctness is covered by fsutil.rs's unit tests; here just confirm
        // jwt_preview wires an `exp`/`iat`/`nbf` claim through it.
        let token = make_token(r#"{"alg":"HS256"}"#, r#"{"iat":0}"#, b"sig");
        let p = jwt_preview(&token);
        assert_eq!(p.iat.unwrap().rfc3339, "1970-01-01T00:00:00Z");
    }

    #[test]
    fn valid_hs256_token_decodes_header_and_payload() {
        let token = make_token(
            r#"{"alg":"HS256","typ":"JWT"}"#,
            r#"{"sub":"1234567890","name":"Ann","iat":1700000000}"#,
            b"fake-signature-bytes",
        );
        let p = jwt_preview(&token);
        assert_eq!(p.alg.as_deref(), Some("HS256"));
        assert_eq!(p.typ.as_deref(), Some("JWT"));
        assert!(p.kid.is_none());
        assert!(p.header_json.as_deref().unwrap().contains("HS256"));
        assert!(p.payload_json.as_deref().unwrap().contains("Ann"));
        assert_eq!(p.iat.as_ref().unwrap().raw, 1_700_000_000);
        assert!(p.signature_present);
        assert_eq!(p.signature_len, b"fake-signature-bytes".len());
        assert!(p.error.is_none(), "well-formed token must not report an error: {:?}", p.error);
    }

    #[test]
    fn expired_token_flags_expired_true() {
        let token = make_token(r#"{"alg":"HS256"}"#, r#"{"exp":1000000000}"#, b"sig");
        let p = jwt_preview(&token);
        assert_eq!(p.expired, Some(true), "exp far in the past must be flagged expired");
        assert_eq!(p.exp.unwrap().raw, 1_000_000_000);
    }

    #[test]
    fn not_yet_valid_token_flags_not_yet_valid_true() {
        // nbf far in the future (year ~2200).
        let token = make_token(r#"{"alg":"HS256"}"#, r#"{"nbf":7258118400}"#, b"sig");
        let p = jwt_preview(&token);
        assert_eq!(p.not_yet_valid, Some(true));
    }

    #[test]
    fn alg_none_token_has_no_signature() {
        let token = make_token(r#"{"alg":"none","typ":"JWT"}"#, r#"{"sub":"x"}"#, b"");
        let p = jwt_preview(&token);
        assert_eq!(p.alg.as_deref(), Some("none"));
        assert!(!p.signature_present, "empty signature segment must report signature_present=false");
        assert_eq!(p.signature_len, 0);
        assert!(p.error.is_none());
    }

    #[test]
    fn two_segment_token_is_reported_malformed_not_panicked() {
        let p = jwt_preview("only.two");
        assert!(p.error.is_some());
        assert!(p.alg.is_none());
        assert!(!p.signature_present);
    }

    #[test]
    fn empty_string_is_reported_malformed() {
        let p = jwt_preview("");
        assert!(p.error.is_some());
    }

    #[test]
    fn garbage_bytes_never_panics_and_reports_an_error() {
        let p = jwt_preview("not-a-jwt-at-all-just.some-garbage!!.###");
        assert!(p.error.is_some());
        // Garbage may still coincidentally decode to *something* for header/payload text is irrelevant —
        // the only real assertion here is "did not panic", which returning at all already proves.
    }

    #[test]
    fn huge_payload_never_panics_and_still_decodes() {
        // A legitimately large-but-under-cap payload: many claims, ~200KB of JSON.
        let mut payload = String::from("{");
        for i in 0..8000 {
            if i > 0 {
                payload.push(',');
            }
            payload.push_str(&format!(r#""claim{i}":"value-{i}""#));
        }
        payload.push('}');
        let token = make_token(r#"{"alg":"HS256"}"#, &payload, b"sig");
        let p = jwt_preview(&token);
        assert!(p.error.is_none(), "large-but-legitimate payload must still decode: {:?}", p.error);
        assert!(p.payload_json.is_some());
    }

    #[test]
    fn oversized_segment_is_rejected_without_decoding() {
        let huge = "A".repeat(MAX_SEGMENT_CHARS + 1);
        let token = format!("{huge}.{huge}.sig");
        let p = jwt_preview(&token);
        assert!(p.error.is_some());
        assert!(p.header_json.is_none());
    }

    #[test]
    fn non_object_json_payload_is_rejected() {
        // Payload decodes to a JSON array, not an object — not a valid JWT payload shape.
        let token = make_token(r#"{"alg":"HS256"}"#, "[1,2,3]", b"sig");
        let p = jwt_preview(&token);
        assert!(p.error.is_some());
        assert!(p.alg.as_deref() == Some("HS256"), "header still decodes even though payload is invalid");
    }
}
