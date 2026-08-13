//! AWS Signature Version 4 (`AWS4-HMAC-SHA256`) request signing, hand-rolled over `hmac` + `sha2`
//! (CPE-1681, epic CPE-1503).
//!
//! SigV4 is four steps, and this module exposes all four rather than only the last one, because that is
//! what makes it *checkable*: AWS publishes fixed-input/fixed-output test vectors for each intermediate
//! stage, and [`SignedRequest`] carries the canonical request and the string-to-sign alongside the final
//! `Authorization` header so a test can assert every stage byte for byte instead of only the end result.
//! A signer that only emits the final header can be wrong in two places that cancel out; one that matches
//! at every stage cannot.
//!
//! 1. **Canonical request** - method, URI, query, headers, signed-header list, payload hash.
//! 2. **String to sign** - algorithm, timestamp, credential scope, SHA-256 of (1).
//! 3. **Signing key** - `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), service), "aws4_request")`.
//! 4. **Signature / `Authorization`** - `HMAC(signing key, (2))`, hex-encoded.
//!
//! # The encoding rule, spelled out
//! This is where hand-rolled SigV4 traditionally goes wrong, so it is stated rather than left implicit:
//!
//! - The **canonical URI** is the request path percent-encoded **once** for S3, using [`encode_path`].
//!   Most AWS services encode the path *twice* and normalize `.`/`..` segments; **S3 does neither** - an
//!   S3 key is an opaque byte string, so `a//b` and `a/../b` are distinct keys and must survive signing
//!   unchanged. Encoding twice here (or normalizing) produces `SignatureDoesNotMatch` against every S3
//!   implementation.
//! - The **canonical query string** encodes key and value with [`encode_query_component`], which - unlike
//!   the path encoder - also escapes `/` as `%2F`, then sorts by encoded key (then encoded value).
//! - Both encoders escape every byte outside the RFC 3986 *unreserved* set (`A-Z a-z 0-9 - . _ ~`);
//!   notably `+` is **not** a space and must reach the wire as `%2B`.
//!
//! Because the canonical URI must match the path actually sent byte for byte, the caller hands this
//! module a path that is **already encoded** - by the very same [`encode_path`] the URL builder in
//! [`crate::S3Config`] uses. One encoder, one output, used on both sides: they cannot drift.
//!
//! The **query** has the same requirement and the same answer: [`canonical_query`] is public, and
//! [`crate::RequestTarget::url_with_query`] builds the on-wire URL from it, so the request layer never
//! has to re-implement the encode-and-sort rule and then drift from what was signed (CPE-1689).

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::Credentials;

type HmacSha256 = Hmac<Sha256>;

/// The only algorithm this module implements (the SigV4 `Algorithm` line and `Authorization` prefix).
pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// The trailing element of every SigV4 credential scope.
pub const SCOPE_TERMINATOR: &str = "aws4_request";

/// `hex(sha256(b""))` - the `x-amz-content-sha256` value for a request with no body (GET/HEAD/DELETE).
pub const EMPTY_PAYLOAD_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The literal S3 accepts in place of a payload hash when the body is not hashed up front. Provided for
/// the later request layer (a streaming PUT); this crate never chooses it on the caller's behalf, since
/// an unsigned payload is a real (if bounded) integrity trade-off.
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Lowercase hex, the only encoding SigV4 uses for digests and signatures.
fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// `hex(sha256(data))` - the payload hash for `x-amz-content-sha256`, and the digest of the canonical
/// request in step 2.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_lower(&hasher.finalize())
}

/// One link of the signing-key chain. `Hmac::new_from_slice` only fails for key lengths HMAC cannot
/// accept, and HMAC-SHA256 accepts every length, so the `expect` is unreachable by construction.
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 accepts keys of any length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// Derive the SigV4 signing key (step 3): the secret is stretched through one HMAC per scope element, so
/// the key that ever touches a request is scoped to a single date, region and service - the reason a
/// leaked signature is not a leaked credential.
///
/// `date` is the `YYYYMMDD` day, **not** the full timestamp; passing the timestamp yields a key that
/// derives cleanly and then fails to verify anywhere, which is the hardest kind of bug to see.
///
/// `region` and `service` are taken as given - this function has no `Result` to fail into, and cannot:
/// the output is a fixed-size HMAC digest, not a string an injected byte could restructure, so there is
/// nothing here for a bad `region`/`service` to corrupt. It is `pub` only for the published AWS test
/// vectors that pin this stage directly; every real caller reaches it through [`Signer`], which validates
/// both (CPE-1691) before this is ever called.
pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, SCOPE_TERMINATOR.as_bytes())
}

/// True for the RFC 3986 *unreserved* characters - the only bytes SigV4 leaves alone.
fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// Percent-encode `s`, escaping every byte outside the unreserved set. `keep_slash` preserves `/` as a
/// path separator (canonical **URI** rules); pass `false` for query components, where `/` is data and
/// must become `%2F`.
fn percent_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        if is_unreserved(*b) || (keep_slash && *b == b'/') {
            out.push(*b as char);
        } else {
            out.push('%');
            out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0').to_ascii_uppercase());
            out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0').to_ascii_uppercase());
        }
    }
    out
}

/// Percent-encode an S3 object key / request path, keeping `/` as the separator. Used for **both** the
/// URL actually requested and the canonical URI that is signed - see the module docs on why those must
/// be the same function.
pub fn encode_path(path: &str) -> String {
    percent_encode(path, true)
}

/// Percent-encode one query-string name or value. Escapes `/` too (`%2F`), unlike [`encode_path`].
pub fn encode_query_component(s: &str) -> String {
    percent_encode(s, false)
}

/// Build the canonical query string: encode every name and value, then sort by encoded name and, for
/// repeated names, by encoded value. A parameter with no value still contributes its `=`.
///
/// **Public because the string it returns is also the one that goes on the wire.** SigV4 signs the
/// canonical query, so a request layer that builds its own `?a=1&b=2` can encode or order a parameter
/// differently from what was signed and get an opaque `SignatureDoesNotMatch` - the exact failure the
/// single-encoder rule already prevents for the path. Callers build the URL with
/// [`crate::RequestTarget::url_with_query`], which calls this function, and pass the same `query` slice
/// to [`SigningInput::query`]: one construction, two uses, no way to drift (CPE-1689).
pub fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(k, v)| (encode_query_component(k), encode_query_component(v)))
        .collect();
    pairs.sort();
    pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

/// Normalize one header value for signing: trim the ends, and collapse each run of internal spaces/tabs
/// to a single space, uniformly.
///
/// Uniformly is not a shortcut. An earlier version of this comment claimed SigV4 exempts quoted strings
/// from the collapse and that this crate deliberately deviates; it does not, and there is no deviation.
/// The published `get-header-value-trim` vector signs `"a   b   c"` (quotes included) and its canonical
/// request carries `"a b c"` - collapsed **inside** the quotes. The claim was deleted at CPE-1689
/// because it invited a future "fix" that would break a passing vector.
///
/// The trim is SP (0x20) and HTAB (0x09) **only** - not `str::trim()`, which strips every Unicode
/// whitespace character (NBSP U+00A0, the Unicode space separators, line/paragraph separators, ...).
/// SigV4's canonicalisation rule trims only space and tab before the collapse; a header value with a
/// leading NBSP is not SP, so S3 does not trim it, and it must survive here too. A signer that trimmed it
/// anyway would canonicalise a different string than the server does for a byte the caller never asked to
/// have removed - the client and S3 sign two different requests, and the caller gets an opaque
/// `SignatureDoesNotMatch` with nothing pointing at the whitespace (CPE-1695).
fn normalize_header_value(v: &str) -> String {
    let trimmed = v.trim_matches(|c| c == ' ' || c == '\t');
    let mut out = String::with_capacity(trimmed.len());
    let mut in_space = false;
    for ch in trimmed.chars() {
        if ch == ' ' || ch == '\t' {
            in_space = true;
            continue;
        }
        if in_space && !out.is_empty() {
            out.push(' ');
        }
        in_space = false;
        out.push(ch);
    }
    out
}

