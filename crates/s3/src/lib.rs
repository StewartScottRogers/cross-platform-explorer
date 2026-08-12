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

use sigv4::encode_path;

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
    /// `key` is a raw S3 key — no leading slash, and *not* pre-encoded. S3 keys are opaque byte strings,
    /// so anything is legal in one: spaces, `#`, `+`, `?`, emoji. Every byte outside the URL-unreserved
    /// set is percent-encoded here by the same function the signer uses for the canonical URI, so the
    /// two can never drift apart.
    pub fn object_target(&self, key: &str) -> Result<RequestTarget, String> {
        self.target_for(&format!("/{}", key.trim_start_matches('/')))
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
        let parts = self
            .endpoint_parts()
            .ok_or_else(|| format!("s3: endpoint must be http(s)://host[:port], got {:?}", self.endpoint))?;
        validate_bucket(&self.bucket)?;
        if self.region.trim().is_empty() {
            return Err("s3: region must not be empty (it is part of the SigV4 credential scope)".into());
        }

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

    /// Split the endpoint into scheme / authority / path prefix, or `None` if it is not an absolute
    /// http(s) URL. The authority is normalized by dropping the scheme's default port, because an HTTP
    /// client will not send `:443` in the `Host` header and the signature has to cover what is sent.
    fn endpoint_parts(&self) -> Option<EndpointParts<'_>> {
        let endpoint = self.endpoint.trim();
        let (scheme, rest) = if let Some(r) = endpoint.strip_prefix("https://") {
            ("https", r)
        } else {
            ("http", endpoint.strip_prefix("http://")?)
        };
        let (authority, path_prefix) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
            None => (rest, ""),
        };
        let default_port = if scheme == "https" { ":443" } else { ":80" };
        let authority = authority.strip_suffix(default_port).unwrap_or(authority);
        if authority.is_empty() {
            return None;
        }
        let host = authority.rsplit_once(':').map(|(h, _)| h).unwrap_or(authority);
        Some(EndpointParts { scheme, authority, host, path_prefix })
    }
}

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
/// `s3-accelerate.amazonaws.com`, …). Matched on the registrable suffix rather than by substring, so a
/// look-alike host like `amazonaws.com.attacker.example` is **not** treated as AWS.
fn is_aws_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "amazonaws.com" || host.ends_with(".amazonaws.com")
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
/// of the request (a `/` would inject a path segment; whitespace or a `?`/`#` would break the URL).
fn validate_bucket(bucket: &str) -> Result<(), String> {
    if bucket.is_empty() {
        return Err("s3: bucket must not be empty".into());
    }
    if bucket.bytes().any(|b| matches!(b, b'/' | b'?' | b'#' | b'\\') || b.is_ascii_whitespace()) {
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

    /// The secret must not appear in any `Debug` output — not on `Credentials`, and not on the
    /// `S3Config` that contains it.
    #[test]
    fn debug_output_never_contains_the_secret() {
        let cfg = S3Config::aws("us-east-1", "my-bucket", creds());
        let rendered = format!("{cfg:?}");
        assert!(!rendered.contains("wJalrXUtnFEMI"), "secret leaked into Debug: {rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
        // The access key id is not secret and stays visible — it is in every Authorization header.
        assert!(rendered.contains("AKIAIOSFODNN7EXAMPLE"), "{rendered}");
        assert!(!format!("{:?}", creds()).contains("wJalrXUtnFEMI"));
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

        let signer = Signer::new(&cfg.credentials, &cfg.region);
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

    /// Flipping the addressing style changes the signature, because it changes both the signed host and
    /// the signed path. Stated as a test so the two halves cannot be wired together loosely later.
    #[test]
    fn flipping_the_addressing_style_changes_the_signature() {
        let base = S3Config::new("https://s3.amazonaws.com", "us-east-1", "examplebucket", creds());
        let sign_with = |style: AddressingStyle| {
            let cfg = base.clone().with_addressing(style);
            let t = cfg.object_target("test.txt").unwrap();
            let signer = Signer::new(&cfg.credentials, &cfg.region);
            signer
                .sign(&SigningInput {
                    method: "GET",
                    encoded_path: &t.encoded_path,
                    query: &[],
                    headers: &[("host", &t.host), ("x-amz-date", "20130524T000000Z")],
                    payload_hash: EMPTY_PAYLOAD_SHA256,
                    amz_date: "20130524T000000Z",
                })
                .unwrap()
                .signature
        };
        assert_ne!(sign_with(AddressingStyle::VirtualHost), sign_with(AddressingStyle::Path));
    }
}
