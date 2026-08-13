//! S3-compatible object-store backend (epic CPE-1503) — **foundation slice** (CPE-1681).
//!
//! This crate is the 4th remote backend in the family that already holds `cpe-sftp` (SSH/SFTP),
//! `cpe-webdav` (HTTP/WebDAV) and `cpe-ftp` (FTP/FTPS), and it is deliberately shaped like them: a
//! standalone crate, synchronous, no async runtime, no C build tooling.
//!
//! What is here is the two things every later ticket in the epic sits on:
//!
//! - **[`S3Config`] and [`AddressingStyle`]** — *where* a request is addressed. The same
//!   `(bucket, key)` has to become `https://bucket.s3.us-east-1.amazonaws.com/key` against AWS and
//!   `http://localhost:9000/bucket/key` against MinIO, because AWS deprecated path-style for new buckets
//!   while most self-hosted S3 gateways only implement path-style. This one field is the whole
//!   "Backblaze B2 / Google Cloud Storage / Wasabi / MinIO come free" claim; get it wrong and half the
//!   ecosystem answers 404.
//! - **[`sigv4`]** — *how* a request is signed.
//!
//! What is **not** here, on purpose: no HTTP client, no XML parsing, no
//! `cpe_server::provider::FileSystemProvider` impl. Error mapping is CPE-1682, `list` is CPE-1683,
//! object ops are CPE-1684, `cpe_vfs::open` routing is CPE-1685 and the frontend is CPE-1686. The slice
//! stops at the point where everything is still a pure function of its inputs — which is exactly why it
//! can be verified in full against AWS's published test vectors with no network, no credentials, no
//! bucket and no Docker.
//!
//! # The secret
//! [`Credentials`] holds the one genuinely secret value in the config (it will arrive from the OS
//! keychain via CPE-1685). Its `Debug` impl redacts it, so `S3Config` can keep a derived `Debug` without
//! a secret leaking into a log line, a panic message, or an error string; see
//! [`Credentials::secret`], the single deliberate way to read it back.

pub mod sigv4;

use sigv4::{canonical_query, encode_path};

/// Access-key credentials for an S3-compatible endpoint.
///
/// The secret is private and its `Debug` is redacted. This is not security theatre: every other field in
/// [`S3Config`] is safe to print while debugging a 403, and the natural thing to reach for is
/// `dbg!(config)` — which, with a derived `Debug`, would put a live secret access key into a log the
/// user may well paste into a bug report.
#[derive(Clone)]
pub struct Credentials {
    /// The access key id (`AKIA…`). Not secret — it appears in the `Authorization` header of every
    /// signed request, in clear.
    pub access_key_id: String,
    secret_access_key: String,
}

impl Credentials {
    pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
        Credentials {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
        }
    }

    /// Read the secret access key. The only way to get it back out, named so that every use site reads
    /// as a deliberate act; the signer is the sole caller in this crate.
    pub fn secret(&self) -> &str {
        &self.secret_access_key
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<redacted>")
            .finish()
    }
}

/// How a bucket is addressed in the request URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddressingStyle {
    /// Decide per endpoint and bucket — see [`S3Config::resolved_addressing`]. The default, because it
    /// is right for both of the two cases users actually have (AWS, and a self-hosted gateway) without
    /// asking them to know the difference.
    #[default]
    Auto,
    /// `https://{bucket}.{host}/{key}` — the bucket is a DNS label on the host. What AWS requires for
    /// buckets created since 2020.
    VirtualHost,
    /// `https://{host}/{bucket}/{key}` — the bucket is the first path segment. What MinIO, Ceph RGW and
    /// most self-hosted gateways implement, and what a bare-IP or `localhost` endpoint has to use since
    /// you cannot prefix a DNS label onto an IP address.
    Path,
}

/// Where and how to reach one S3-compatible bucket.
///
/// `Debug` is derived and safe — the secret redaction lives in [`Credentials`], so it cannot be lost by
/// someone adding a field here later.
///
/// # Endpoint, region and bucket are three separate inputs
/// They have to be. The epic's original brief said the saved connection's `host` *was* the bucket, which
/// leaves nowhere to put a custom endpoint or a region — and a custom endpoint is the entire reason
/// MinIO, Backblaze B2, Wasabi and GCS come free. The settled connection-profile convention (ruled at
/// CPE-1686, wired up in CPE-1685) reads `s3://region@endpoint-host:port/bucket[/prefix]`, which maps
/// onto this struct directly: `host`+`port` → [`endpoint`](Self::endpoint) (default port 443),
/// `user` → [`region`](Self::region) (default `us-east-1`), first path segment →
/// [`bucket`](Self::bucket). Any remaining path segments are a key prefix, which is a *provider*
/// concern (CPE-1683/1684) rather than an addressing one, so there is deliberately no `prefix` field
/// here — this type answers only "what URL does `(bucket, key)` become".
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Service endpoint as an absolute URL: `https://s3.us-east-1.amazonaws.com`,
    /// `http://localhost:9000`, `https://s3.us-west-004.backblazeb2.com`. A trailing slash is fine, and
    /// a path prefix (some gateways sit under one) is preserved.
    ///
    /// Validated when a target is built, because the authority becomes the signed `Host` header: no
    /// control characters or interior whitespace (they would split the header), no `?`/`#`/`\`, no
    /// `user:password@` userinfo (it would be signed into the request and printed in `Debug`), and a
    /// numeric port if one is given. See [`S3Config::object_target`] for the errors.
    pub endpoint: String,
    /// The signing region (`us-east-1`, …). Part of the credential scope, so it must match what the
    /// server expects even on implementations that have no real regions — MinIO's default is
    /// `us-east-1`.
    pub region: String,
    /// The bucket this config addresses.
    pub bucket: String,
    /// Path-style vs virtual-host, or [`AddressingStyle::Auto`].
    pub addressing: AddressingStyle,
    pub credentials: Credentials,
}

/// A resolved request target: the URL to call, the `host` header value to sign, and the encoded path.
///
/// All three come out of one construction so they cannot disagree. That matters more than it looks:
/// SigV4 covers the canonical URI and the `host` header, so a URL built by one code path and a signature
/// computed over another produces `SignatureDoesNotMatch` with nothing in the message to say why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTarget {
    /// The absolute URL to request (no query string; the caller appends one if needed).
    pub url: String,
    /// The `Host` header value, which is also a signed header. Carries a non-default port
    /// (`localhost:9000`) and omits a default one (`:443` for https, `:80` for http) — matching what an
    /// HTTP client puts on the wire, because that is the string the server will verify against.
    pub host: String,
    /// The percent-encoded absolute path, byte-for-byte identical to the path component of `url`. This
    /// is what [`sigv4::SigningInput::encoded_path`] wants.
    pub encoded_path: String,
}

impl RequestTarget {
    /// The URL to request when the call carries query parameters, built from the **same** encode-and-sort
    /// function that produces the signed canonical query ([`sigv4::canonical_query`]).
    ///
    /// This is the query-string half of the guarantee [`url`](Self::url) already gives the path: SigV4
    /// covers the canonical query, so a request layer that assembled its own `?a=1&b=2` could encode or
    /// order a parameter differently from what it signed and get `SignatureDoesNotMatch` with nothing in
    /// the message to say why. Pass the identical `query` slice to
    /// [`sigv4::SigningInput::query`] and the two cannot disagree - one construction, two uses
    /// (CPE-1689; the caller is `ListObjectsV2` in CPE-1683).
    ///
    /// An empty `query` returns [`url`](Self::url) unchanged, so there is no dangling `?`.
    pub fn url_with_query(&self, query: &[(&str, &str)]) -> String {
        match canonical_query(query) {
            q if q.is_empty() => self.url.clone(),
            q => format!("{}?{q}", self.url),
        }
    }
}