/// Lowercase, normalize, sort and de-duplicate the headers to sign, returning the canonical-headers
/// block and the `;`-joined signed-header list. Repeated header names are merged into one comma-joined
/// value in the order given, per the SigV4 spec.
fn canonical_headers(headers: &[(&str, &str)]) -> (String, String) {
    let mut merged: Vec<(String, String)> = Vec::with_capacity(headers.len());
    for (name, value) in headers {
        let name = name.trim().to_ascii_lowercase();
        let value = normalize_header_value(value);
        match merged.iter_mut().find(|(n, _)| *n == name) {
            Some((_, existing)) => {
                existing.push(',');
                existing.push_str(&value);
            }
            None => merged.push((name, value)),
        }
    }
    merged.sort_by(|a, b| a.0.cmp(&b.0));
    let block = merged.iter().map(|(n, v)| format!("{n}:{v}\n")).collect::<String>();
    let signed = merged.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(";");
    (block, signed)
}

/// Everything about a single request that SigV4 covers.
///
/// `encoded_path` is the absolute request path **already percent-encoded** by [`encode_path`], exactly as
/// it will be sent; see the module docs. `headers` must include `host` - S3 rejects a signature that does
/// not cover it, and [`Signer::sign`] enforces that rather than leaving it as advice - and is signed
/// exactly as given, so a caller signs the headers that matter rather than every header the HTTP client
/// happens to add.
#[derive(Debug, Clone, Copy)]
pub struct SigningInput<'a> {
    /// Uppercase HTTP method (`GET`, `PUT`, ...). Checked, not assumed: see [`Signer::sign`].
    pub method: &'a str,
    /// Absolute, already-encoded request path (`/` for the bucket root).
    pub encoded_path: &'a str,
    /// Raw (unencoded) query parameters; this module encodes and sorts them.
    pub query: &'a [(&'a str, &'a str)],
    /// Raw headers to sign, in any order.
    pub headers: &'a [(&'a str, &'a str)],
    /// `hex(sha256(body))`, [`EMPTY_PAYLOAD_SHA256`], or [`UNSIGNED_PAYLOAD`].
    pub payload_hash: &'a str,
    /// The request timestamp in SigV4's basic-ISO-8601 form, `YYYYMMDDTHHMMSSZ`.
    pub amz_date: &'a str,
}

/// The result of signing, with every intermediate stage kept.
///
/// The intermediates are public on purpose: they are what the published AWS test vectors are expressed
/// in, so tests assert against them directly, and a live `SignatureDoesNotMatch` can be diagnosed by
/// diffing the canonical request against the one the server echoes back - which is the only practical
/// way to debug a SigV4 mismatch.
#[derive(Debug, Clone)]
pub struct SignedRequest {
    /// Step 1.
    pub canonical_request: String,
    /// Step 2.
    pub string_to_sign: String,
    /// The `date/region/service/aws4_request` credential scope.
    pub credential_scope: String,
    /// The `;`-joined lowercase header names covered by the signature.
    pub signed_headers: String,
    /// Step 4, hex.
    pub signature: String,
    /// The complete `Authorization` header value.
    pub authorization: String,
}

/// Reject `,` and `/` on top of [`crate::validate_structural_text`], for the fields that are woven
/// directly into the `Authorization` header's `Credential=` parameter or the SigV4 credential scope
/// string (`{date}/{region}/{service}/aws4_request`): `region`, `service`, and `access_key_id`.
///
/// **Deliberately NOT folded into the shared `validate_structural_text` standard** (CPE-1691, UAT
/// SEC-4): an endpoint legitimately carries `/` as a path separator, so widening the shared check would
/// refuse every endpoint with a path prefix. This layers the extra restriction on top instead, the same
/// way [`crate::validate_bucket`] layers its own extra `/`/`@`/`:` refusal on top of the shared standard —
/// one more instance of "the shared check plus this field's own reason", not a competing standard.
///
/// Both bytes are live, not theoretical: `access_key_id = "AKIA,Signature=deadbeef"` produces
/// `Authorization: ...Credential=AKIA,Signature=deadbeef/20130524/..., SignedHeaders=...` - a forged
/// `Signature=` parameter ahead of the real one, because `,` is what separates `Authorization`
/// parameters. `region = "us-east-1/s3/aws4_request"` restructures the credential-scope string itself,
/// because `/` is what separates its elements.
fn validate_credential_scope_element(kind: &str, s: &str) -> Result<(), String> {
    crate::validate_structural_text(kind, s)?;
    if let Some(ch) = s.chars().find(|c| matches!(c, ',' | '/')) {
        return Err(format!(
            "s3: {kind} must not contain ',' or '/' (they would restructure the credential scope or forge \
             an Authorization parameter), got {ch:?} in {s:?}"
        ));
    }
    Ok(())
}

/// Validate a SigV4 region: not empty, and none of the bytes [`validate_credential_scope_element`]
/// refuses.
///
/// The region becomes part of the credential scope, which is embedded verbatim in the signed
/// `Authorization` header (`Credential={key}/{date}/{region}/{service}/aws4_request`) - a control
/// character there is the identical request-splitting shape CPE-1689 closed at the endpoint, and a `/`
/// there restructures the scope string itself, just reached through a different field
/// (`region "us-east-1\r\nX-Injected: 1"` used to sign cleanly; so did `"us-east-1/s3/aws4_request"`).
///
/// **Called from both public paths that take a region** - [`crate::S3Config::object_target`] /
/// [`crate::S3Config::bucket_target`] (via their shared `target_for`) and [`Signer::new`] /
/// [`Signer::for_service`] - which is the point: before CPE-1691, `target_for` rejected an empty region
/// and `Signer::new(&creds, "")` signed happily. A validation enforced on one entry point and skipped on
/// another is barely a validation.
pub(crate) fn validate_region(region: &str) -> Result<(), String> {
    if region.trim().is_empty() {
        return Err("s3: region must not be empty (it is part of the SigV4 credential scope)".into());
    }
    validate_credential_scope_element("region", region)
}

/// A header name must be an RFC 7230 `token` - the grammar HTTP itself defines for one. Unlike a value
/// (see [`reject_framing_bytes`]) a name is structured, not free-form, so framing bytes are not the only
/// unsafe alphabet: `;` would forge an extra entry in `SignedHeaders` (`"x-evil;host"` satisfied
/// `signed_headers.split(';').any(|n| n == "host")` with **no real `host` header present at all** -
/// CPE-1691's UAT SEC-1, a full bypass of the CPE-1689 host-presence guard), `,` would forge an extra
/// `Authorization` parameter the same way [`validate_credential_scope_element`] refuses it for
/// `access_key_id`, and `:` or a space would make the canonical header line's name/value boundary
/// ambiguous (`"x evil:injected"` -> canonical line `x evil:injected:1`).
///
/// The allowed set is exactly [RFC 7230 §3.2.6](https://www.rfc-editor.org/rfc/rfc7230#section-3.2.6)'s
/// `tchar`: ASCII alphanumerics plus `` !#$%&'*+-.^_`|~ ``. Nothing outside that set can appear in a real
/// HTTP header name, so refusing everything else costs no legitimate header.
fn validate_header_name(name: &str) -> Result<(), String> {
    let n = name.trim(); // `canonical_headers` trims before lowercasing - validate what it will use.
    if n.is_empty()
        || !n.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
    {
        return Err(format!(
            "s3: header name must be an RFC 7230 token (no ':', ';', ',', space or control), got {name:?}"
        ));
    }
    Ok(())
}

/// Reject a header **value** containing a byte that would let it escape its own line in the signed
/// header block or the `Authorization` header: CR, LF or NUL.
///
/// This is NOT the check for a header **name** - see [`validate_header_name`], a full RFC 7230 token
/// check. A name is structured (only a fixed alphabet is ever legal), but a value is not: S3 header
/// values are not restricted to that alphabet, so this rule is deliberately narrow, and stays narrow for
/// a real reason rather than a convenient one. The motivating case - `x-amz-copy-source` built from an S3
/// object key - does not actually carry a raw CR/LF in practice: AWS requires that header's value to be
/// URI-encoded, so a key's CR/LF bytes reach it as `%0D%0A`, never raw (this crate's own
/// [`crate::S3Config::object_target`] URI-encodes every key byte the same way, via [`encode_path`]).
/// The check below is therefore defence-in-depth against a future caller that signs an unencoded value,
/// not a guard against a request this crate can build today - but the *rule* is still the right one: what
/// a value must never do is change the *shape* of the request by escaping its own line. CR and LF do
/// that directly, and a raw NUL is refused alongside them because C-string-based HTTP tooling downstream
/// can silently truncate a header at one - the same "byte changes what the server actually sees" failure
/// in a different disguise.
///
/// # CPE-1695: the rest of the C0 range (VT, FF, and friends) was investigated, not widened
///
/// RFC 7230's `field-content` grammar excludes the whole C0 control range from a header value (only SP
/// and HTAB are legal inside one), so `\x0b` (VT), `\x0c` (FF) and the other controls this function lets
/// through are a spec violation. CPE-1691 deliberately left them alone pending an investigation into
/// *which* controls actually break *which* client, rather than a scope expansion decided in passing. This
/// is that investigation. **The rule stays at CR/LF/NUL** - but read the correction below before relying
/// on any claim about what a VT or FF byte actually does downstream.
///
/// That client is `ureq` 2.12.1 - resolved at that exact version by `crates/webdav/Cargo.lock`
/// (`crates/webdav/Cargo.toml` only asks for `version = "2"`), the sibling remote-backend crate this one
/// will share an HTTP layer with once the request layer lands (CPE-1683/1684; this crate has no HTTP
/// client of its own yet). `PreludeBuilder::write_header` in `ureq-2.12.1/src/unit.rs:514` does build the
/// request line with `write!(self.prelude, "{}: {}\r\n", name, value)` straight onto a `Vec<u8>`, and
/// `Header::new` in `ureq-2.12.1/src/header.rs:85-87` is just `format!("{}: {}", name, value)` - neither
/// of those two functions checks the value's byte range.
///
/// ## Correction (PR #883 review): the header is DROPPED, not passed through
///
/// The first pass of this investigation read those two functions and concluded a VT or FF "is written to
/// the wire unchanged - no error, no panic, no truncation". **That is wrong, because it stopped one hop
/// short of the real send path.** The write loop at `ureq-2.12.1/src/unit.rs:467-473` never hands
/// `write_header` a raw value; it calls `header.value()` first and skips the header entirely when that
/// returns `None`:
///
/// ```text
/// for header in &unit.headers {
///     if let Some(v) = header.value() {
///         prelude.write_header(header.name(), v)?;
///     }
/// }
/// ```
///
/// `Header::value()` (`header.rs:99-109`) filters through `is_field_vchar_or_obs_fold`
/// (`header.rs:231-237`), which permits only `{SP, HTAB} ∪ [0x21, 0x7E]`. Because the filter applies to
/// the whole `Option<&str>`, a *single* non-conforming byte makes `value()` return `None` and the
/// `if let Some(v)` drops **the entire header** from the outgoing request. Silently. VT (`0x0B`) and FF
/// (`0x0C`) both fail that predicate.
///
/// **So does a non-breaking space** - NBSP is `0xC2 0xA0` in UTF-8 and both bytes are >= 0x80. That is the
/// same NBSP this very ticket decided must be *preserved* through canonicalisation. The preservation is
/// correct and necessary (S3 does not trim it either, so signing over trimmed text was a real bug), but
/// it is not sufficient: once `ureq` is actually wired up, an NBSP-bearing header would be signed and then
/// dropped before it reached the wire.
///
/// This is worse for the goal than a byte surviving would be. A dropped signed header desynchronises
/// what-was-signed from what-was-sent, which is precisely the opaque `SignatureDoesNotMatch` this ticket
/// exists to reduce - arriving by a different route.
///
/// **Therefore: an open question for CPE-1683/1684, not a closed one.** Whoever wires up the transport
/// must decide what to do when a signed header value contains a byte `ureq` will refuse to send - refuse
/// the request loudly at our layer, encode the value, or use a client without that filter. Do not assume
/// the bytes reach the wire.
///
/// ## What the finding does and does not cover
///
/// It covers the client this codebase will actually use, at its resolved version, traced from
/// `RequestBuilder::set` (`request.rs:310-312`) through `Unit::new` (`request.rs:141-148`) to the write
/// loop above. It says nothing about how real AWS S3 or a third-party S3-compatible server parses these
/// bytes server-side - that needs a live network call this crate has no fixture for and this ticket did
/// not budget.
///
/// Nothing here justifies widening the rule, and widening would not even fix the real hazard: NBSP
/// triggers the identical silent drop, so a VT/FF-only reject would be incomplete against it, and
/// CPE-1691's premise ("S3 legitimately carries near-arbitrary bytes in some header values") still argues
/// against a client-side reject. The rule stays exactly where CPE-1691 put it. Reopen it only with a
/// live-server finding that one of these bytes actually breaks something - not a "widen it to be safe"
/// instinct, which is the thing this comment exists to stop.
pub(crate) fn reject_framing_bytes(kind: &str, s: &str) -> Result<(), String> {
    if let Some(ch) = s.chars().find(|c| matches!(c, '\r' | '\n' | '\0')) {
        return Err(format!(
            "s3: {kind} must not contain a CR, LF or NUL (it would let the value escape its line in the \
             signed request), got {ch:?} in {s:?}"
        ));
    }
    Ok(())
}

/// Signs requests for one `(credentials, region, service)` triple.
///
/// Holds a borrowed [`Credentials`] so the secret is never copied into a second place that could outlive
/// it or end up in a log line.
#[derive(Debug, Clone, Copy)]
pub struct Signer<'a> {
    credentials: &'a Credentials,
    region: &'a str,
    service: &'a str,
}

impl<'a> Signer<'a> {
    /// A signer for the `s3` service - what every request in this crate uses. Delegates to
    /// [`Signer::for_service`] with `service: "s3"` rather than duplicating its validation, so there is
    /// exactly one place that decides what a valid `(region, service, access_key_id)` triple looks like.
    pub fn new(credentials: &'a Credentials, region: &'a str) -> Result<Self, String> {
        Self::for_service(credentials, region, "s3")
    }

    /// A signer for some other service name. Only the published AWS test vectors need this (they are
    /// stated against `service` and `iam` as well as `s3`), but it costs one field and keeps the vector
    /// tests honest - they exercise the same code path production does, rather than a test-only copy.
    ///
    /// Validates `region`, `service`, and `credentials.access_key_id` - all three are woven into either
    /// the credential scope or the `Authorization` header's `Credential=` field, so a control character,
    /// `,` or `/` in any one of them can inject or restructure (CPE-1691, UAT SEC-2/SEC-4: an unvalidated
    /// `service` let `Signer::for_service(&c, "us-east-1", "s3\r\nX-Injected: 1")` land raw in the
    /// credential scope, and `,`/`/` in `region`/`access_key_id` were not refused by the plain structural
    /// check at all).
    pub fn for_service(credentials: &'a Credentials, region: &'a str, service: &'a str) -> Result<Self, String> {
        validate_region(region)?;
        validate_credential_scope_element("access key id", &credentials.access_key_id)?;
        validate_credential_scope_element("service name", service)?;
        Ok(Signer { credentials, region, service })
    }