impl S3Config {
    /// A config for AWS S3 proper in `region`: endpoint `https://s3.{region}.amazonaws.com`, addressing
    /// left on [`AddressingStyle::Auto`] (which resolves to virtual-host for a DNS-compatible bucket).
    pub fn aws(
        region: impl Into<String>,
        bucket: impl Into<String>,
        credentials: Credentials,
    ) -> Self {
        let region = region.into();
        S3Config {
            endpoint: format!("https://s3.{region}.amazonaws.com"),
            region,
            bucket: bucket.into(),
            addressing: AddressingStyle::Auto,
            credentials,
        }
    }

    /// A config for an arbitrary S3-compatible endpoint (MinIO, Ceph, B2, Wasabi, GCS's S3 shim).
    pub fn new(
        endpoint: impl Into<String>,
        region: impl Into<String>,
        bucket: impl Into<String>,
        credentials: Credentials,
    ) -> Self {
        S3Config {
            endpoint: endpoint.into(),
            region: region.into(),
            bucket: bucket.into(),
            addressing: AddressingStyle::Auto,
            credentials,
        }
    }

    /// Pin the addressing style explicitly, overriding [`AddressingStyle::Auto`].
    pub fn with_addressing(mut self, addressing: AddressingStyle) -> Self {
        self.addressing = addressing;
        self
    }

    /// The addressing style that will actually be used.
    ///
    /// [`AddressingStyle::Auto`] resolves by two questions, in order:
    ///
    /// 1. **Can this bucket be a DNS label at all?** A bucket whose name contains a dot, an underscore
    ///    or an uppercase letter — or that is not 3–63 characters — cannot safely be one. A dot is the
    ///    interesting case: `my.bucket.s3.amazonaws.com` does not match the `*.s3.amazonaws.com`
    ///    wildcard certificate, so virtual-host addressing over HTTPS fails TLS verification before it
    ///    ever reaches S3. Such a bucket gets path-style regardless of endpoint.
    /// 2. **Is this AWS?** A host under `amazonaws.com` gets virtual-host, because AWS has deprecated
    ///    path-style for buckets created since September 2020. Everything else — MinIO, Ceph RGW,
    ///    `localhost`, a bare IP — gets path-style, which is the form every S3-compatible
    ///    implementation supports even when it also supports virtual-host.
    ///
    /// The rule is deliberately conservative rather than clever: path-style is the *compatible* answer,
    /// so `Auto` only leaves it where it is actually required. A user whose gateway wants virtual-host
    /// says so with [`with_addressing`](Self::with_addressing).
    pub fn resolved_addressing(&self) -> AddressingStyle {
        match self.addressing {
            AddressingStyle::Auto => {
                if !is_dns_compatible_bucket(&self.bucket) {
                    AddressingStyle::Path
                } else if is_aws_host(self.endpoint_parts().map(|p| p.host).unwrap_or_default()) {
                    AddressingStyle::VirtualHost
                } else {
                    AddressingStyle::Path
                }
            }
            explicit => explicit,
        }
    }

    /// The request target for one object key.
    ///
    /// `key` is a raw S3 key, *not* pre-encoded. S3 keys are opaque byte strings, so anything is legal
    /// in one: spaces, `#`, `+`, `?`, emoji. Every byte outside the URL-unreserved set is percent-encoded
    /// here by the same function the signer uses for the canonical URI, so the two can never drift apart.
    ///
    /// # A leading `/` is part of the key
    /// The key is appended to the bucket root verbatim, so `"a.txt"`, `"/a.txt"` and `"//a.txt"` are
    /// three **different** objects at three different URLs — which is what they are on S3, where a
    /// leading slash is an ordinary key byte and turns up routinely from a naive path join. Until
    /// CPE-1689 this trimmed every leading slash, collapsing all three onto `/a.txt`; a user with such a
    /// key silently read or wrote the wrong object. Trimming would also contradict the crate's own
    /// stated rule that `a//b` survives unchanged — it did, in the middle, but not at the front.
    ///
    /// An **empty** key is refused rather than quietly addressing the bucket root: S3 has no zero-length
    /// key, and returning [`bucket_target`](Self::bucket_target)'s URL for one is the same
    /// silently-wrong-object failure in a different costume.
    pub fn object_target(&self, key: &str) -> Result<RequestTarget, String> {
        if key.is_empty() {
            return Err(
                "s3: object key must not be empty (S3 has no zero-length key; for the bucket root itself \
                 call bucket_target)"
                    .into(),
            );
        }
        self.target_for(&format!("/{key}"))
    }

    /// The request target for the bucket itself — what `ListObjectsV2` and bucket-level requests use.
    /// Virtual-host addressing puts the bucket in the host, so the path is just `/`; path-style puts it
    /// in the first path segment.
    pub fn bucket_target(&self) -> Result<RequestTarget, String> {
        self.target_for("/")
    }

    /// Shared construction for [`object_target`](Self::object_target) and
    /// [`bucket_target`](Self::bucket_target). `raw_path` is the `/`-rooted, unencoded key path.
    fn target_for(&self, raw_path: &str) -> Result<RequestTarget, String> {
        let parts = self.endpoint_parts()?;
        validate_bucket(&self.bucket)?;
        // The same check `sigv4::Signer::new`/`for_service` apply to a region — see that function's doc
        // for why this used to be the *only* one of the two public region paths that was guarded (CPE-1691).
        sigv4::validate_region(&self.region)?;

        let (host, path_prefix) = match self.resolved_addressing() {
            AddressingStyle::Path => (
                parts.authority.to_string(),
                format!("{}/{}", parts.path_prefix, encode_path(&self.bucket)),
            ),
            // Auto is resolved above and can never reach here; treat it as virtual-host for totality.
            AddressingStyle::VirtualHost | AddressingStyle::Auto => {
                (format!("{}.{}", self.bucket, parts.authority), parts.path_prefix.to_string())
            }
        };

        // `/` alone is the bucket root: a path-style prefix already ends without a slash, so the empty
        // suffix is right, and a virtual-host request with no prefix must still send `/`.
        let suffix = if raw_path == "/" { String::new() } else { encode_path(raw_path) };
        let encoded_path = match format!("{path_prefix}{suffix}") {
            p if p.is_empty() => "/".to_string(),
            p => p,
        };

        Ok(RequestTarget {
            url: format!("{}://{host}{encoded_path}", parts.scheme),
            host,
            encoded_path,
        })
    }