    /// Sign `input`, returning every stage.
    ///
    /// Fails on a malformed `amz_date`, a method that is not an uppercase HTTP token, a header name that
    /// is not an RFC 7230 token ([`validate_header_name`]), a header value carrying a CR/LF/NUL
    /// ([`reject_framing_bytes`]), a header list with no `host` in it (or more than one), or a `host`
    /// value that is empty or whitespace-only. Each of those would otherwise produce a signature that
    /// derives cleanly and can only fail at the server as an opaque `SignatureDoesNotMatch` - the failure
    /// mode this module exists to keep out of the user's hands. No error ever echoes the secret (nothing
    /// in this module formats it into a string other than the HMAC input).
    pub fn sign(&self, input: &SigningInput<'_>) -> Result<SignedRequest, String> {
        validate_method(input.method)?;
        let date = short_date(input.amz_date)?;

        // Validate every header up front, and check `host`'s value on each RAW, pre-merge occurrence
        // rather than on the merged/collapsed text `canonical_headers` produces below (CPE-1691, UAT
        // SEC-3): `[("host", ""), ("host", "")]` merges to `host:,`, which is non-empty even though
        // neither supplied value was usable, so a check on the merged line missed it. A second `host`
        // header is refused outright rather than merged - `[("host","good"),("host","evil")]` merging to
        // `host:good,evil` is the same "signs cleanly, fails opaquely at the server" shape D3 exists to
        // prevent, one level up from the empty-value case.
        let mut host_count = 0u32;
        for (name, value) in input.headers {
            validate_header_name(name)?;
            reject_framing_bytes("header value", value)?;
            if name.trim().eq_ignore_ascii_case("host") {
                host_count += 1;
                if host_count > 1 {
                    return Err(format!(
                        "s3: only one \"host\" header may be signed - got a second value {value:?}"
                    ));
                }
                if normalize_header_value(value).is_empty() {
                    return Err(format!(
                        "s3: the \"host\" header value must not be empty or whitespace-only - S3 will \
                         reject a signature for a request with no usable Host, got {value:?}"
                    ));
                }
            }
        }

        let (header_block, signed_headers) = canonical_headers(input.headers);
        // Counting exact pre-merge `host` names above is deliberately redundant with
        // `validate_header_name`, and must NOT be "simplified" back to scanning the merged
        // `signed_headers` list (`signed_headers.split(';').any(|n| n == "host")`). That older form was
        // the SEC-1 bypass: a name of `x-evil;host` put the substring `host` into the list and satisfied
        // the check with no `host` header present at all. No test can distinguish the two forms today -
        // `validate_header_name` refuses every name that could contain a `;`, so no legal input reaches
        // the difference - which is exactly why this note exists instead of a test (CPE-1691, reviewer
        // NB-1). The `debug_assert` is the tripwire: an empty element in the list would mean a name
        // slipped past the token check. (An empty `signed_headers` is the no-headers-at-all case,
        // refused just below by `host_count`.)
        debug_assert!(
            signed_headers.is_empty() || signed_headers.split(';').all(|n| !n.is_empty()),
            "validate_header_name should have refused an empty header name, got [{signed_headers}]"
        );
        if host_count == 0 {
            return Err(format!(
                "s3: the headers to sign must include \"host\" - SigV4 covers it and S3 rejects a \
                 signature that does not; got [{signed_headers}]"
            ));
        }

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            input.method,
            if input.encoded_path.is_empty() { "/" } else { input.encoded_path },
            canonical_query(input.query),
            header_block,
            signed_headers,
            input.payload_hash,
        );

        let credential_scope = format!("{date}/{}/{}/{SCOPE_TERMINATOR}", self.region, self.service);
        let string_to_sign = format!(
            "{ALGORITHM}\n{}\n{credential_scope}\n{}",
            input.amz_date,
            sha256_hex(canonical_request.as_bytes())
        );

        let key = signing_key(self.credentials.secret(), date, self.region, self.service);
        let signature = hex_lower(&hmac_sha256(&key, string_to_sign.as_bytes()));

        let authorization = format!(
            "{ALGORITHM} Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.credentials.access_key_id,
        );

        Ok(SignedRequest {
            canonical_request,
            string_to_sign,
            credential_scope,
            signed_headers,
            signature,
            authorization,
        })
    }
}

/// Reject an HTTP method that is not already an uppercase token.
///
/// The method is the first line of the canonical request, byte for byte, so `"get"`, `"Get"` and
/// `" GET "` each sign to a different - and silently wrong - signature. Uppercasing here would fix the
/// signature while quietly disagreeing with whatever the caller's HTTP client puts on the request line,
/// so this refuses instead: the field is documented as uppercase, and the crate's rule is that failing
/// loudly beats doing something different in silence (CPE-1689).
fn validate_method(method: &str) -> Result<(), String> {
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(format!(
            "s3: HTTP method must be an uppercase token with no whitespace (GET, PUT, ...), got {method:?}"
        ));
    }
    Ok(())
}

/// The `YYYYMMDD` prefix of a `YYYYMMDDTHHMMSSZ` timestamp, validated. The shape is checked rather than
/// assumed because a timestamp that is merely *wrong* still derives a signing key without complaint and
/// fails only at the server, as an opaque `SignatureDoesNotMatch`.
fn short_date(amz_date: &str) -> Result<&str, String> {
    let bytes = amz_date.as_bytes();
    let well_formed = bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[9..15].iter().all(u8::is_ascii_digit);
    if !well_formed {
        return Err(format!("s3: x-amz-date must be YYYYMMDDTHHMMSSZ, got {amz_date:?}"));
    }
    Ok(&amz_date[..8])
}

/// Format a Unix timestamp (UTC seconds) as SigV4's `x-amz-date`, `YYYYMMDDTHHMMSSZ`.
///
/// Hand-rolled rather than pulled from a date crate: this is the only calendar arithmetic the crate
/// needs, it is a pure function of an integer, and it keeps the "no new dependency family" promise. The
/// civil-from-days conversion is Howard Hinnant's shift-the-year-to-March algorithm - correct across
/// leap years and centuries, with no lookup tables. Signing is always done in **UTC**; there is no local
/// timezone anywhere in this path, so the three CI OSes cannot disagree about what time it is.
pub fn amz_date_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (secs_of_day / 3600, (secs_of_day % 3600) / 60, secs_of_day % 60);
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

/// Days since the Unix epoch -> `(year, month, day)` in the proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap day lands at the end of the year and needs no special case.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era, 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era, 0..=399
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year (March-based), 0..=365
    let mp = (5 * doy + 2) / 153; // March-based month, 0..=11
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Credentials;

    /// The credentials the **aws-sig-v4-test-suite** vectors are stated against.
    fn suite_credentials() -> Credentials {
        Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY")
    }

    /// The credentials the **AWS S3 developer-guide** signing examples are stated against.
    fn s3_doc_credentials() -> Credentials {
        Credentials::new("AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY")
    }

    // ---------------------------------------------------------------------------------------------
    // Stage 3 - the signing key, against AWS's published derivation example.
    // ---------------------------------------------------------------------------------------------

    /// AWS SigV4 docs, "Examples of how to derive a signing key": secret
    /// `wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY`, date `20150830`, region `us-east-1`, service `iam`.
    /// The published derived key is the hex below. This pins stage 3 on its own, so a broken chain shows
    /// up here rather than only as a wrong final signature.
    #[test]
    fn signing_key_matches_the_published_derivation_example() {
        let key = signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        assert_eq!(
            hex_lower(&key),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // Stages 1-4 - aws-sig-v4-test-suite `get-vanilla`.
    // ---------------------------------------------------------------------------------------------

    /// `get-vanilla` from AWS's published SigV4 test suite: `GET /` against `example.amazonaws.com`,
    /// region `us-east-1`, service `service`, at `20150830T123600Z`.
    ///
    /// Note on what is evidence here: the canonical request and the `Authorization` header are asserted
    /// against the **published** vector text. The string-to-sign's fourth line is
    /// `sha256(canonical request)` - it is not independently quoted from the suite, and it does not need
    /// to be: the canonical request above it is vector-pinned, and the signature below it is an HMAC over
    /// this exact string, so a single wrong byte anywhere in the string-to-sign would change the `.authz`
    /// signature and fail the last assertion.
    #[test]
    fn get_vanilla_vector_matches_at_every_stage() {
        let creds = suite_credentials();
        let signer = Signer::for_service(&creds, "us-east-1", "service").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[
                    ("Host", "example.amazonaws.com"),
                    ("X-Amz-Date", "20150830T123600Z"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20150830T123600Z",
            })
            .unwrap();

        assert_eq!(
            signed.canonical_request,
            concat!(
                "GET\n",
                "/\n",
                "\n",
                "host:example.amazonaws.com\n",
                "x-amz-date:20150830T123600Z\n",
                "\n",
                "host;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            "get-vanilla.creq"
        );
        assert_eq!(
            signed.string_to_sign,
            concat!(
                "AWS4-HMAC-SHA256\n",
                "20150830T123600Z\n",
                "20150830/us-east-1/service/aws4_request\n",
                "bb579772317eb040ac9ed261061d46c1f17a8133879d6129b6e1c25292927e63",
            ),
            "get-vanilla.sts"
        );
        assert_eq!(
            signed.authorization,
            concat!(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, ",
                "SignedHeaders=host;x-amz-date, ",
                "Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
            ),
            "get-vanilla.authz"
        );
    }

    /// `get-vanilla-query-order-key-case` from the same suite: two query parameters supplied in
    /// non-canonical order (`Param2=value2&Param1=value1`). The canonical request must re-sort them, so
    /// this is the vector that proves the query ordering rule rather than asserting it in a comment.
    #[test]
    fn query_parameters_are_sorted_into_canonical_order() {
        let creds = suite_credentials();
        let signer = Signer::for_service(&creds, "us-east-1", "service").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[("Param2", "value2"), ("Param1", "value1")],
                headers: &[
                    ("Host", "example.amazonaws.com"),
                    ("X-Amz-Date", "20150830T123600Z"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20150830T123600Z",
            })
            .unwrap();

        assert!(
            signed.canonical_request.contains("\nParam1=value1&Param2=value2\n"),
            "query must be re-sorted; got:\n{}",
            signed.canonical_request
        );
        assert_eq!(
            signed.authorization,
            concat!(
                "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, ",
                "SignedHeaders=host;x-amz-date, ",
                "Signature=b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500",
            ),
            "get-vanilla-query-order-key-case.authz"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // Stages 1-4 - the AWS S3 developer-guide examples (service `s3`, the one we actually ship).
    // ---------------------------------------------------------------------------------------------

    /// "Signature Calculations for the Authorization Header: Transferring Payload in a Single Chunk",
    /// Example 1 - GET Object with a `Range` header, `examplebucket.s3.amazonaws.com`, `20130524T000000Z`.
    /// This is the vector that covers the real production path: service `s3`, virtual-host addressing,
    /// and `x-amz-content-sha256` among the signed headers.
    #[test]
    fn s3_get_object_vector_matches_at_every_stage() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/test.txt",
                query: &[],
                headers: &[
                    ("host", "examplebucket.s3.amazonaws.com"),
                    ("range", "bytes=0-9"),
                    ("x-amz-content-sha256", EMPTY_PAYLOAD_SHA256),
                    ("x-amz-date", "20130524T000000Z"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap();

        assert_eq!(
            signed.canonical_request,
            concat!(
                "GET\n",
                "/test.txt\n",
                "\n",
                "host:examplebucket.s3.amazonaws.com\n",
                "range:bytes=0-9\n",
                "x-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
                "x-amz-date:20130524T000000Z\n",
                "\n",
                "host;range;x-amz-content-sha256;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )
        );
        assert_eq!(
            signed.string_to_sign,
            concat!(
                "AWS4-HMAC-SHA256\n",
                "20130524T000000Z\n",
                "20130524/us-east-1/s3/aws4_request\n",
                "7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972",
            )
        );
        assert_eq!(
            signed.signature,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    /// Same AWS page, Example 2 - PUT Object with a body, `x-amz-storage-class`, and a real payload hash.
    /// Covers the write side: a non-empty `x-amz-content-sha256` that the canonical request must carry
    /// in two places (the header block and the trailing payload-hash line).
    #[test]
    fn s3_put_object_vector_matches() {
        let body = b"Welcome to Amazon S3.";
        let payload_hash = sha256_hex(body);
        assert_eq!(
            payload_hash,
            "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072",
            "the published payload hash for this example's body"
        );

        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "PUT",
                encoded_path: "/test%24file.text",
                query: &[],
                headers: &[
                    ("date", "Fri, 24 May 2013 00:00:00 GMT"),
                    ("host", "examplebucket.s3.amazonaws.com"),
                    ("x-amz-content-sha256", &payload_hash),
                    ("x-amz-date", "20130524T000000Z"),
                    ("x-amz-storage-class", "REDUCED_REDUNDANCY"),
                ],
                payload_hash: &payload_hash,
                amz_date: "20130524T000000Z",
            })
            .unwrap();

        assert_eq!(
            signed.signed_headers,
            "date;host;x-amz-content-sha256;x-amz-date;x-amz-storage-class"
        );
        assert_eq!(
            signed.signature,
            "98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"
        );
    }

    /// Same AWS page, Example 3 - GET Bucket (List Objects) with query parameters `max-keys=2&prefix=J`.
    /// The only published **S3** vector with a query string, so it is what pins the canonical-query rule
    /// for the service this crate actually talks to (ListObjectsV2 in CPE-1683 is exactly this shape).
    #[test]
    fn s3_list_objects_vector_matches() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[("max-keys", "2"), ("prefix", "J")],
                headers: &[
                    ("host", "examplebucket.s3.amazonaws.com"),
                    ("x-amz-content-sha256", EMPTY_PAYLOAD_SHA256),
                    ("x-amz-date", "20130524T000000Z"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap();

        // Equality, not `starts_with`: a prefix assertion would have tolerated anything appended after
        // the part being checked, which is precisely what a canonical-request bug looks like. The
        // pinned signature below carried the load on its own before CPE-1689; now the vector text does
        // too, so a failure says *where* it diverged instead of only *that* it did.
        assert_eq!(
            signed.canonical_request,
            concat!(
                "GET\n",
                "/\n",
                "max-keys=2&prefix=J\n",
                "host:examplebucket.s3.amazonaws.com\n",
                "x-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
                "x-amz-date:20130524T000000Z\n",
                "\n",
                "host;x-amz-content-sha256;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )
        );
        assert_eq!(
            signed.signature,
            "34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // The encoding rules, and the ways they are usually got wrong.
    // ---------------------------------------------------------------------------------------------

    /// The path encoder escapes everything outside the unreserved set but keeps `/`; the query encoder
    /// escapes `/` as well. `+` is never a space in either.
    #[test]
    fn path_and_query_encoders_differ_only_in_how_they_treat_slash() {
        assert_eq!(encode_path("/a b/c+d.txt"), "/a%20b/c%2Bd.txt");
        assert_eq!(encode_query_component("a/b"), "a%2Fb");
        assert_eq!(encode_path("/a/b"), "/a/b");
        // Unreserved set passes through untouched; everything else is escaped uppercase-hex.
        assert_eq!(encode_path("/AZaz09-._~"), "/AZaz09-._~");
        assert_eq!(encode_path("/~!*'()"), "/~%21%2A%27%28%29");
        // Multi-byte UTF-8 is escaped per byte (U+00E9 is 0xC3 0xA9).
        assert_eq!(encode_path("/\u{e9}"), "/%C3%A9");
    }

    /// S3 is the service that does **not** normalize the canonical URI: duplicate slashes and dot
    /// segments are part of the key, so they must survive into the signature unchanged. A signer that
    /// borrowed the generic (non-S3) normalizing rule would collapse these and fail against every real
    /// bucket that has such a key.
    #[test]
    fn s3_canonical_uri_is_not_path_normalized() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let sign_path = |p: &str| {
            signer
                .sign(&SigningInput {
                    method: "GET",
                    encoded_path: p,
                    query: &[],
                    headers: &[("host", "examplebucket.s3.amazonaws.com")],
                    payload_hash: EMPTY_PAYLOAD_SHA256,
                    amz_date: "20130524T000000Z",
                })
                .unwrap()
        };
        assert!(sign_path("/a//b").canonical_request.starts_with("GET\n/a//b\n"));
        assert!(sign_path("/a/../b").canonical_request.starts_with("GET\n/a/../b\n"));
        assert_ne!(sign_path("/a//b").signature, sign_path("/a/b").signature);
    }

    /// Header values are trimmed and their internal whitespace runs collapsed, header names are
    /// lowercased and sorted, and repeated names merge into one comma-joined value.
    #[test]
    fn headers_are_lowercased_trimmed_sorted_and_merged() {
        let creds = suite_credentials();
        let signer = Signer::for_service(&creds, "us-east-1", "service").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[
                    ("My-Header1", "  a   b  c  "),
                    ("Host", "example.amazonaws.com"),
                    ("My-Header1", "second"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20150830T123600Z",
            })
            .unwrap();
        assert_eq!(signed.signed_headers, "host;my-header1");
        assert!(
            signed.canonical_request.contains("my-header1:a b c,second\n"),
            "got:\n{}",
            signed.canonical_request
        );
    }

    /// `get-header-value-trim` from AWS's published SigV4 test suite — the vector that settles what the
    /// collapse rule does inside a quoted string.
    ///
    /// It matters because the module used to carry a comment claiming SigV4 exempts quoted strings from
    /// the collapse and that this crate deliberately deviates. It does not: the published canonical
    /// request for this vector contains `my-header2:"a b c"`, collapsed **inside** the quotes, which is
    /// what the code already produced. The comment described a deviation that did not exist and invited
    /// a future "fix" that would have broken this vector; it is deleted, and this test is what stops the
    /// claim coming back (CPE-1689).
    ///
    /// Scope of the evidence: the **canonical request** below is the published vector text. The
    /// signature is not asserted here because the suite's `.authz` line for this vector was not
    /// re-derived from the published file — the canonical request is the stage the claim was about, and
    /// the vectors above already pin the stages after it.
    #[test]
    fn header_value_whitespace_collapses_inside_a_quoted_string_too() {
        let creds = suite_credentials();
        let signer = Signer::for_service(&creds, "us-east-1", "service").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[
                    ("Host", "example.amazonaws.com"),
                    ("My-Header1", "value1"),
                    ("My-Header2", "\"a   b   c\""),
                    ("X-Amz-Date", "20150830T123600Z"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20150830T123600Z",
            })
            .unwrap();

        assert_eq!(
            signed.canonical_request,
            concat!(
                "GET\n",
                "/\n",
                "\n",
                "host:example.amazonaws.com\n",
                "my-header1:value1\n",
                "my-header2:\"a b c\"\n",
                "x-amz-date:20150830T123600Z\n",
                "\n",
                "host;my-header1;my-header2;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            "get-header-value-trim.creq"
        );
    }

    /// CPE-1695: `normalize_header_value` trims SP and HTAB only, not `str::trim()`'s full Unicode
    /// whitespace set. A leading/trailing NBSP (U+00A0) is not SP, so it must survive - if it were
    /// stripped, the client would canonicalise a different value than S3 does, and the two signatures
    /// would silently diverge.
    #[test]
    fn normalize_header_value_trims_sp_and_htab_only_not_unicode_whitespace() {
        // The unit under test, directly, before it disappears into a signature.
        assert_eq!(normalize_header_value("\u{a0}value\u{a0}"), "\u{a0}value\u{a0}");
        assert_eq!(normalize_header_value("  value  "), "value");
        assert_eq!(normalize_header_value("\tvalue\t"), "value");
        // Sequential-space collapsing is unaffected by switching the trim.
        assert_eq!(normalize_header_value("  a   b   c  "), "a b c");

        // End to end: the NBSP reaches the canonical request rather than being silently stripped there.
        let creds = suite_credentials();
        let signer = Signer::for_service(&creds, "us-east-1", "service").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[
                    ("Host", "example.amazonaws.com"),
                    ("X-Amz-Date", "20150830T123600Z"),
                    ("My-Header1", "\u{a0}value"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20150830T123600Z",
            })
            .unwrap();
        assert!(
            signed.canonical_request.contains("my-header1:\u{a0}value\n"),
            "the leading NBSP must survive normalisation, got:\n{}",
            signed.canonical_request
        );
    }

    /// CPE-1695: the documented decision on the rest of the C0 range (see the doc comment on
    /// [`reject_framing_bytes`]) is to leave VT/FF/friends unrefused, because the client this crate will
    /// actually ship requests through (`ureq`) does not choke on them - only CR/LF/NUL do anything special
    /// to that client's request-line assembly. This pins that decision as passing behaviour, so a future
    /// change either direction (a stray widening, or an accidental narrowing that starts refusing these)
    /// shows up here rather than only in the doc comment.
    #[test]
    fn vertical_tab_and_form_feed_are_not_refused_in_a_header_value() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[("host", "h.example"), ("x-amz-meta-note", "a\x0bb\x0cc")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap();
        assert!(
            signed.canonical_request.contains("x-amz-meta-note:a\x0bb\x0cc\n"),
            "VT/FF must survive both validation and normalisation unchanged, got:\n{}",
            signed.canonical_request
        );
    }

    // ---------------------------------------------------------------------------------------------
    // What `sign` refuses (CPE-1689). Each of these produced a signature that could only fail at the
    // server, as an opaque `SignatureDoesNotMatch` with nothing in it to say why.
    // ---------------------------------------------------------------------------------------------

    /// The `host` header is documented as mandatory — S3 will not accept a signature that does not cover
    /// it. Signing without one used to succeed and emit `SignedHeaders=`, producing exactly the opaque
    /// server-side failure this module exists to prevent. The error names the missing header.
    #[test]
    fn signing_without_a_host_header_is_refused_by_name() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let sign_with = |headers: &[(&str, &str)]| {
            signer.sign(&SigningInput {
                method: "GET",
                encoded_path: "/test.txt",
                query: &[],
                headers,
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
        };

        let err = sign_with(&[]).unwrap_err();
        assert!(err.contains("host"), "the error must name the missing header: {err}");
        assert!(!err.contains("wJalrXUtnFEMI"), "the secret must never reach an error message: {err}");

        // A header list that has other headers but no host is refused just the same.
        let err = sign_with(&[("x-amz-date", "20130524T000000Z")]).unwrap_err();
        assert!(err.contains("host"), "{err}");
        // A name that merely *contains* "host" is not a host header.
        assert!(sign_with(&[("x-forwarded-host", "example.com")]).is_err());

        // Any capitalisation of the real thing is accepted — the canonical form is lowercased first.
        assert!(sign_with(&[("HOST", "examplebucket.s3.amazonaws.com")]).is_ok());
        assert!(sign_with(&[(" Host ", "examplebucket.s3.amazonaws.com")]).is_ok());
    }

    /// The method is the first line of the canonical request byte for byte, so `"get"`, `"Get"` and
    /// `" GET "` each signed to a different and silently wrong signature. They are refused.
    #[test]
    fn a_method_that_is_not_an_uppercase_token_is_refused() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let sign_method = |method: &str| {
            signer.sign(&SigningInput {
                method,
                encoded_path: "/test.txt",
                query: &[],
                headers: &[("host", "examplebucket.s3.amazonaws.com")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
        };

        for bad in ["get", "Get", " GET ", "GET ", "", "GET\r\nX-Injected: 1", "G3T"] {
            let err = sign_method(bad).unwrap_err();
            assert!(err.contains("uppercase token"), "for {bad:?}: {err}");
            assert!(!err.contains("wJalrXUtnFEMI"), "{err}");
        }
        for good in ["GET", "PUT", "HEAD", "DELETE", "POST"] {
            assert!(sign_method(good).is_ok(), "rejected a valid method {good:?}");
        }
    }

    /// An empty query value still contributes its `=` - `?acl` signs as `acl=`, which is exactly the
    /// shape the later object-ops ticket needs for sub-resource requests.
    #[test]
    fn valueless_query_parameter_signs_with_a_trailing_equals() {
        assert_eq!(canonical_query(&[("acl", "")]), "acl=");
        assert_eq!(canonical_query(&[]), "");
    }

    // ---------------------------------------------------------------------------------------------
    // What `sign`/`Signer::new`/`Signer::for_service` refuse (CPE-1691). Found by the independent UAT
    // on PR #868 (CPE-1689): endpoint validation closed the request-splitting shape at one door and left
    // the header/region/access-key-id doors open. Every case here is driven through the public API - the
    // header cases through `sign()`, the region cases through both public paths that take one.
    // ---------------------------------------------------------------------------------------------

    /// The one that matters most: a header **value** carrying CR/LF used to sign cleanly and put a
    /// second, attacker-chosen line into the canonical request - `x-amz-copy-source` built from an S3
    /// key (which legally contains CR/LF) into `X-Amz-Acl: public-read` standing on its own line. Refused
    /// now, and the assertion checks the *message* (not `is_err()`): CPE-1691 was filed partly because a
    /// CRLF **endpoint** was once caught by the unrelated port check, so a bare `is_err()` here would be
    /// exactly that false comfort again.
    #[test]
    fn a_header_value_containing_cr_or_lf_is_refused_and_the_canonical_request_keeps_its_line_count() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let sign_with_copy_source = |value: &str| {
            signer.sign(&SigningInput {
                method: "PUT",
                encoded_path: "/dest.txt",
                query: &[],
                headers: &[("host", "h.example"), ("x-amz-copy-source", value)],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
        };

        // The exact shape from the ticket: a copy-source key smuggling a bogus header onto its own line.
        let err = sign_with_copy_source("/bucket/dir/evil\r\nX-Amz-Acl: public-read").unwrap_err();
        assert!(
            err.contains("CR, LF or NUL"),
            "the message must name the framing-byte rule, not just fail: {err}"
        );
        // The bad value is echoed in the message (it is not a secret - only `secret_access_key` is, and
        // this crate never formats that one into anything) because seeing exactly what was rejected is
        // the whole point of a message a caller might paste into a bug report.
        assert!(err.contains("public-read"), "{err}");

        // Bare LF and a raw NUL are refused the same way - each also escapes or truncates the value's
        // own line, just via a different byte.
        for bad in ["/bucket/dir\nX-Injected: 1", "/bucket/dir\0evil"] {
            let err = sign_with_copy_source(bad).unwrap_err();
            assert!(err.contains("CR, LF or NUL"), "for {bad:?}: {err}");
        }

        // A legitimate copy-source key needing none of those bytes signs to exactly the eight canonical
        // request lines two signed headers produce (method, path, query, 2 header lines, blank,
        // signed-header list, payload hash) - proving the refused cases above were rejected outright
        // rather than merely losing a byte or two. Before CPE-1691 the malicious value above would have
        // produced a *nine*-line canonical request, per the ticket.
        let clean = sign_with_copy_source("/bucket/dir/evil").unwrap();
        assert_eq!(
            clean.canonical_request.lines().count(),
            8,
            "a clean two-header request has 8 canonical-request lines; got:\n{}",
            clean.canonical_request
        );
    }

    /// A header **name** is structured, not free-form - it must be an RFC 7230 token - so it is *not*
    /// covered by [`reject_framing_bytes`]'s narrow "CR/LF/NUL only" rule for values; see
    /// [`validate_header_name`]. Each case here is its **own** `#[test]` per the independent UAT on this
    /// PR: previously the name check shared a test with the value check, so disabling just the name check
    /// did not go red on its own — a change that reds the wrong test (or none) proves nothing about which
    /// guard is load-bearing.
    ///
    /// `"x-evil;host"` is the important one: it is the exact SEC-1 finding from that UAT. Before this
    /// fix, a header list with **no real `host` header at all** — just this one malicious name — still
    /// satisfied `signed_headers.split(';').any(|n| n == "host")`, because the injected `;host` became a
    /// second entry in the merged `SignedHeaders` list. That fully bypassed the CPE-1689 host-presence
    /// guard and signed a request with zero actual `Host` coverage. The assertion below checks for the
    /// RFC-7230-token message, not merely `is_err()`, specifically so a regression that swaps this check
    /// for something else that happens to also reject `;` (but not close the bypass) cannot pass silently.
    #[test]
    fn a_header_name_that_is_not_an_rfc7230_token_is_refused_and_cannot_forge_the_host_check() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();

        // The SEC-1 bypass, reproduced exactly: no "host" header anywhere in the list, only a name that
        // satisfied the OLD `signed_headers.split(';').any(|n| n == "host")` check.
        let err = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[("x-evil;host", "1")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap_err();
        assert!(err.contains("RFC 7230 token"), "{err}");
        assert!(
            !err.contains("must include \"host\""),
            "the request must be refused for the name being invalid, not merely re-discovered as \
             hostless by a later check: {err}"
        );

        // The rest signed alongside a real `host` header, so a name-only refusal is what is being pinned
        // rather than the separate (and already-covered) missing-host case.
        let sign_with_name = |name: &str| {
            signer.sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[("host", "h.example"), (name, "1")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
        };

        // A forged `Authorization` parameter ahead of the real `Signature=`.
        let err = sign_with_name("x, Signature=deadbeef").unwrap_err();
        assert!(err.contains("RFC 7230 token"), "{err}");

        // An ambiguous name/value boundary on the canonical header line.
        let err = sign_with_name("x evil:injected").unwrap_err();
        assert!(err.contains("RFC 7230 token"), "{err}");

        // CR/LF in a name is refused by this check too (it is not an RFC 7230 tchar either), even though
        // the narrower value-only `reject_framing_bytes` rule does not apply to names at all.
        let err = sign_with_name("x-evil\r\nX-Injected").unwrap_err();
        assert!(err.contains("RFC 7230 token"), "{err}");

        // An empty name is not a token either. Without the `is_empty()` clause this signs to a canonical
        // line of `:v` with `SignedHeaders=";host"` - a name that is not there at all (reviewer NB-2).
        let err = sign_with_name("").unwrap_err();
        assert!(err.contains("RFC 7230 token"), "{err}");

        // A real header name — including one with the token punctuation RFC 7230 actually allows — still
        // signs.
        assert!(sign_with_name("x-amz-meta-foo").is_ok());
        assert!(sign_with_name("x!y#z").is_ok());
    }

    /// `region "us-east-1\r\nX-Injected: 1"` used to land raw in the `Authorization` header's credential
    /// scope (`Credential=AKIA…/20130524/us-east-1\r\nX-Injected: 1/s3/aws4_request`). Proven on **both**
    /// public paths that take a region, per the ticket: this test is `Signer::new`'s; the sibling test in
    /// `lib.rs` (`a_control_character_or_empty_region_is_refused_through_target_for_too`) is
    /// `S3Config::target_for`'s. Before CPE-1691 only one of the two checked anything at all.
    #[test]
    fn signer_new_and_for_service_refuse_an_empty_or_control_character_region_the_same_way() {
        let creds = s3_doc_credentials();

        let err = Signer::new(&creds, "us-east-1\r\nX-Injected: 1").unwrap_err();
        assert!(err.contains("control characters or whitespace"), "{err}");

        // The exact regression named in the ticket: this used to succeed and scope to "us-east-1//..".
        let err = Signer::new(&creds, "").unwrap_err();
        assert!(err.contains("region must not be empty"), "{err}");

        // `for_service` is not a second unguarded door.
        let err = Signer::for_service(&creds, "us-east-1\r\nX-Injected: 1", "s3").unwrap_err();
        assert!(err.contains("control characters or whitespace"), "{err}");
        let err = Signer::for_service(&creds, "", "s3").unwrap_err();
        assert!(err.contains("region must not be empty"), "{err}");

        // A normal region still works on both constructors.
        assert!(Signer::new(&creds, "us-east-1").is_ok());
        assert!(Signer::for_service(&creds, "us-east-1", "s3").is_ok());
    }

    /// SEC-2 from the independent reviewer on this PR: `service` lands in `credential_scope` and verbatim
    /// in `Authorization` exactly like `region` does, but was left completely unvalidated.
    /// `Signer::for_service(&c, "us-east-1", "s3\r\nX-Injected: 1")` used to yield
    /// `scope = "20130524/us-east-1/s3\r\nX-Injected: 1/aws4_request"`. `Signer::new` is covered too,
    /// because it now delegates to `for_service("s3")` rather than duplicating the check.
    #[test]
    fn for_service_refuses_a_control_character_service_name() {
        let creds = s3_doc_credentials();
        let err = Signer::for_service(&creds, "us-east-1", "s3\r\nX-Injected: 1").unwrap_err();
        assert!(err.contains("service name"), "{err}");
        assert!(err.contains("control characters or whitespace"), "{err}");

        // A normal service name still works, on both constructors.
        assert!(Signer::for_service(&creds, "us-east-1", "s3").is_ok());
        assert!(Signer::new(&creds, "us-east-1").is_ok());
    }

    /// SEC-4 from the independent reviewer: `,` terminates an `Authorization` parameter and `/` separates
    /// credential-scope elements, and the plain `validate_structural_text` standard refuses neither (an
    /// endpoint legitimately carries `/`, so the shared check cannot). `validate_credential_scope_element`
    /// layers both refusals on top for the three fields actually woven into `Authorization`/the credential
    /// scope: `access_key_id`, `region`, and `service`. Three distinct live payloads, one per field:
    #[test]
    fn credential_scope_elements_refuse_a_comma_or_slash_that_would_restructure_the_authorization_header() {
        let good_creds = s3_doc_credentials();

        // `,` in `access_key_id` forges an extra `Authorization` parameter ahead of the real `Signature=`.
        let bad_key_id = Credentials::new(
            "AKIA,Signature=deadbeef",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let err = Signer::new(&bad_key_id, "us-east-1").unwrap_err();
        assert!(err.contains("access key id"), "{err}");
        assert!(err.contains("must not contain ',' or '/'"), "{err}");

        // `/` in `region` restructures the credential-scope string itself.
        let err = Signer::new(&good_creds, "us-east-1/s3/aws4_request").unwrap_err();
        assert!(err.contains("region"), "{err}");
        assert!(err.contains("must not contain ',' or '/'"), "{err}");

        // The analogous shape for `service`: both bytes, in the field SEC-2 added validation for.
        let err = Signer::for_service(&good_creds, "us-east-1", "s3,Signature=deadbeef").unwrap_err();
        assert!(err.contains("service name"), "{err}");
        assert!(err.contains("must not contain ',' or '/'"), "{err}");
        let err = Signer::for_service(&good_creds, "us-east-1", "s3/extra").unwrap_err();
        assert!(err.contains("service name"), "{err}");
        assert!(err.contains("must not contain ',' or '/'"), "{err}");
    }

    /// `access_key_id` reaches the `Authorization` header's `Credential=` field unescaped
    /// (`Credential={access_key_id}/{credential_scope}, ...`), so a CR/LF there would inject a second
    /// header line into the response the caller's HTTP client eventually parses back. Refused at
    /// `Signer::new`/`for_service`, where credentials and region are bound together, so no signature can
    /// ever be produced from a poisoned key id.
    #[test]
    fn an_access_key_id_containing_cr_or_lf_cannot_inject_into_the_authorization_header() {
        let bad_creds = Credentials::new(
            "AKIA\r\nX-Injected: 1",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let err = Signer::new(&bad_creds, "us-east-1").unwrap_err();
        assert!(err.contains("access key id"), "{err}");
        assert!(err.contains("control characters or whitespace"), "{err}");

        let err = Signer::for_service(&bad_creds, "us-east-1", "s3").unwrap_err();
        assert!(err.contains("access key id"), "{err}");

        // A well-formed access key id still signs, and the id itself is safe to appear (it is not
        // secret - see `debug_output_never_contains_the_secret` in lib.rs for the part that is).
        let good_creds = s3_doc_credentials();
        let signer = Signer::new(&good_creds, "us-east-1").unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[("host", "examplebucket.s3.amazonaws.com")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap();
        assert!(signed.authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/"));
    }

    /// D3: presence of a header *named* `host` proved nothing about whether its *value* was usable.
    /// `("host", "")` and `("host", "   ")` used to sign cleanly - `SignedHeaders=host` with an empty
    /// `host:` line - and fail only at the server. The error names the actual problem.
    ///
    /// `[("host", ""), ("host", "")]` is the UAT's SEC-3 finding: the original guard inspected the
    /// **merged, post-`canonical_headers`** line, where two empty values merge to `host:,` — non-empty,
    /// so the guard missed it. This check now runs on each raw, pre-merge occurrence instead (see the
    /// loop in `sign()`), so this exact pair is refused too.
    #[test]
    fn an_empty_or_whitespace_only_host_value_is_refused_by_name() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let sign_with_headers = |headers: &[(&str, &str)]| signer.sign(&SigningInput {
            method: "GET",
            encoded_path: "/",
            query: &[],
            headers,
            payload_hash: EMPTY_PAYLOAD_SHA256,
            amz_date: "20130524T000000Z",
        });
        let sign_with_host = |host: &str| sign_with_headers(&[("host", host)]);

        let err = sign_with_host("").unwrap_err();
        assert!(err.contains("empty or whitespace-only"), "{err}");
        let err = sign_with_host("   ").unwrap_err();
        assert!(err.contains("empty or whitespace-only"), "{err}");

        // SEC-3: two empty `host` values, which used to merge to a non-empty `host:,` line.
        let err = sign_with_headers(&[("host", ""), ("host", "")]).unwrap_err();
        assert!(err.contains("empty or whitespace-only"), "{err}");

        // A real host still signs.
        assert!(sign_with_host("examplebucket.s3.amazonaws.com").is_ok());
    }

    /// The other half of SEC-3, called out as recommended by the reviewer: two **different** `host`
    /// values used to merge into `host:good.example,evil.example` — spec-correct per SigV4's header-merge
    /// rule, but exactly the "signs cleanly, fails opaquely at the server" shape D3 exists to prevent, one
    /// level up from the empty-value case. A second `host` header is refused outright instead.
    #[test]
    fn a_second_host_header_is_refused_outright_rather_than_merged() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let err = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[("host", "good.example"), ("host", "evil.example")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap_err();
        assert!(err.contains("only one \"host\" header"), "{err}");
    }

    // ---------------------------------------------------------------------------------------------
    // x-amz-date handling.
    // ---------------------------------------------------------------------------------------------

    /// The two timestamps every vector in this file uses, derived from their Unix seconds. UTC only -
    /// no local timezone is consulted, so all three CI OSes compute the same string.
    #[test]
    fn amz_date_formats_unix_seconds_as_utc() {
        assert_eq!(amz_date_from_unix(1_369_353_600), "20130524T000000Z");
        assert_eq!(amz_date_from_unix(1_440_938_160), "20150830T123600Z");
        assert_eq!(amz_date_from_unix(0), "19700101T000000Z");
        // Leap day, and a leap day in a divisible-by-400 century year.
        assert_eq!(amz_date_from_unix(1_582_934_400), "20200229T000000Z");
        assert_eq!(amz_date_from_unix(951_782_400), "20000229T000000Z");
        assert_eq!(amz_date_from_unix(1_735_689_599), "20241231T235959Z");
    }

    /// A malformed timestamp is refused up front rather than silently producing a signature that can
    /// only fail at the server as an opaque `SignatureDoesNotMatch`.
    #[test]
    fn malformed_amz_date_is_rejected_without_leaking_the_secret() {
        let creds = s3_doc_credentials();
        let signer = Signer::new(&creds, "us-east-1").unwrap();
        let err = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: "/",
                query: &[],
                headers: &[("host", "examplebucket.s3.amazonaws.com")],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "2013-05-24T00:00:00Z",
            })
            .unwrap_err();
        assert!(err.contains("YYYYMMDDTHHMMSSZ"), "{err}");
        assert!(
            !err.contains("wJalrXUtnFEMI"),
            "the secret must never reach an error message: {err}"
        );
    }

    /// The empty-body payload hash constant is what it claims to be.
    #[test]
    fn empty_payload_constant_is_sha256_of_nothing() {
        assert_eq!(sha256_hex(b""), EMPTY_PAYLOAD_SHA256);
    }
}