    /// Split the endpoint into scheme / authority / path prefix, or an error naming what is wrong with
    /// it. The authority is normalized by dropping the scheme's default port, because an HTTP client will
    /// not send `:443` in the `Host` header and the signature has to cover what is sent.
    ///
    /// # Why the endpoint is validated at all (CPE-1689)
    /// The authority becomes the signed `Host` header verbatim. Before this was checked, an endpoint
    /// carrying a CR/LF put a second line into the canonical request — the classic request-splitting
    /// shape — and one carrying `user:password@host` put a live password into both the signed request
    /// and `RequestTarget`'s `Debug`, in the one crate that otherwise works hard to keep credentials out
    /// of both. The bucket was validated for exactly this reason and the endpoint was not, even though it
    /// is the field that lands in a header.
    fn endpoint_parts(&self) -> Result<EndpointParts<'_>, String> {
        let endpoint = self.endpoint.trim();
        validate_endpoint_text(endpoint)?;
        let (scheme, rest) = if let Some(r) = endpoint.strip_prefix("https://") {
            ("https", r)
        } else if let Some(r) = endpoint.strip_prefix("http://") {
            ("http", r)
        } else {
            return Err(self.endpoint_shape_error());
        };
        let (authority, path_prefix) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
            None => (rest, ""),
        };
        // Userinfo was already refused by `validate_endpoint_text`, which runs before any error path
        // that could format the endpoint. This is a belt-and-braces net for a future caller that reaches
        // this function by another route; it is unreachable through `endpoint_parts` today.
        debug_assert!(!authority.contains('@'), "userinfo must be refused before the endpoint is parsed");
        let (host, port) = split_host_port(authority).ok_or_else(|| self.endpoint_shape_error())?;
        if host.is_empty() {
            return Err(self.endpoint_shape_error());
        }
        if let Some(port) = port {
            if port.parse::<u16>().is_err() {
                return Err(format!(
                    "s3: endpoint port must be a number 0-65535, got {port:?} in {:?}",
                    self.endpoint
                ));
            }
        }
        // Drop the scheme's default port from the authority the `Host` header is built from — a client
        // sends `example.com`, not `example.com:443`, and the signature must cover what is sent.
        let default_port = if scheme == "https" { ":443" } else { ":80" };
        let authority = match port {
            Some(_) => authority.strip_suffix(default_port).unwrap_or(authority),
            None => authority,
        };
        Ok(EndpointParts { scheme, authority, host, path_prefix })
    }

    fn endpoint_shape_error(&self) -> String {
        format!("s3: endpoint must be http(s)://host[:port], got {:?}", self.endpoint)
    }
}

/// Split an authority into `(host, port)`. Handles the bracketed IPv6 form (`[::1]:9000`), and splits a
/// normal authority at its **first** colon.
///
/// The first colon, not the last: `rsplit_once` read `user:pw@s3.amazonaws.com` as host `"user"`, which
/// then silently failed the AWS test and dropped to path-style addressing (CPE-1689). With this split
/// that input yields the port `"pw@s3.amazonaws.com"`, which the numeric check refuses — and the
/// userinfo check above refuses it earlier still, with a better message.
fn split_host_port(authority: &str) -> Option<(&str, Option<&str>)> {
    match authority.strip_prefix('[') {
        Some(rest) => {
            let end = rest.find(']')?;
            let host = &rest[..end];
            match &rest[end + 1..] {
                "" => Some((host, None)),
                after => Some((host, Some(after.strip_prefix(':')?))),
            }
        }
        None => match authority.split_once(':') {
            Some((host, port)) => Some((host, Some(port))),
            None => Some((authority, None)),
        },
    }
}

/// Reject text containing a byte that cannot legally appear in a URL, a `Host` header, or a SigV4
/// credential-scope element: any control character or whitespace (each could split a signed header line
/// or a hostname), and `?`, `#`, `\` (each would open a new URL component the caller did not intend).
///
/// **One validation standard, reused rather than reinvented (CPE-1691).** This is the character-class
/// check every piece of caller-supplied *structured* text must pass before it reaches the canonical
/// request or the `Authorization` header — as opposed to a free-form header **value**, which S3 lets
/// carry near-arbitrary bytes and which [`sigv4::reject_framing_bytes`] guards with a narrower rule
/// instead (framing bytes only, not an alphabet restriction). [`validate_endpoint_text`] and
/// [`validate_bucket`] both call this directly; [`sigv4::validate_region`] and the access-key-id check in
/// [`sigv4::Signer::new`] call it as `crate::validate_structural_text`. Before this ticket, `validate_bucket`
/// had its own, weaker byte-class check (`u8::is_ascii_whitespace`, which misses `\0`, `\x0b`, `\x7f` and
/// non-ASCII) — exactly the "two validators of different strictness on the same kind of input" drift this
/// function exists to close off.
///
/// `kind` names the field in the error message (`"endpoint"`, `"bucket name"`, `"region"`, ...); the check
/// itself never varies.
fn validate_structural_text(kind: &str, s: &str) -> Result<(), String> {
    if let Some(ch) = s.chars().find(|c| c.is_control() || c.is_whitespace()) {
        return Err(format!(
            "s3: {kind} must not contain control characters or whitespace (they would split a signed \
             header), got {ch:?} in {s:?}"
        ));
    }
    if let Some(ch) = s.chars().find(|c| matches!(c, '?' | '#' | '\\')) {
        return Err(format!("s3: {kind} must not contain a query, fragment or backslash, got {ch:?} in {s:?}"));
    }
    Ok(())
}

/// Reject an endpoint containing a byte that cannot legally appear in a URL that becomes a `Host` header
/// and a request path.
///
/// The counterpart of [`validate_bucket`], and for the same reason: these are the characters that would
/// *change the shape* of the request rather than merely address something odd. A CR or LF is the serious
/// one — it splits the signed `Host` header into two header lines. Surrounding whitespace is trimmed by
/// the caller before this runs, so only interior whitespace reaches here.
fn validate_endpoint_text(endpoint: &str) -> Result<(), String> {
    // The userinfo check runs FIRST, before anything below can format the endpoint into a message.
    // That ordering is the whole protection and is not stylistic — see the long note at the return.
    if endpoint.contains('@') {
        return Err(USERINFO_REFUSED.into());
    }
    validate_structural_text("endpoint", endpoint)
}

/// The refusal for an endpoint carrying userinfo. A single constant, deliberately **not** formatted with
/// the endpoint, because the whole point of refusing userinfo is that the password must not reach a signed
/// request, a `Debug` line, or — here — an error string a user will paste into a bug report.
///
/// **This check must run before every other endpoint check.** The PR #868 reviewer found the original
/// placement — after `validate_endpoint_text` and after the scheme strip — leaked the password in **six of
/// seven** realistic shapes, because those earlier paths echo `{endpoint:?}` and `#`, `?`, `\` and spaces
/// are ordinary password characters (a wrong scheme, the commonest paste error, leaked it too). Refusing
/// carefully in one branch is worth nothing if five branches upstream print the thing first. Note this only
/// covers messages produced here: `S3Config`'s derived `Debug` still prints the endpoint field verbatim,
/// which is why the field must never be allowed to hold a password in the first place.
/// The message names `@` **anywhere**, not "userinfo", because that is what the check actually does.
/// Hoisting it to the front necessarily widened it past the authority — a gateway endpoint with `@` in
/// its path prefix (`https://gw.example.com/s3@v1`) is now refused too. Narrowing it back to the
/// pre-path portion would reintroduce the ordering hazard, since the path split happens after the trim
/// and can itself be handed a malformed string, so refusing `@` anywhere is the deliberate rule — and
/// the message has to describe the rule rather than the motivation, or an operator with a legitimate
/// `@` in a path prefix reads an explanation about passwords that has nothing to do with their input.
const USERINFO_REFUSED: &str = "s3: endpoint must not contain '@' anywhere — this refuses userinfo \
                                (`https://user:password@host`), whose password would be signed into the \
                                request and printed by every Debug and error line carrying the endpoint. \
                                The rule is deliberately wider than userinfo so it can run before any \
                                check that formats the endpoint into a message. Supply credentials via \
                                S3Config::credentials; if you have a legitimate '@' in a gateway path \
                                prefix, it is not supported.";

/// The pieces of a parsed endpoint URL.
#[derive(Debug, Clone, Copy)]
struct EndpointParts<'a> {
    scheme: &'a str,
    /// `host[:port]`, with the scheme's default port removed.
    authority: &'a str,
    /// `authority` without the port — what the AWS-host test looks at.
    host: &'a str,
    /// Any path the endpoint URL carried, without a trailing slash (`""` for the common case).
    path_prefix: &'a str,
}

/// True for an endpoint host that belongs to AWS S3 proper (`s3.us-east-1.amazonaws.com`,
/// `s3-accelerate.amazonaws.com`, `s3.cn-north-1.amazonaws.com.cn`, …). Matched on the registrable
/// suffix rather than by substring, so a look-alike host like `amazonaws.com.attacker.example` is
/// **not** treated as AWS.
///
/// `amazonaws.com.cn` is the AWS China partition — genuine AWS, on a different registrable suffix
/// because the operator is a separate legal entity. Matching only `.amazonaws.com` silently dropped
/// every China-region bucket to path-style, which works but is not what the rule claims to answer
/// (CPE-1689). It is listed explicitly rather than matched loosely, so the look-alike above still fails.
fn is_aws_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    ["amazonaws.com", "amazonaws.com.cn"]
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

/// True if `bucket` can be used as a DNS label in a virtual-host URL. Deliberately stricter than S3's
/// own naming rules: a legal-but-dotted bucket name breaks the wildcard TLS certificate rather than S3
/// itself, so it must fall back to path-style.
fn is_dns_compatible_bucket(bucket: &str) -> bool {
    let len = bucket.len();
    (3..=63).contains(&len)
        && bucket
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && bucket.bytes().next().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bucket.bytes().next_back().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Reject a bucket name that cannot appear in a URL at all. Not full S3 bucket-name validation — the
/// server is the authority on that, and a client-side rule that is stricter than the server's silently
/// locks users out of buckets that really exist. This only refuses names that would *change the shape*
/// of the request (a `/` would inject a path segment; whitespace or a `?`/`#` would break the URL) — the
/// same standard [`validate_endpoint_text`] applies, via the shared [`validate_structural_text`], so the
/// two cannot drift back into different strictness (CPE-1691; see that function's doc for what used to
/// slip through here: `\0`, `\x0b`, `\x7f`, non-ASCII).
///
/// `@` and `:` are refused on top of the shared standard, added at CPE-1689: under an **explicit**
/// [`AddressingStyle::VirtualHost`] the bucket is pasted in front of the endpoint authority, so
/// `evil@attacker.example` would build the host `evil@attacker.example.s3.amazonaws.com` — userinfo
/// smuggled in through the other end of the same URL that [`validate_endpoint_text`] guards. It is
/// unreachable through [`AddressingStyle::Auto`] (such a bucket is not DNS-compatible and resolves to
/// path-style, where the bucket is percent-encoded), so this is defence in depth rather than a live bug.
fn validate_bucket(bucket: &str) -> Result<(), String> {
    if bucket.is_empty() {
        return Err("s3: bucket must not be empty".into());
    }
    validate_structural_text("bucket name", bucket)?;
    if bucket.bytes().any(|b| matches!(b, b'/' | b'@' | b':')) {
        return Err(format!("s3: bucket name must not contain a path or URL separator, got {bucket:?}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sigv4::{Signer, SigningInput, EMPTY_PAYLOAD_SHA256};

    fn creds() -> Credentials {
        Credentials::new("AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY")
    }

    // ---------------------------------------------------------------------------------------------
    // Addressing: the same (bucket, key) under both styles.
    // ---------------------------------------------------------------------------------------------

    /// The headline requirement, stated as one test: one `(bucket, key)` pair, two addressing styles,
    /// two different URLs — AWS virtual-host and MinIO path-style.
    #[test]
    fn the_same_bucket_and_key_address_differently_under_each_style() {
        let aws = S3Config::aws("us-east-1", "my-bucket", creds());
        assert_eq!(aws.resolved_addressing(), AddressingStyle::VirtualHost);
        assert_eq!(
            aws.object_target("photos/cat.jpg").unwrap().url,
            "https://my-bucket.s3.us-east-1.amazonaws.com/photos/cat.jpg"
        );

        let minio = S3Config::new("http://localhost:9000", "us-east-1", "my-bucket", creds());
        assert_eq!(minio.resolved_addressing(), AddressingStyle::Path);
        assert_eq!(
            minio.object_target("photos/cat.jpg").unwrap().url,
            "http://localhost:9000/my-bucket/photos/cat.jpg"
        );
    }

    /// The `host` header that gets signed carries a non-default port and drops a default one — it has to
    /// match what the HTTP client actually sends or every request fails to verify.
    #[test]
    fn signed_host_header_matches_what_a_client_would_send() {
        let minio = S3Config::new("http://localhost:9000", "us-east-1", "my-bucket", creds());
        assert_eq!(minio.object_target("k").unwrap().host, "localhost:9000");

        let aws = S3Config::aws("eu-west-2", "my-bucket", creds());
        assert_eq!(aws.object_target("k").unwrap().host, "my-bucket.s3.eu-west-2.amazonaws.com");

        // An explicitly-written default port is dropped, because a client will not send it either.
        let explicit = S3Config::new("https://s3.example.com:443", "us-east-1", "my-bucket", creds())
            .with_addressing(AddressingStyle::Path);
        assert_eq!(explicit.object_target("k").unwrap().host, "s3.example.com");
        assert_eq!(explicit.object_target("k").unwrap().url, "https://s3.example.com/my-bucket/k");
    }

    /// A key needing percent-encoding survives into both the URL and the path that gets signed —
    /// identically, since one encoder produces both.
    #[test]
    fn keys_needing_percent_encoding_are_encoded_the_same_way_in_both_styles() {
        let key = "holiday photos/mount blanc (2024)+raw#1.jpg";

        let aws = S3Config::aws("us-east-1", "my-bucket", creds());
        let t = aws.object_target(key).unwrap();
        assert_eq!(
            t.url,
            "https://my-bucket.s3.us-east-1.amazonaws.com/holiday%20photos/mount%20blanc%20%282024%29%2Braw%231.jpg"
        );
        assert!(t.url.ends_with(&t.encoded_path), "the URL must end with exactly the signed path");

        let minio = S3Config::new("http://localhost:9000", "us-east-1", "my-bucket", creds());
        let t2 = minio.object_target(key).unwrap();
        assert_eq!(
            t2.url,
            "http://localhost:9000/my-bucket/holiday%20photos/mount%20blanc%20%282024%29%2Braw%231.jpg"
        );
        // `/` inside a key stays a separator; everything else is escaped.
        assert!(t2.encoded_path.starts_with("/my-bucket/holiday%20photos/"));
    }

    /// D1, the one that could silently touch the wrong object: a leading `/` is an ordinary key byte, so
    /// `a.txt`, `/a.txt`, `//a.txt` and `///a.txt` are four different objects at four different URLs.
    /// Before CPE-1689 `trim_start_matches('/')` collapsed all four onto `/a.txt`.
    #[test]
    fn leading_slashes_are_part_of_the_key_and_never_collapse_onto_one_object() {
        let aws = S3Config::aws("us-east-1", "my-bucket", creds());
        let url = |key: &str| aws.object_target(key).unwrap().url;

        assert_eq!(url("a.txt"), "https://my-bucket.s3.us-east-1.amazonaws.com/a.txt");
        assert_eq!(url("/a.txt"), "https://my-bucket.s3.us-east-1.amazonaws.com//a.txt");
        assert_eq!(url("//a.txt"), "https://my-bucket.s3.us-east-1.amazonaws.com///a.txt");
        assert_eq!(url("///a.txt"), "https://my-bucket.s3.us-east-1.amazonaws.com////a.txt");

        // Stated as the property, not just the four strings: all four are distinct.
        let urls = ["a.txt", "/a.txt", "//a.txt", "///a.txt"].map(url);
        for (i, a) in urls.iter().enumerate() {
            for b in urls.iter().skip(i + 1) {
                assert_ne!(a, b, "two keys that differ only in leading slashes addressed one object");
            }
        }

        // The signature follows the path, so the collision was a signing collision too.
        let sign = |key: &str| {
            let t = aws.object_target(key).unwrap();
            Signer::new(&aws.credentials, &aws.region)
                .unwrap()
                .sign(&SigningInput {
                    method: "GET",
                    encoded_path: &t.encoded_path,
                    query: &[],
                    headers: &[("host", &t.host)],
                    payload_hash: EMPTY_PAYLOAD_SHA256,
                    amz_date: "20130524T000000Z",
                })
                .unwrap()
                .signature
        };
        assert_ne!(sign("a.txt"), sign("/a.txt"));

        // Path-style addressing keeps them distinct as well, after the bucket segment.
        let minio = S3Config::new("http://localhost:9000", "us-east-1", "my-bucket", creds());
        assert_eq!(minio.object_target("/a.txt").unwrap().url, "http://localhost:9000/my-bucket//a.txt");
        assert_eq!(minio.object_target("a.txt").unwrap().url, "http://localhost:9000/my-bucket/a.txt");
    }

    /// The other way a key could quietly address the wrong thing: an empty key used to return the bucket
    /// root's URL. S3 has no zero-length key, so it is refused by name.
    #[test]
    fn an_empty_key_is_refused_instead_of_addressing_the_bucket_root() {
        let aws = S3Config::aws("us-east-1", "my-bucket", creds());
        let err = aws.object_target("").unwrap_err();
        assert!(err.contains("object key must not be empty"), "{err}");
        assert!(err.contains("bucket_target"), "the error must say what to call instead: {err}");
    }

    /// The bucket root: virtual-host puts the bucket in the host and asks for `/`; path-style asks for
    /// `/{bucket}`. This is the URL `ListObjectsV2` will use in CPE-1683.
    #[test]
    fn bucket_root_target_differs_by_style() {
        let aws = S3Config::aws("us-east-1", "my-bucket", creds());
        let t = aws.bucket_target().unwrap();
        assert_eq!(t.url, "https://my-bucket.s3.us-east-1.amazonaws.com/");
        assert_eq!(t.encoded_path, "/");

        let minio = S3Config::new("http://localhost:9000", "us-east-1", "my-bucket", creds());
        let t = minio.bucket_target().unwrap();
        assert_eq!(t.url, "http://localhost:9000/my-bucket");
        assert_eq!(t.encoded_path, "/my-bucket");
    }

    /// `Auto` chooses per endpoint and bucket; an explicit style always wins.
    #[test]
    fn auto_addressing_picks_virtual_host_only_for_aws_with_a_dns_safe_bucket() {
        // AWS + DNS-safe bucket -> virtual-host.
        assert_eq!(
            S3Config::aws("us-east-1", "my-bucket", creds()).resolved_addressing(),
            AddressingStyle::VirtualHost
        );
        // AWS + dotted bucket -> path-style, because `my.bucket.s3.amazonaws.com` does not match the
        // `*.s3.amazonaws.com` wildcard certificate.
        assert_eq!(
            S3Config::aws("us-east-1", "my.bucket", creds()).resolved_addressing(),
            AddressingStyle::Path
        );
        // A look-alike host is not AWS.
        assert_eq!(
            S3Config::new("https://amazonaws.com.attacker.example", "us-east-1", "b1", creds())
                .resolved_addressing(),
            AddressingStyle::Path
        );
        // Bare IP, uppercase bucket, too-short bucket -> path-style.
        assert_eq!(
            S3Config::new("http://192.168.1.10:9000", "us-east-1", "my-bucket", creds())
                .resolved_addressing(),
            AddressingStyle::Path
        );
        assert_eq!(
            S3Config::aws("us-east-1", "MyBucket", creds()).resolved_addressing(),
            AddressingStyle::Path
        );
        assert_eq!(
            S3Config::aws("us-east-1", "ab", creds()).resolved_addressing(),
            AddressingStyle::Path
        );
        // An explicit choice overrides both questions.
        assert_eq!(
            S3Config::aws("us-east-1", "my-bucket", creds())
                .with_addressing(AddressingStyle::Path)
                .resolved_addressing(),
            AddressingStyle::Path
        );
        assert_eq!(
            S3Config::new("http://localhost:9000", "us-east-1", "my-bucket", creds())
                .with_addressing(AddressingStyle::VirtualHost)
                .object_target("k")
                .unwrap()
                .url,
            "http://my-bucket.localhost:9000/k"
        );
    }

    /// An endpoint that carries a path prefix (some gateways sit under one) keeps it, ahead of the
    /// bucket segment in path-style.
    #[test]
    fn endpoint_path_prefix_is_preserved() {
        let cfg = S3Config::new("https://gw.example.com/s3/", "us-east-1", "my-bucket", creds());
        let t = cfg.object_target("a/b.txt").unwrap();
        assert_eq!(t.url, "https://gw.example.com/s3/my-bucket/a/b.txt");
        assert_eq!(cfg.bucket_target().unwrap().url, "https://gw.example.com/s3/my-bucket");
    }

    // ---------------------------------------------------------------------------------------------
    // Validation and secret handling.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn malformed_endpoints_regions_and_buckets_are_refused() {
        let bad_scheme = S3Config::new("ftp://example.com", "us-east-1", "b1", creds());
        assert!(bad_scheme.object_target("k").unwrap_err().contains("http(s)://host"));

        let no_host = S3Config::new("https://", "us-east-1", "b1", creds());
        assert!(no_host.object_target("k").unwrap_err().contains("http(s)://host"));

        let no_region = S3Config::new("https://s3.example.com", "  ", "b1", creds());
        assert!(no_region.object_target("k").unwrap_err().contains("region"));

        let no_bucket = S3Config::new("https://s3.example.com", "us-east-1", "", creds());
        assert!(no_bucket.object_target("k").unwrap_err().contains("bucket must not be empty"));

        let slashed = S3Config::new("https://s3.example.com", "us-east-1", "a/b", creds());
        assert!(slashed.object_target("k").unwrap_err().contains("path or URL separator"));
    }

    /// D2, the request-splitting shape: a CR/LF in the endpoint would have put a second header line into
    /// the signed canonical request. It is refused, and the test asserts the injected text reaches
    /// neither the URL nor the canonical request — the two places it could have done damage.
    #[test]
    fn an_endpoint_with_control_characters_is_refused_and_nothing_reaches_the_canonical_request() {
        let cfg = S3Config::new(
            "https://s3.example.com\r\nX-Injected: 1",
            "us-east-1",
            "my-bucket",
            creds(),
        );
        let err = cfg.object_target("k").unwrap_err();
        assert!(err.contains("control characters or whitespace"), "{err}");

        // Nothing was produced at all, so there is no target to build a URL from or to sign.
        assert!(cfg.bucket_target().is_err());
        // The refusal is the only thing that comes back, and it carries no raw CR/LF of its own — the
        // echoed endpoint is `Debug`-escaped, so even the error string cannot split a line.
        assert!(!err.contains('\r') && !err.contains('\n'), "the error itself carries a raw CR/LF: {err:?}");

        // A bare tab, a vertical tab, a NUL and an interior space are refused by the same rule.
        for bad in [
            "https://s3.example\tcom",
            "https://s3.example\u{0b}com",
            "https://s3.example\0com",
            "https://s3.example com",
            "https://s3.example\u{85}com",
        ] {
            let cfg = S3Config::new(bad, "us-east-1", "my-bucket", creds());
            assert!(cfg.object_target("k").is_err(), "accepted control/whitespace endpoint {bad:?}");
        }

        // Surrounding whitespace is still trimmed rather than refused — that was always allowed and is
        // not what splits a header. (Not AWS, so this resolves to path-style and the host is the
        // endpoint authority unchanged.)
        let padded = S3Config::new("  https://s3.example.com\r\n", "us-east-1", "my-bucket", creds());
        assert_eq!(padded.object_target("k").unwrap().host, "s3.example.com");
    }

    /// D2's credential half: an endpoint carrying userinfo is refused, and the password appears in
    /// neither the error message nor any `Debug` output — the two channels the crate otherwise keeps
    /// secrets out of. Also pins the host split, which used to take everything before the **last** colon
    /// and so read this endpoint's host as `"user"`.
    #[test]
    fn an_endpoint_with_userinfo_is_refused_without_echoing_the_password() {
        let cfg = S3Config::new("https://user:hunter2@s3.amazonaws.com", "us-east-1", "b1", creds());
        let err = cfg.object_target("k").unwrap_err();
        assert!(err.contains("userinfo"), "{err}");
        assert!(!err.contains("hunter2"), "the password leaked into the error message: {err}");

        // No `RequestTarget` is ever built, so the password reaches neither a signed canonical request
        // nor `RequestTarget`'s `Debug` — the two places D2 found it. (`S3Config`'s own derived `Debug`
        // still echoes back whatever string the user put in `endpoint`; that is exactly why this input
        // is refused instead of parsed.)
        assert!(cfg.object_target("k").is_err() && cfg.bucket_target().is_err());
        assert!(!format!("{:?}", cfg.object_target("k")).contains("hunter2"));

        // The same input under the old last-colon split parsed its host as "user"; nothing accepts it now.
        assert!(S3Config::new("https://user@s3.amazonaws.com", "us-east-1", "b1", creds())
            .object_target("k")
            .unwrap_err()
            .contains("userinfo"));

        // The one above is the shape that happened to be checked. The password is only safe if EVERY
        // shape is refused by the userinfo branch rather than by some earlier check that formats the
        // endpoint into its message — which is what the PR #868 reviewer found, in six of these seven.
        // `#`, `?`, `\` and space are ordinary password characters, and a wrong scheme is the commonest
        // paste error of the lot; each one used to reach a path that echoed `{endpoint:?}` verbatim.
        //
        // Asserting on "userinfo" and not merely on `is_err()` is the point: every one of these is an
        // error either way, so `is_err()` would have passed against the leaking version. The assertion
        // has to name WHICH refusal fired.
        for endpoint in [
            "https://user:hunter2@s3.example.com",
            "s3://user:hunter2@s3.example.com",      // wrong scheme — the shape-error path
            "https://user:hun#ter2@s3.example.com",  // '#' — the query/fragment path
            "https://user:hun?ter2@s3.example.com",  // '?' — same path
            "https://user:hun\\ter2@s3.example.com", // '\' — same path
            "https://user:hun ter2@s3.example.com",  // space — the control/whitespace path
            "https://user:hunter2/x@s3.example.com", // non-numeric port path
        ] {
            let cfg = S3Config::new(endpoint, "us-east-1", "b1", creds());
            let err = cfg.object_target("k").unwrap_err();
            assert!(err.contains("userinfo"), "{endpoint} was refused by the wrong check: {err}");
            assert!(!err.contains("hunter2"), "the password leaked from {endpoint}: {err}");
        }

        // The four the reviewer added after the fix, each of which previously routed through a
        // *different* echoing branch: the trim path, the control-character path, the authority `@`
        // check, and the shape error. They are here because catching them one at a time is what the old
        // code was doing; the hoist catches them structurally, and this pins that difference.
        for endpoint in [
            "  https://user:hunter2@s3.example.com  ", // surrounding whitespace, trimmed first
            "https://user:hun\r\nter2@s3.example.com", // CRLF inside the password
            "https://user@s3.example.com",             // bare user, no password at all
            "HTTPS://user:hunter2@s3.example.com",      // wrong-case scheme *and* userinfo
        ] {
            let err = S3Config::new(endpoint, "us-east-1", "b1", creds())
                .object_target("k")
                .unwrap_err();
            assert!(err.contains("userinfo"), "{endpoint:?} was refused by the wrong check: {err}");
            assert!(!err.contains("hunter2"), "the password leaked from {endpoint:?}: {err}");
        }
    }

    /// The authority is split at its **first** colon, so a port is a port and everything else is
    /// refused rather than silently reinterpreted. Bracketed IPv6 literals keep their own colons.
    #[test]
    fn the_host_and_port_split_takes_the_first_colon_and_rejects_a_non_numeric_port() {
        // Not an AWS host, so this is path-style and `host` is the endpoint authority itself.
        let plain = S3Config::new("https://s3.example.com:9000", "us-east-1", "my-bucket", creds());
        assert_eq!(plain.object_target("k").unwrap().host, "s3.example.com:9000");

        let ipv6 = S3Config::new("http://[::1]:9000", "us-east-1", "my-bucket", creds())
            .with_addressing(AddressingStyle::Path);
        let t = ipv6.object_target("k").unwrap();
        assert_eq!(t.host, "[::1]:9000");
        assert_eq!(t.url, "http://[::1]:9000/my-bucket/k");
        // A bracketed literal on the default port drops it, like any other host.
        let ipv6_default = S3Config::new("http://[::1]:80", "us-east-1", "my-bucket", creds())
            .with_addressing(AddressingStyle::Path);
        assert_eq!(ipv6_default.object_target("k").unwrap().host, "[::1]");

        for bad in ["https://s3.example.com:port", "https://s3.example.com:99999", "https://s3.example.com:"] {
            let cfg = S3Config::new(bad, "us-east-1", "my-bucket", creds());
            let err = cfg.object_target("k").unwrap_err();
            assert!(err.contains("port must be a number"), "for {bad:?}: {err}");
        }
    }

    /// The split itself, asserted directly, because through the public API the first-colon rule is
    /// masked: the input that used to expose it (`user:pw@host`, read as host `"user"`) is now refused
    /// by the userinfo check before the split runs, and every other two-colon authority is refused by
    /// the numeric-port check whichever colon it split on. This is the assertion that goes red on its
    /// own if the `split_once` here ever becomes `rsplit_once` again.
    #[test]
    fn the_authority_split_takes_the_first_colon() {
        assert_eq!(split_host_port("s3.example.com"), Some(("s3.example.com", None)));
        assert_eq!(split_host_port("s3.example.com:9000"), Some(("s3.example.com", Some("9000"))));
        // Two colons: host is everything before the *first*. `rsplit_once` said `("a:b", "c")`.
        assert_eq!(split_host_port("a:b:c"), Some(("a", Some("b:c"))));
        assert_eq!(split_host_port("user:pw@s3.amazonaws.com"), Some(("user", Some("pw@s3.amazonaws.com"))));
        // Bracketed IPv6 keeps its own colons and yields the port after the bracket, if any.
        assert_eq!(split_host_port("[::1]"), Some(("::1", None)));
        assert_eq!(split_host_port("[::1]:9000"), Some(("::1", Some("9000"))));
        assert_eq!(split_host_port("[2001:db8::1]:80"), Some(("2001:db8::1", Some("80"))));
        // Malformed bracket forms have no answer rather than a guessed one.
        assert_eq!(split_host_port("[::1"), None);
        assert_eq!(split_host_port("[::1]9000"), None);
    }

    /// An endpoint carrying a query or fragment would break the URL the same way a `?` in a bucket name
    /// does — the bucket was already guarded against it and the endpoint was not.
    #[test]
    fn an_endpoint_with_a_query_fragment_or_backslash_is_refused() {
        for bad in ["https://s3.example.com?x=1", "https://s3.example.com#f", "https://s3.example.com\\a"] {
            let cfg = S3Config::new(bad, "us-east-1", "my-bucket", creds());
            let err = cfg.object_target("k").unwrap_err();
            assert!(err.contains("query, fragment or backslash"), "for {bad:?}: {err}");
        }
    }

    /// D5: AWS China is AWS. `s3.cn-north-1.amazonaws.com.cn` sits on a different registrable suffix
    /// because the partition is operated by a separate entity, and matching only `.amazonaws.com`
    /// silently dropped every China-region bucket to path-style. The look-alike host must still fail.
    #[test]
    fn aws_china_endpoints_are_recognised_as_aws_but_look_alikes_are_not() {
        let style = |endpoint: &str| {
            S3Config::new(endpoint, "cn-north-1", "my-bucket", creds()).resolved_addressing()
        };
        assert_eq!(style("https://s3.us-east-1.amazonaws.com"), AddressingStyle::VirtualHost);
        assert_eq!(style("https://s3.cn-north-1.amazonaws.com.cn"), AddressingStyle::VirtualHost);
        assert_eq!(style("https://s3.cn-northwest-1.amazonaws.com.cn"), AddressingStyle::VirtualHost);
        assert_eq!(style("https://amazonaws.com.cn"), AddressingStyle::VirtualHost);
        // Not AWS: a look-alike that merely contains the suffix, and one that extends it.
        assert_eq!(style("https://amazonaws.com.attacker.example"), AddressingStyle::Path);
        assert_eq!(style("https://amazonaws.com.cn.attacker.example"), AddressingStyle::Path);
        assert_eq!(style("https://notamazonaws.com"), AddressingStyle::Path);
        assert_eq!(style("https://evil-amazonaws.com.cn"), AddressingStyle::Path);

        // …and the whole point: a China-region bucket gets a virtual-host URL.
        let cn = S3Config::new("https://s3.cn-north-1.amazonaws.com.cn", "cn-north-1", "my-bucket", creds());
        assert_eq!(
            cn.object_target("a.txt").unwrap().url,
            "https://my-bucket.s3.cn-north-1.amazonaws.com.cn/a.txt"
        );
    }

    /// D6: `@` and `:` in a bucket name are userinfo and a port smuggled in through the other end of the
    /// URL. Not reachable through `Auto` (such a name is not DNS-compatible, so it goes path-style and
    /// gets percent-encoded), but an explicit virtual-host style pastes the bucket in front of the host.
    #[test]
    fn a_bucket_name_may_not_carry_userinfo_or_a_port() {
        for bad in ["evil@attacker.example", "host:9000", "a@b"] {
            let cfg = S3Config::new("https://s3.amazonaws.com", "us-east-1", bad, creds())
                .with_addressing(AddressingStyle::VirtualHost);
            let err = cfg.object_target("k").unwrap_err();
            assert!(err.contains("path or URL separator"), "for {bad:?}: {err}");
        }
    }

    /// The class item 3 of CPE-1691 documented as still open after CPE-1689: `\0`, `\x0b`, `\x7f` and
    /// non-ASCII all used to pass `validate_bucket`'s own `is_ascii_whitespace`-based check while
    /// `validate_endpoint_text` already refused every one of them (`\r`/`\n` happened to be caught only
    /// because they are ASCII whitespace). This pins `validate_bucket` to the **same** standard so the two
    /// cannot drift apart again: every byte `validate_endpoint_text` refuses in an otherwise-bare hostname
    /// (no `@`, `?`, `#`, `\` of its own — those are exercised elsewhere) must also refuse a bucket, driven
    /// through the public `object_target` under an explicit `VirtualHost` style so the bucket text reaches
    /// a real signed request the same way item 3's exploit did.
    #[test]
    fn validate_bucket_refuses_every_byte_validate_endpoint_text_refuses() {
        let bad_bytes = ['\0', '\u{0b}', '\u{7f}', '\u{85}', '\r', '\n', '\t', ' '];
        for ch in bad_bytes {
            let bucket = format!("a{ch}b");
            let endpoint = format!("https://s3.{ch}example.com");

            let endpoint_err = S3Config::new(&endpoint, "us-east-1", "my-bucket", creds())
                .object_target("k")
                .unwrap_err();
            assert!(
                endpoint_err.contains("control characters or whitespace"),
                "validate_endpoint_text no longer refuses {:?} (U+{:04X}) — the shared standard moved \
                 without this test noticing: {endpoint_err}",
                ch,
                ch as u32
            );

            let bucket_err = S3Config::new("https://s3.amazonaws.com", "us-east-1", &bucket, creds())
                .with_addressing(AddressingStyle::VirtualHost)
                .object_target("k")
                .unwrap_err();
            assert!(
                bucket_err.contains("control characters or whitespace"),
                "validate_bucket accepted {:?} (U+{:04X}) that validate_endpoint_text refuses — the two \
                 validators have drifted apart again: {bucket_err}",
                ch,
                ch as u32
            );
        }
    }

    /// The other half of the drift pin: `validate_bucket`'s own extra restrictions (`/`, `@`, `:`, added
    /// at CPE-1689 for reasons `validate_endpoint_text` does not share — see that function's doc) are
    /// still refused after routing the shared bytes through `validate_structural_text`.
    #[test]
    fn validate_bucket_still_refuses_its_own_extra_bytes_after_the_shared_refactor() {
        for bad in ["a/b", "evil@attacker.example", "host:9000"] {
            let cfg = S3Config::new("https://s3.amazonaws.com", "us-east-1", bad, creds())
                .with_addressing(AddressingStyle::VirtualHost);
            let err = cfg.object_target("k").unwrap_err();
            assert!(err.contains("path or URL separator"), "for {bad:?}: {err}");
        }
    }

    /// AC2's other public path: `S3Config::target_for` (via `object_target`/`bucket_target`) refuses an
    /// empty or control-character region the same way `Signer::new` does — see the sibling test in
    /// `sigv4.rs`, `signer_new_and_for_service_refuse_an_empty_or_control_character_region_the_same_way`.
    /// Before CPE-1691 `target_for` was the *only* one of the two that checked anything, so this is the
    /// test that closes the second door.
    #[test]
    fn a_control_character_or_empty_region_is_refused_through_target_for_too() {
        let bad_region = S3Config::new("https://s3.example.com", "us-east-1\r\nX-Injected: 1", "b1", creds());
        let err = bad_region.object_target("k").unwrap_err();
        assert!(err.contains("control characters or whitespace"), "{err}");

        let empty_region = S3Config::new("https://s3.example.com", "", "b1", creds());
        assert!(empty_region.object_target("k").unwrap_err().contains("region must not be empty"));

        // A normal region still works.
        assert!(
            S3Config::new("https://s3.example.com", "us-east-1", "b1", creds())
                .object_target("k")
                .is_ok()
        );
    }

    /// The secret must not appear in any `Debug` output — not on `Credentials`, not on the `S3Config`
    /// that contains it, and not on the `Signer` that borrows it.
    ///
    /// Both `{:?}` and `{:#?}` are checked. They are different code paths through a derived `Debug`, and
    /// only the compact one was covered before CPE-1689; a guard that leaves half the formatter untested
    /// is half a guard. `Signer` is covered because it is `pub` and derives `Debug`: it is safe today
    /// only because it holds a `&Credentials`, and nothing was watching the day someone stores an owned
    /// secret on it instead (D7).
    #[test]
    fn debug_output_never_contains_the_secret() {
        let cfg = S3Config::aws("us-east-1", "my-bucket", creds());
        let signer = Signer::new(&cfg.credentials, &cfg.region).unwrap();
        let rendered = [
            format!("{cfg:?}"),
            format!("{cfg:#?}"),
            format!("{:?}", creds()),
            format!("{:#?}", creds()),
            format!("{signer:?}"),
            format!("{signer:#?}"),
        ];
        for r in &rendered {
            assert!(!r.contains("wJalrXUtnFEMI"), "secret leaked into Debug: {r}");
            assert!(r.contains("<redacted>"), "{r}");
            // The access key id is not secret and stays visible — it is in every Authorization header.
            assert!(r.contains("AKIAIOSFODNN7EXAMPLE"), "{r}");
        }
    }

    /// D8, the query half of the "one encoder, cannot drift" guarantee. The URL a request layer sends and
    /// the canonical query it signs come from the same [`sigv4::canonical_query`] call, so the on-wire
    /// query is byte-for-byte the third line of the canonical request. This is the ListObjectsV2 shape
    /// CPE-1683 needs, written down before it has a chance to rebuild the string by hand.
    #[test]
    fn the_sent_query_and_the_signed_query_come_from_one_construction() {
        let cfg = S3Config::new("https://s3.amazonaws.com", "us-east-1", "examplebucket", creds());
        let target = cfg.bucket_target().unwrap();
        // Deliberately out of canonical order and needing encoding: a `/` in a value must become `%2F`.
        let query = [("prefix", "holiday photos/"), ("max-keys", "2"), ("list-type", "2")];

        let url = target.url_with_query(&query);
        let signed = Signer::new(&cfg.credentials, &cfg.region)
            .unwrap()
            .sign(&SigningInput {
                method: "GET",
                encoded_path: &target.encoded_path,
                query: &query,
                headers: &[("host", &target.host)],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap();

        let canonical_line = signed.canonical_request.split('\n').nth(2).unwrap();
        assert_eq!(canonical_line, "list-type=2&max-keys=2&prefix=holiday%20photos%2F");
        assert_eq!(url, format!("{}?{canonical_line}", target.url));
        assert_eq!(
            url,
            "https://examplebucket.s3.amazonaws.com/?list-type=2&max-keys=2&prefix=holiday%20photos%2F"
        );
        // No query, no dangling `?`.
        assert_eq!(target.url_with_query(&[]), target.url);
    }

    // ---------------------------------------------------------------------------------------------
    // The two halves together.
    // ---------------------------------------------------------------------------------------------

    /// The seam this whole slice exists to make safe: the path that goes into the signature is the exact
    /// path that goes onto the wire, and the signed `host` is the exact host that will be contacted.
    /// Reproduces the AWS "GET Object" example end to end, starting from an [`S3Config`] instead of
    /// hand-written strings — so a change to the addressing code that broke signing would show up here.
    #[test]
    fn a_config_built_target_signs_to_the_published_aws_example() {
        let cfg = S3Config::new(
            "https://s3.amazonaws.com",
            "us-east-1",
            "examplebucket",
            Credentials::new("AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"),
        );
        assert_eq!(cfg.resolved_addressing(), AddressingStyle::VirtualHost);
        let target = cfg.object_target("test.txt").unwrap();
        assert_eq!(target.host, "examplebucket.s3.amazonaws.com");
        assert_eq!(target.url, "https://examplebucket.s3.amazonaws.com/test.txt");

        let signer = Signer::new(&cfg.credentials, &cfg.region).unwrap();
        let signed = signer
            .sign(&SigningInput {
                method: "GET",
                encoded_path: &target.encoded_path,
                query: &[],
                headers: &[
                    ("host", &target.host),
                    ("range", "bytes=0-9"),
                    ("x-amz-content-sha256", EMPTY_PAYLOAD_SHA256),
                    ("x-amz-date", "20130524T000000Z"),
                ],
                payload_hash: EMPTY_PAYLOAD_SHA256,
                amz_date: "20130524T000000Z",
            })
            .unwrap();
        assert_eq!(
            signed.signature,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    /// Flipping the addressing style changes what gets signed, because it changes both the signed host
    /// and the signed path. Stated as a test so the two halves cannot be wired together loosely later.
    ///
    /// The assertion is on the two **canonical requests**, byte for byte. It used to be a bare
    /// `assert_ne!` on the signatures, which would pass on almost any wrong implementation — two
    /// different wrong hosts differ just as reliably as two right ones. The canonical request is the
    /// exact input the signature is a pure function of, and the published vectors in `sigv4` pin that
    /// function, so pinning this text pins the signatures with it. The `assert_ne!` is kept as the
    /// supplementary check it always was.
    #[test]
    fn flipping_the_addressing_style_changes_what_is_signed() {
        let base = S3Config::new("https://s3.amazonaws.com", "us-east-1", "examplebucket", creds());
        let sign_with = |style: AddressingStyle| {
            let cfg = base.clone().with_addressing(style);
            let t = cfg.object_target("test.txt").unwrap();
            let signer = Signer::new(&cfg.credentials, &cfg.region).unwrap();
            let signed = signer
                .sign(&SigningInput {
                    method: "GET",
                    encoded_path: &t.encoded_path,
                    query: &[],
                    headers: &[("host", &t.host), ("x-amz-date", "20130524T000000Z")],
                    payload_hash: EMPTY_PAYLOAD_SHA256,
                    amz_date: "20130524T000000Z",
                })
                .unwrap();
            (signed.canonical_request, signed.signature)
        };

        let (virtual_host, vh_signature) = sign_with(AddressingStyle::VirtualHost);
        assert_eq!(
            virtual_host,
            concat!(
                "GET\n",
                "/test.txt\n",
                "\n",
                "host:examplebucket.s3.amazonaws.com\n",
                "x-amz-date:20130524T000000Z\n",
                "\n",
                "host;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )
        );

        let (path_style, path_signature) = sign_with(AddressingStyle::Path);
        assert_eq!(
            path_style,
            concat!(
                "GET\n",
                "/examplebucket/test.txt\n",
                "\n",
                "host:s3.amazonaws.com\n",
                "x-amz-date:20130524T000000Z\n",
                "\n",
                "host;x-amz-date\n",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )
        );

        assert_ne!(vh_signature, path_signature);
    }
}
