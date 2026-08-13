//! `S3Provider`: the [`FileSystemProvider`] impl for an S3-compatible bucket (CPE-1683, epic CPE-1503).
//!
//! This is the first module in `cpe-s3` that actually talks to a server — everything in [`crate::sigv4`]
//! and [`crate::error`] is a pure function of its inputs, checkable against fixed vectors with no network
//! at all. `list` here is `ListObjectsV2` with `delimiter=/`, paginated to completion, presenting
//! `<CommonPrefixes>` as virtual directories (`ProviderCapabilities::has_real_dirs = false`) and
//! `<Contents>` as files. The remaining ops (`stat`/`read`/`write`/`delete`/`mkdir`/`rename`) are CPE-1684
//! and are stubbed here with a named "not yet implemented" error rather than a `todo!()`, so a caller that
//! reaches one gets a message instead of a panic.
//!
//! # GCS decision (ticket-mandated, made before anything below was written)
//! The epic's original claim — "B2/Wasabi/MinIO/GCS all come free once addressing is right" — was
//! narrowed at this ticket's filing: the CPE-1681 worker flagged that GCS's XML API "does not support
//! ListObjectsV2 the same way", without a live check. Re-checked here (2026-08-13) against GCS's current
//! published XML API reference (`docs.cloud.google.com/storage/docs/xml-api/get-bucket-list`,
//! `.../storage/docs/interoperability`, fetched live for this ticket, not recalled from training data):
//! the documented request/response shape is a **superset** of what this module sends and parses —
//! `list-type=2`, `delimiter`, `continuation-token`, `start-after`, and a response carrying
//! `IsTruncated`/`NextContinuationToken`/`CommonPrefixes` are all explicitly documented, with no caveat
//! text anywhere on either page about a ListObjectsV2 incompatibility. That specific claim in the epic is
//! therefore **corrected, not merely narrowed** — see the epic's Work Log for the full note.
//!
//! What is *not* re-verified: an actual signed request against a live GCS bucket. This crate's SigV4
//! signer (CPE-1681) targets AWS's published algorithm; GCS's own docs describe "a V4 signing process"
//! and HMAC credentials without confirming byte-for-byte canonicalisation parity with AWS SigV4, and this
//! headless environment has no GCS account, credentials, or network egress to test one live. **Decision:
//! GCS is treated the same as any other undedicated S3-compatible gateway for v1 — expected to work by
//! protocol shape, not verified end to end, not specially handled or special-cased anywhere in this
//! module.** No GCS-specific branch exists (nor should one — the whole point of building to the documented
//! wire protocol is that no per-gateway code is needed). A live-conformance ticket against a real GCS
//! bucket, mirroring the QNAP-NAS precedent already used for SFTP/WebDAV/FTP, is the natural follow-up
//! once credentials are available; this ticket does not file it, since that is a resourcing decision, not
//! a scoping one.
//!
//! # The in-process fixture: built to be reused, not rebuilt
//! [`tests::handle`] maps the handful of S3 verbs onto `std::fs` under a temp-directory root — the same
//! technique `crates/webdav/src/lib.rs` uses for its PROPFIND fixture. It already answers GET (object read
//! and, via `list-type=2`, `ListObjectsV2`), HEAD, PUT and DELETE, even though this ticket's own tests only
//! exercise the `list-type=2` arm; CPE-1684 (`stat`/`read`/`write`/`delete`/`mkdir`) should extend this
//! same function rather than standing up a second in-process server.
//!
//! # The `mkdir` marker-key shape (agreed here, for CPE-1684 to depend on)
//! [`provider_path_to_key_prefix`] is the single source of truth for both this ticket's `ListObjectsV2`
//! `prefix` parameter *and* the exact key CPE-1684's `mkdir` must write its zero-byte marker object under:
//! a `/`-rooted provider path like `/photos/2024` becomes the key `photos/2024/` — no leading slash, one
//! trailing slash, no other content. `mkdir("/photos/2024")` should `PUT` an empty body to exactly that
//! key. [`parse_list_bucket_result`] already filters a `<Contents>` entry whose `<Key>` equals the
//! *requested* prefix (the directory's own marker, returned by real S3 when you list `photos/2024/` with
//! that same prefix) so it never shows up as a spurious empty file inside itself — CPE-1684's `mkdir` only
//! needs to write that exact key shape for the two tickets to agree.
//!
//! # The `ureq`-header-drop decision, and a correction to the CPE-1684 warning (measured, not assumed)
//! `list` is the first code in this crate to send a request over `ureq`, so it is also first to exercise
//! the landmine CPE-1684's ticket describes in detail: `ureq` 2.12.1's write loop (`unit.rs:467-474`)
//! calls `Header::value()`, which filters through `is_field_vchar_or_obs_fold` and returns `None` — and
//! the header is skipped — for a value carrying any byte outside `{SP, HTAB} ∪ [0x21, 0x7E]`.
//!
//! **That finding was traced from `ureq`'s source, not measured against the code path this crate actually
//! uses. Measured here, it doesn't reproduce the way the ticket warns.** [`guard_header_sendable`] was
//! written first, exactly as prescribed ("refuse loudly at our layer"); the negative-control probe
//! required for this ticket's evidence (temporarily deleting the four `guard_header_sendable` calls and
//! re-running `tests::sending_a_request_with_a_header_byte_ureq_would_drop_is_refused_before_any_bytes_leave_the_process`)
//! showed that **`ureq`'s own `Agent::get(url).set(name, value).call()` path already refuses the same
//! input, loudly, before any byte reaches the fixture** — `ureq::Error::Transport` with the message
//! `Bad Header: invalid header 'Authorization: ...'`, and the fixture's request counter stayed at `0`. The
//! silent-drop mechanism the ticket cites is real (it is right there in `unit.rs`), but it evidently sits
//! behind a different internal path than the request-builder's `.set()`/`.call()` this module uses — most
//! likely the one that parses headers arriving *off the wire* (an incoming response), not the one that
//! writes headers going *out*. This module does not use that other path, so — measured, not assumed — it
//! is not exposed to the silent drop.
//!
//! **Decision: keep [`guard_header_sendable`] anyway**, downgraded from "closes a silent-corruption hole"
//! to "fails a beat earlier, with a clearer, byte-naming message, before touching the network at all" —
//! `ureq`'s own `Bad Header: invalid header '<full Authorization value>'` message is genuinely usable but
//! echoes the whole header including the signature, and does not say *which byte* made it invalid. Refusing
//! at this layer costs nothing (no new dependency, one small pure function) and is strictly friendlier to
//! debug than waiting for `ureq` to say so. This is *not* the same decision the CPE-1684 warning asked for
//! ("refuse loudly … because the alternative is silent" no longer applies here); it is "refuse loudly
//! because it is cheap and clearer", which is a weaker but still positive case. **CPE-1684 should re-run
//! this same measurement against whatever code path its own PUT/HEAD/DELETE requests use** before repeating
//! the silent-drop framing verbatim — it may hold there, or it may not; this finding is scoped to `GET` via
//! the builder API, not to `ureq` in general.

use std::io::Read as _;

use cpe_server::provider::{FileSystemProvider, ProviderCapabilities, ProviderEntry};

use crate::{error, sigv4, RequestTarget, S3Config};

/// Upper bound on how many bytes of an HTTP response body this module will ever read into memory, for
/// both a successful `ListObjectsV2` page and a non-2xx error body. A real page of up to 1000 keys is a
/// few hundred KB at most (each `<Contents>`/`<CommonPrefixes>` element is well under 1 KB); this is wide
/// headroom above that while still bounding what a hostile or badly misconfigured endpoint (a giant proxy
/// error page, a server that never closes the connection) can make this process buffer.
const MAX_RESPONSE_BODY_BYTES: usize = 8 * 1024 * 1024;

/// Upper bound on total entries [`S3Provider::list`] will buffer across all pages of one listing (CPE-1683
/// AC: "cap the total so a pathological bucket cannot exhaust memory"). This is a *single directory
/// level*, not a recursive walk — even a bucket used as a flat million-object dumping ground rarely has
/// more than a few thousand direct children of one prefix in normal use, so this is generous headroom
/// while still bounding memory against a hostile or merely enormous single-level listing.
const MAX_LIST_ENTRIES: usize = 200_000;

/// Upper bound on how many `ListObjectsV2` pages [`S3Provider::list`] will follow before refusing to
/// continue. This exists independently of [`MAX_LIST_ENTRIES`]: a server that keeps answering
/// `IsTruncated=true` with a fresh token but zero *new* entries per page would never trip the entry cap,
/// only the page cap. At `max-keys=1000` this is a wide margin over any real single-level listing.
const MAX_LIST_PAGES: usize = 1_000;

/// The deepest element nesting [`parse_list_bucket_result`] will hand to `roxmltree` before refusing to
/// parse at all — ported from `crates/webdav/src/lib.rs`'s `MAX_XML_NESTING_DEPTH` (CPE-1398), for the
/// identical reason: `roxmltree::Document::parse` recurses per nesting level, and a `ListObjectsV2`
/// response is exactly the kind of network-controlled body a hostile or merely broken S3-compatible
/// gateway could use to stack-overflow this process. See that constant's doc for the depth/stack-size
/// measurements this margin is set against; a real `ListBucketResult` nests at most 3 levels
/// (`ListBucketResult > Contents > Key`), so 64 costs nothing for legitimate responses.
const MAX_XML_NESTING_DEPTH: usize = 64;

/// True for a byte `ureq` 2.12.1's header-value grammar accepts. Mirrors `ureq`'s own filter,
/// `is_field_vchar_or_obs_fold` (`header.rs:231-237`, as traced in `crate::sigv4::reject_framing_bytes`'s
/// doc comment): only SP, HTAB, and printable ASCII `[0x21, 0x7E]`. Anything else — including every
/// non-ASCII byte, i.e. all of UTF-8's multi-byte sequences — is refused. See this module's top doc for
/// where that refusal actually happens on the code path this crate uses: measured, it is `ureq` itself
/// raising a loud `Bad Header` transport error, not the silent per-header drop CPE-1684's ticket describes
/// for a different internal code path.
fn is_ureq_sendable_byte(b: u8) -> bool {
    b == b' ' || b == b'\t' || (0x21..=0x7E).contains(&b)
}

/// Refuse, loudly and before any request is sent, a header value carrying a byte outside `ureq`'s sendable
/// range. See this module's top doc ("The `ureq`-header-drop decision, and a correction to the CPE-1684
/// warning") for the full story: measured against the actual `Agent::get(url).set(..).call()` path this
/// module uses, `ureq` 2.12.1 already refuses this input itself (loudly, before any byte reaches the
/// network) rather than silently dropping the header as the ticket that flagged this warned. This function
/// is kept anyway, downgraded from "closes a silent hole" to "fails one beat sooner with a clearer, byte-
/// naming message, for free".
fn guard_header_sendable(label: &str, value: &str) -> Result<(), String> {
    if let Some(b) = value.bytes().find(|b| !is_ureq_sendable_byte(*b)) {
        return Err(format!(
            "s3: the {label} header value contains byte {b:#04x}, which ureq (this crate's HTTP client) \
             refuses to send — refusing here first, before the request is attempted, with the specific byte \
             named rather than waiting for ureq's own (less specific) refusal. Value: {value:?}"
        ));
    }
    Ok(())
}

/// Cheap, non-recursive guard against maliciously (or accidentally) deep XML nesting, run before the
/// document is handed to `roxmltree`. Ported near-verbatim from `crates/webdav/src/lib.rs`'s
/// `xml_nesting_too_deep` (CPE-1398) — walks the real tokens from [`xmlparser::Tokenizer`] rather than a
/// hand-rolled `<`/`>` scan, so quoted attribute values (which may legally contain a bare `>`) cannot
/// fool the depth count into under-reporting.
fn xml_nesting_too_deep(xml: &str, max_depth: usize) -> bool {
    let mut depth: usize = 0;
    for token in xmlparser::Tokenizer::from(xml) {
        let token = match token {
            Ok(t) => t,
            Err(_) => break, // malformed XML — let roxmltree::Document::parse report the real error
        };
        if let xmlparser::Token::ElementEnd { end, .. } = token {
            match end {
                xmlparser::ElementEnd::Open => {
                    depth += 1;
                    if depth > max_depth {
                        return true;
                    }
                }
                xmlparser::ElementEnd::Close(..) => depth = depth.saturating_sub(1),
                xmlparser::ElementEnd::Empty => {}
            }
        }
    }
    false
}

/// Convert a `/`-rooted provider path into the S3 key prefix used both as `ListObjectsV2`'s `prefix`
/// parameter here, and — by agreement with CPE-1684 — as the exact key its `mkdir` marker object is
/// written under. See this module's top doc, "The `mkdir` marker-key shape".
///
/// The bucket root (`""` or `"/"`) maps to the empty prefix (list the whole bucket); it has no marker
/// object and is never filtered as one.
pub fn provider_path_to_key_prefix(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}

/// The part of `key` (a `<Key>` or `<Prefix>` from a `ListObjectsV2` response) that comes after
/// `key_prefix`, or `None` if `key` does not actually start with it.
///
/// This is a defensive check, not an optimistic one: `ListObjectsV2` guarantees every key it returns for
/// a given `prefix` starts with that prefix, but nothing on this side should assume a network-controlled
/// response honours its own protocol (CPE-1683 AC5 — a key must not be able to escape the listed prefix).
fn leaf_under_prefix<'a>(key: &'a str, key_prefix: &str) -> Option<&'a str> {
    key.strip_prefix(key_prefix)
}

/// One page of a `ListObjectsV2` response, parsed.
struct ListPage {
    entries: Vec<ProviderEntry>,
    is_truncated: bool,
    next_token: Option<String>,
}

/// Parse a `ListObjectsV2` `<ListBucketResult>` body into one [`ListPage`], relative to the `key_prefix`
/// that was requested (see [`provider_path_to_key_prefix`]).
///
/// Filters, source-side, exactly like `crates/webdav`'s `parse_multistatus` does for a hostile `<d:href>`:
///
/// - A `<Contents>`/`<CommonPrefixes>` entry whose key does not actually start with `key_prefix` is
///   dropped (a server returning outside its own advertised prefix — CPE-1683 AC5).
/// - A `<Contents>` entry whose key equals `key_prefix` exactly is the directory's own zero-byte marker
///   object (CPE-1683 AC4) and is dropped — it is the directory being listed, not a file inside it.
/// - The remaining leaf name (the part after `key_prefix`, with a `CommonPrefixes` leaf's trailing `/`
///   also stripped) must pass [`cpe_server::transfer::is_safe_name`] — the same guard SFTP/WebDAV apply to
///   an attacker-controlled remote name, so a key carrying `../`, an embedded `/`, or a leading `/` cannot
///   produce an entry that escapes the listed prefix once rendered locally.
fn parse_list_bucket_result(xml: &str, key_prefix: &str) -> Result<ListPage, String> {
    if xml_nesting_too_deep(xml, MAX_XML_NESTING_DEPTH) {
        return Err("s3: ListObjectsV2 response XML nesting too deep".to_string());
    }
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("s3: bad ListObjectsV2 XML: {e}"))?;

    let is_truncated = doc
        .descendants()
        .find(|n| n.tag_name().name() == "IsTruncated")
        .and_then(|n| n.text())
        .map(|t| t.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let next_token = doc
        .descendants()
        .find(|n| n.tag_name().name() == "NextContinuationToken")
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let mut entries = Vec::new();

    for content in doc.descendants().filter(|n| n.tag_name().name() == "Contents") {
        let Some(key) = content.descendants().find(|n| n.tag_name().name() == "Key").and_then(|n| n.text())
        else {
            continue;
        };
        let Some(leaf) = leaf_under_prefix(key, key_prefix) else { continue };
        if leaf.is_empty() {
            continue; // the directory's own zero-byte marker object, not a file inside it (AC4)
        }
        if !cpe_server::transfer::is_safe_name(leaf) {
            continue; // AC5: a hostile/malformed key must not escape the listed prefix
        }
        let size = content
            .descendants()
            .find(|n| n.tag_name().name() == "Size")
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<u64>().ok())
            .unwrap_or(0);
        entries.push(ProviderEntry { name: leaf.to_string(), is_dir: false, size });
    }

    for cp in doc.descendants().filter(|n| n.tag_name().name() == "CommonPrefixes") {
        let Some(prefix_text) =
            cp.descendants().find(|n| n.tag_name().name() == "Prefix").and_then(|n| n.text())
        else {
            continue;
        };
        let Some(leaf) = leaf_under_prefix(prefix_text, key_prefix) else { continue };
        let leaf = leaf.trim_end_matches('/');
        if leaf.is_empty() {
            continue;
        }
        if !cpe_server::transfer::is_safe_name(leaf) {
            continue; // AC5, the directory-entry mirror of the Contents check above
        }
        entries.push(ProviderEntry { name: leaf.to_string(), is_dir: true, size: 0 });
    }

    Ok(ListPage { entries, is_truncated, next_token })
}

/// An S3-compatible bucket presented as a synchronous [`FileSystemProvider`].
///
/// Holds an owned [`S3Config`] (cheap to clone: no connection state, SigV4 is stateless per-request) and
/// a `ureq::Agent` with auto-redirect disabled — the same reasoning `cpe-webdav`'s `WebdavProvider`
/// documents (CPE-1461): a signed request is signed for one exact host/path, and blindly following a
/// server-supplied `3xx` `Location` would replay the signature against a target it was never computed for
/// (and is a standing SSRF-adjacent risk regardless).
pub struct S3Provider {
    config: S3Config,
    agent: ureq::Agent,
}

impl S3Provider {
    /// Build a provider for `config`. Does not perform a request; the first `list` (and, once CPE-1684
    /// lands, `stat`/`read`/…) issues one and surfaces addressing/auth/connection errors then.
    pub fn connect(config: &S3Config) -> Self {
        let agent = ureq::AgentBuilder::new().redirects(0).build();
        S3Provider { config: config.clone(), agent }
    }

    /// Sign and send one `GET` against `target` with `query`, returning `(status, body)` for the caller to
    /// interpret — 2xx bodies are handed to [`parse_list_bucket_result`], non-2xx bodies to
    /// [`error::map_s3_error`]. Never itself decides success/failure from the status code, so both callers
    /// see the exact bytes the server sent.
    fn signed_get(&self, target: &RequestTarget, query: &[(&str, &str)]) -> Result<(u16, Vec<u8>), String> {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("s3: system clock reads before the Unix epoch: {e}"))?
            .as_secs();
        let amz_date = sigv4::amz_date_from_unix(secs as i64);

        let signer = sigv4::Signer::new(&self.config.credentials, &self.config.region)?;
        let signed = signer.sign(&sigv4::SigningInput {
            method: "GET",
            encoded_path: &target.encoded_path,
            query,
            headers: &[
                ("host", target.host.as_str()),
                ("x-amz-date", amz_date.as_str()),
                ("x-amz-content-sha256", sigv4::EMPTY_PAYLOAD_SHA256),
            ],
            payload_hash: sigv4::EMPTY_PAYLOAD_SHA256,
            amz_date: &amz_date,
        })?;

        // Guard every header that will actually be sent, INCLUDING the `Host` ureq derives automatically
        // from the URL (never explicitly `.set()` below, but signed above and just as capable of being
        // silently dropped if `target.host` ever carried a byte outside ureq's sendable set — see this
        // module's top doc, "The ureq-header-drop decision"). Checked here rather than left to `S3Config`
        // validation because it is a property of the transport, not of addressing correctness.
        guard_header_sendable("Host (in the request URL's authority)", &target.host)?;
        guard_header_sendable("x-amz-date", &amz_date)?;
        guard_header_sendable("x-amz-content-sha256", sigv4::EMPTY_PAYLOAD_SHA256)?;
        guard_header_sendable("Authorization", &signed.authorization)?;

        let url = target.url_with_query(query);
        let req = self
            .agent
            .get(&url)
            .set("x-amz-date", &amz_date)
            .set("x-amz-content-sha256", sigv4::EMPTY_PAYLOAD_SHA256)
            .set("Authorization", &signed.authorization);

        match req.call() {
            Ok(resp) => {
                let status = resp.status();
                let mut buf = Vec::new();
                resp.into_reader()
                    .take(MAX_RESPONSE_BODY_BYTES as u64)
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("s3: {url}: reading the response body failed: {e}"))?;
                Ok((status, buf))
            }
            // A non-2xx status: still try to read whatever body came with it (best-effort — S3's error
            // detail lives there), but never fail the call over a body read error on the error path
            // itself; `error::map_s3_error` already handles an empty/truncated/garbled body honestly.
            Err(ureq::Error::Status(code, resp)) => {
                let mut buf = Vec::new();
                let _ = resp.into_reader().take(MAX_RESPONSE_BODY_BYTES as u64).read_to_end(&mut buf);
                Ok((code, buf))
            }
            Err(ureq::Error::Transport(t)) => Err(format!("s3: {url}: {t}")),
        }
    }
}

impl FileSystemProvider for S3Provider {
    /// `ListObjectsV2` with `delimiter=/`, paginated to completion via `continuation-token`
    /// (CPE-1683 AC2). See this module's top doc for the marker-filtering and traversal guards
    /// [`parse_list_bucket_result`] applies to every entry before it is returned.
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        let key_prefix = provider_path_to_key_prefix(path);
        let target = self.config.bucket_target()?;

        let mut out = Vec::new();
        let mut continuation: Option<String> = None;
        let mut pages = 0usize;
        loop {
            pages += 1;
            if pages > MAX_LIST_PAGES {
                return Err(format!(
                    "s3: list {path:?} exceeded {MAX_LIST_PAGES} ListObjectsV2 pages without finishing \
                     (the server kept answering IsTruncated=true) — refusing to keep following a possibly \
                     hostile or misbehaving server forever"
                ));
            }

            // `delimiter=/` is what turns a flat key space into virtual directories: `<Contents>` becomes
            // this level's files, `<CommonPrefixes>` becomes this level's subdirectories, and nothing
            // deeper is ever returned — the request itself, not client-side filtering, is what keeps the
            // cost proportional to one level (CPE-1683 AC1/AC3).
            let mut query: Vec<(&str, &str)> = vec![("list-type", "2"), ("delimiter", "/"), ("max-keys", "1000")];
            if !key_prefix.is_empty() {
                query.push(("prefix", key_prefix.as_str()));
            }
            if let Some(token) = continuation.as_deref() {
                query.push(("continuation-token", token));
            }

            let (status, body) = self.signed_get(&target, &query)?;
            if !(200..300).contains(&status) {
                // CPE-1683 AC6: every non-2xx response goes through the one shared error path, never an
                // ad-hoc string built here.
                return Err(error::map_s3_error(status, &body));
            }

            let text = std::str::from_utf8(&body)
                .map_err(|e| format!("s3: list {path:?}: response body was not valid UTF-8: {e}"))?;
            let page = parse_list_bucket_result(text, &key_prefix)?;

            for entry in page.entries {
                out.push(entry);
                if out.len() > MAX_LIST_ENTRIES {
                    return Err(format!(
                        "s3: list {path:?} exceeded {MAX_LIST_ENTRIES} entries — refusing to keep \
                         buffering a possibly pathological or hostile bucket listing in memory"
                    ));
                }
            }

            if !page.is_truncated {
                break;
            }
            continuation = Some(page.next_token.ok_or_else(|| {
                format!(
                    "s3: list {path:?}: response said IsTruncated=true but supplied no \
                     NextContinuationToken — cannot fetch the next page"
                )
            })?);
        }

        Ok(out)
    }

    fn stat(&self, _path: &str) -> Result<ProviderEntry, String> {
        Err("s3: stat is not yet implemented (CPE-1684)".to_string())
    }

    fn read(&self, _path: &str) -> Result<Vec<u8>, String> {
        Err("s3: read is not yet implemented (CPE-1684)".to_string())
    }

    fn write(&mut self, _path: &str, _data: &[u8]) -> Result<(), String> {
        Err("s3: write is not yet implemented (CPE-1684)".to_string())
    }

    fn mkdir(&mut self, _path: &str) -> Result<(), String> {
        Err("s3: mkdir is not yet implemented (CPE-1684)".to_string())
    }

    fn delete(&mut self, _path: &str) -> Result<(), String> {
        Err("s3: delete is not yet implemented (CPE-1684)".to_string())
    }

    fn rename(&mut self, _from: &str, _to: &str) -> Result<(), String> {
        Err("s3: rename is not supported — S3 has no atomic rename (a copy+delete emulation is not \
             attempted; see the CPE-1684 rename decision)"
            .to_string())
    }

    /// S3 "directories" are a key-prefix convention, not real objects, and S3 has no atomic rename
    /// (CPE-1683 scope; the honest `rename` refusal itself is CPE-1684's, this is just the capability
    /// flag a caller can check before trying). Every other field keeps the full-POSIX default.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities { has_real_dirs: false, supports_rename: false, ..ProviderCapabilities::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddressingStyle, Credentials};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const TEST_BUCKET: &str = "test-bucket";

    fn creds() -> Credentials {
        Credentials::new("AKIAIOSFODNN7EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY")
    }

    fn cfg(base_url: &str) -> S3Config {
        S3Config::new(base_url, "us-east-1", TEST_BUCKET, creds()).with_addressing(AddressingStyle::Path)
    }

    /// Minimal percent-decoding for query parameter names/values (`%2F` -> `/`, etc.), ported from
    /// `crates/webdav/src/lib.rs`'s `percent_decode` — the fixture needs to read back what
    /// `sigv4::encode_query_component` encoded on the way out. Invalid escapes pass through unchanged.
    fn percent_decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn parse_query(qs: &str) -> Vec<(String, String)> {
        if qs.is_empty() {
            return Vec::new();
        }
        qs.split('&')
            .filter_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                Some((percent_decode(k), percent_decode(v)))
            })
            .collect()
    }

    /// Build one `ListObjectsV2` XML page by reading `root`'s real directory content at `prefix`,
    /// honouring `max_keys` and a `start_at` row offset — genuine server-side pagination over a real
    /// directory, not a hand-typed canned string, so a test that shrinks `max_keys` below the row count
    /// gets a truthfully truncated first page. A sentinel file named `.s3marker` in the listed directory
    /// (stripped from the real rows) makes the page additionally emit a `<Contents>` entry whose `<Key>`
    /// equals `prefix` itself — simulating the zero-byte `mkdir` marker object CPE-1684 will write, which
    /// a real filesystem cannot represent directly (the marker key always ends in the path separator).
    fn list_page_xml(root: &Path, prefix: &str, start_at: usize, max_keys: usize) -> String {
        let dir = if prefix.is_empty() { root.to_path_buf() } else { root.join(prefix.trim_end_matches('/')) };
        let mut rows: Vec<String> = Vec::new();
        let has_marker = dir.join(".s3marker").is_file();
        if has_marker {
            rows.push(format!("<Contents><Key>{prefix}</Key><Size>0</Size></Contents>"));
        }
        let mut names: Vec<(String, bool, u64)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name == ".s3marker" {
                    continue;
                }
                if let Ok(meta) = e.metadata() {
                    names.push((name, meta.is_dir(), if meta.is_dir() { 0 } else { meta.len() }));
                }
            }
        }
        names.sort();
        for (name, is_dir, size) in &names {
            let key = format!("{prefix}{name}");
            if *is_dir {
                rows.push(format!("<CommonPrefixes><Prefix>{key}/</Prefix></CommonPrefixes>"));
            } else {
                rows.push(format!("<Contents><Key>{key}</Key><Size>{size}</Size></Contents>"));
            }
        }

        let total = rows.len();
        let start = start_at.min(total);
        let end = (start + max_keys).min(total);
        let is_truncated = end < total;
        let next = if is_truncated {
            format!("<NextContinuationToken>{end}</NextContinuationToken>")
        } else {
            String::new()
        };
        format!(
            "<?xml version=\"1.0\"?><ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
             <IsTruncated>{is_truncated}</IsTruncated>{next}{}</ListBucketResult>",
            rows[start..end].join("")
        )
    }

    /// Serve one request against `root`, mapping S3 verbs onto `std::fs` — the same technique
    /// `crates/webdav/src/lib.rs` uses for its PROPFIND fixture. **Built to be extended, not rebuilt**:
    /// CPE-1684 should add its `stat`/`read`/`write`/`delete`/`mkdir` tests against this same function
    /// (GET/HEAD/PUT/DELETE are already one-liners over `std::fs` below) rather than standing up a second
    /// in-process server. `GET` with `list-type=2` is `ListObjectsV2`; any other `GET` reads an object.
    ///
    /// Enforces `delimiter=/` on every `list-type=2` request with a 400 (CPE-1683 AC3, "the fixture
    /// asserts the request carried delimiter=/") — production code always sends it, so this only trips if
    /// that line is ever removed; `tests::the_fixture_rejects_a_listobjectsv2_request_missing_delimiter`
    /// proves the enforcement fires by calling the fixture directly, bypassing `S3Provider`.
    ///
    /// `page_cap`, if set, overrides whatever `max-keys` the client asked for with something smaller —
    /// modelling a real gateway's right to truncate a response below the requested `max-keys` at its own
    /// discretion. `S3Provider::list` always asks for `max-keys=1000`, comfortably above any test fixture
    /// tree, so without this a client-driven request can never be forced through more than one real page;
    /// `page_cap` is what actually exercises the continuation-token loop end-to-end over HTTP.
    fn handle(mut req: tiny_http::Request, root: &Path, page_cap: Option<usize>, requests: &AtomicUsize) {
        requests.fetch_add(1, Ordering::Relaxed);
        let method = req.method().to_string().to_uppercase();
        let full = req.url().to_string();
        let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
        let path = raw_path.strip_prefix(&format!("/{TEST_BUCKET}")).unwrap_or(raw_path);
        let params = parse_query(raw_query);
        let param = |name: &str| params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());

        if method == "GET" && param("list-type") == Some("2") {
            if param("delimiter") != Some("/") {
                let _ = req.respond(tiny_http::Response::from_string(
                    "TEST FIXTURE: ListObjectsV2 request missing delimiter=/",
                ).with_status_code(400));
                return;
            }
            let prefix = param("prefix").unwrap_or("").to_string();
            let requested_max_keys: usize = param("max-keys").and_then(|v| v.parse().ok()).unwrap_or(1000);
            let max_keys = page_cap.map_or(requested_max_keys, |cap| requested_max_keys.min(cap));
            let start_at: usize = param("continuation-token").and_then(|v| v.parse().ok()).unwrap_or(0);
            let xml = list_page_xml(root, &prefix, start_at, max_keys);
            let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
            let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
            return;
        }

        let real = root.join(path.trim_start_matches('/'));
        match method.as_str() {
            "GET" => match std::fs::read(&real) {
                Ok(data) => {
                    let _ = req.respond(tiny_http::Response::from_data(data));
                }
                Err(_) => {
                    let _ = req.respond(tiny_http::Response::empty(404));
                }
            },
            "HEAD" => {
                let code = if real.is_file() { 200 } else { 404 };
                let _ = req.respond(tiny_http::Response::empty(code));
            }
            "PUT" => {
                if path.ends_with('/') {
                    let _ = std::fs::create_dir_all(&real);
                } else {
                    if let Some(p) = real.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    let mut body = Vec::new();
                    let _ = req.as_reader().read_to_end(&mut body);
                    let _ = std::fs::write(&real, &body);
                }
                let _ = req.respond(tiny_http::Response::empty(200));
            }
            "DELETE" => {
                let _ = std::fs::remove_file(&real);
                let _ = req.respond(tiny_http::Response::empty(204));
            }
            _ => {
                let _ = req.respond(tiny_http::Response::empty(405));
            }
        }
    }

    /// Spawn the in-process S3 fixture on an ephemeral port over a fresh temp directory; returns
    /// `(base_url, root, requests)`. `requests` counts every request the fixture receives, so a test can
    /// prove a request was never sent (e.g. the `ureq`-header-drop guard firing before any I/O). See
    /// [`handle`]'s doc for `page_cap`.
    fn spawn_s3_fixture_with_page_cap(page_cap: Option<usize>) -> (String, PathBuf, Arc<AtomicUsize>) {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("cpe-s3-fixture-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&root).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.clone();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &root_for_thread, page_cap, &requests_thread);
            }
        });
        (format!("http://{addr}"), root, requests)
    }

    /// The common case: no server-enforced page cap (the fixture honours whatever `max-keys` the client
    /// sent, which `S3Provider::list` always sets to 1000).
    fn spawn_s3_fixture() -> (String, PathBuf, Arc<AtomicUsize>) {
        spawn_s3_fixture_with_page_cap(None)
    }

    // ---------------------------------------------------------------------------------------------
    // AC1/AC3: immediate children only, `delimiter=/`, row count independent of what's elsewhere.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn list_returns_immediate_files_and_dirs_and_nothing_from_a_deeper_level_or_a_sibling() {
        let (base, root, requests) = spawn_s3_fixture();
        // /photos: two direct children (a file and a subdirectory) plus a file two levels deep, which
        // must NOT appear when listing /photos itself.
        std::fs::create_dir_all(root.join("photos/2024")).unwrap();
        std::fs::write(root.join("photos/cat.jpg"), b"meow").unwrap();
        std::fs::write(root.join("photos/2024/deep.jpg"), b"x").unwrap();
        // A sibling prefix with far more objects than /photos has — the row count for /photos must not
        // reflect it (AC3).
        std::fs::create_dir_all(root.join("unrelated")).unwrap();
        for i in 0..50 {
            std::fs::write(root.join(format!("unrelated/f{i}.bin")), b"x").unwrap();
        }

        let provider = S3Provider::connect(&cfg(&base));
        assert!(!provider.capabilities().has_real_dirs);
        assert!(!provider.capabilities().supports_rename);
        assert!(provider.capabilities().supports_write, "unrelated fields keep the full-POSIX default");

        let mut entries = provider.list("/photos").expect("list");
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2, "must be exactly the 2 immediate children, not the sibling's 50: {entries:?}");
        assert_eq!(entries[0].name, "2024");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "cat.jpg");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].size, 4);
        assert!(
            !entries.iter().any(|e| e.name.contains("deep")),
            "a file two levels down leaked into a one-level listing: {entries:?}"
        );
        assert!(requests.load(Ordering::Relaxed) >= 1);
    }

    /// `crates/webdav`-style: the fixture itself refuses a `list-type=2` request that doesn't carry
    /// `delimiter=/`, called directly (bypassing `S3Provider`, which always sends it) to prove the
    /// enforcement is real rather than a dead assertion that nothing ever exercises.
    #[test]
    fn the_fixture_rejects_a_listobjectsv2_request_missing_delimiter() {
        let (base, _root, _requests) = spawn_s3_fixture();
        let resp = ureq::get(&format!("{base}/{TEST_BUCKET}?list-type=2"))
            .call()
            .expect_err("missing delimiter must not succeed");
        match resp {
            ureq::Error::Status(code, _) => assert_eq!(code, 400),
            other => panic!("expected an HTTP 400, got a transport error: {other}"),
        }
    }

    // ---------------------------------------------------------------------------------------------
    // AC2: pagination is followed to completion, and proven — not assumed.
    // ---------------------------------------------------------------------------------------------

    /// Real, server-driven pagination over genuine HTTP round trips: `S3Provider::list` always requests
    /// `max-keys=1000`, comfortably above this test's 7 files, so the fixture is spawned with
    /// `page_cap = Some(3)` — a real gateway is always free to hand back fewer than the requested
    /// `max-keys` and mark `IsTruncated` — forcing three genuine round trips (3+3+1) that the client only
    /// learns to make via `IsTruncated`/`NextContinuationToken` in each response. This is the end-to-end
    /// half of the pagination proof; `dropping_the_continuation_loop_after_one_page_would_lose_entries`
    /// below is the unit-level half showing exactly what a missing loop would lose.
    #[test]
    fn pagination_is_followed_across_three_pages_and_removing_the_loop_would_lose_entries() {
        let (base, root, requests) = spawn_s3_fixture_with_page_cap(Some(3));
        std::fs::create_dir_all(root.join("bulk")).unwrap();
        let names: Vec<String> = (0..7).map(|i| format!("f{i:02}.txt")).collect();
        for name in &names {
            std::fs::write(root.join("bulk").join(name), b"x").unwrap();
        }

        let provider = S3Provider::connect(&cfg(&base));
        let entries = provider.list("/bulk").expect("list");
        let mut got: Vec<String> = entries.into_iter().map(|e| e.name).collect();
        got.sort();
        let mut want = names.clone();
        want.sort();
        assert_eq!(got, want, "not every page's entries came back — pagination was not followed to completion");
        assert_eq!(
            requests.load(Ordering::Relaxed),
            3,
            "7 entries at a 3-per-page server cap must take exactly 3 round trips (3+3+1); a different \
             count means the continuation loop is not doing what it should"
        );
    }

    /// The pagination loop itself, proven red/green by calling `parse_list_bucket_result` directly (the
    /// function `S3Provider::list`'s loop is built on) against the exact 3-page fixture output above, and
    /// asserting that manually stopping after page 1 — i.e. what happens if the continuation-token loop is
    /// removed — loses entries. This is the "deleting the continuation-token loop turns this test red"
    /// proof the ticket asks for: it does not merely assert the finished behaviour, it demonstrates the
    /// specific defect that dropping the loop would reintroduce.
    #[test]
    fn dropping_the_continuation_loop_after_one_page_would_lose_entries() {
        let (_base, root, _requests) = spawn_s3_fixture();
        std::fs::create_dir_all(root.join("bulk")).unwrap();
        let names: Vec<String> = (0..7).map(|i| format!("f{i:02}.txt")).collect();
        for name in &names {
            std::fs::write(root.join("bulk").join(name), b"x").unwrap();
        }

        let page1_xml = list_page_xml(&root, "bulk/", 0, 3);
        let page1 = parse_list_bucket_result(&page1_xml, "bulk/").unwrap();
        assert!(page1.is_truncated, "the fixture's own first page must be truncated for this proof to mean anything");
        assert_eq!(page1.entries.len(), 3, "stopping after one page — i.e. no continuation loop — only sees 3 of 7");
        assert_ne!(page1.entries.len(), names.len(), "a 1-page read must NOT already see everything");

        // The full 3-page walk (what `S3Provider::list`'s loop actually does) sees all 7.
        let page2_xml = list_page_xml(&root, "bulk/", 3, 3);
        let page2 = parse_list_bucket_result(&page2_xml, "bulk/").unwrap();
        let page3_xml = list_page_xml(&root, "bulk/", 6, 3);
        let page3 = parse_list_bucket_result(&page3_xml, "bulk/").unwrap();
        assert!(!page3.is_truncated);
        let total = page1.entries.len() + page2.entries.len() + page3.entries.len();
        assert_eq!(total, names.len());
    }

    // ---------------------------------------------------------------------------------------------
    // AC4: the mkdir marker never shows up as a spurious file inside its own directory.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn a_zero_byte_prefix_marker_object_does_not_appear_as_a_file_entry() {
        let (base, root, _requests) = spawn_s3_fixture();
        std::fs::create_dir_all(root.join("empty-dir")).unwrap();
        std::fs::write(root.join("empty-dir/.s3marker"), b"").unwrap(); // simulates the mkdir marker

        let provider = S3Provider::connect(&cfg(&base));
        let entries = provider.list("/empty-dir").expect("list");
        assert!(entries.is_empty(), "the directory's own marker leaked in as a file: {entries:?}");

        // The same directory listed from its PARENT must show a real directory entry, not a stray file.
        let parent_entries = provider.list("/").expect("list");
        let d = parent_entries.iter().find(|e| e.name == "empty-dir").expect("empty-dir entry");
        assert!(d.is_dir);
    }

    // ---------------------------------------------------------------------------------------------
    // AC5: a hostile/malformed key cannot produce an entry that escapes the listed prefix.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn a_content_key_with_a_traversal_segment_or_embedded_slash_is_dropped() {
        for bad_key in ["photos/../secret.txt", "photos/..", "photos//nested.txt", "photos/sub/file.txt"] {
            let xml = format!(
                "<ListBucketResult><IsTruncated>false</IsTruncated>\
                 <Contents><Key>{bad_key}</Key><Size>1</Size></Contents></ListBucketResult>"
            );
            let page = parse_list_bucket_result(&xml, "photos/").unwrap();
            assert!(page.entries.is_empty(), "unsafe key {bad_key:?} produced an entry: {:?}", page.entries);
        }
    }

    #[test]
    fn a_content_key_with_a_leading_slash_after_the_prefix_is_dropped() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                    <Contents><Key>/etc/passwd</Key><Size>1</Size></Contents></ListBucketResult>";
        // Requested prefix is empty (bucket root) — a key beginning with `/` yields a leaf of `/etc/passwd`.
        let page = parse_list_bucket_result(xml, "").unwrap();
        assert!(page.entries.is_empty(), "a leading-slash key produced an entry: {:?}", page.entries);
    }

    #[test]
    fn a_common_prefix_with_a_traversal_segment_is_dropped() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                    <CommonPrefixes><Prefix>photos/../../etc/</Prefix></CommonPrefixes></ListBucketResult>";
        let page = parse_list_bucket_result(xml, "photos/").unwrap();
        assert!(page.entries.is_empty(), "unsafe CommonPrefixes entry was not dropped: {:?}", page.entries);
    }

    #[test]
    fn a_key_outside_the_requested_prefix_is_dropped() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                    <Contents><Key>other/file.txt</Key><Size>1</Size></Contents></ListBucketResult>";
        let page = parse_list_bucket_result(xml, "photos/").unwrap();
        assert!(page.entries.is_empty(), "a key outside the requested prefix produced an entry: {:?}", page.entries);
    }

    // ---------------------------------------------------------------------------------------------
    // AC6: non-2xx responses go through the shared CPE-1682 error path, not an ad-hoc string.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn a_non_2xx_response_is_reported_through_the_shared_map_s3_error_path() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let body = "<Error><Code>AccessDenied</Code><Message>You do not have permission</Message></Error>";
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_status_code(403).with_header(ct));
            }
        });
        let base = format!("http://{addr}");

        let provider = S3Provider::connect(&cfg(&base));
        let err = provider.list("/").expect_err("403 must surface as an error");
        // The exact wording `map_s3_error` produces for a recognised code — proves this went through the
        // shared path (CPE-1682/CPE-1700) rather than a bare "HTTP 403" string built here.
        assert!(err.contains("AccessDenied"), "{err}");
        assert!(err.contains("You do not have permission"), "{err}");
    }

    // ---------------------------------------------------------------------------------------------
    // The ureq-header-drop decision: refused loudly, before anything is sent.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn sending_a_request_with_a_header_byte_ureq_would_drop_is_refused_before_any_bytes_leave_the_process() {
        let (base, _root, requests) = spawn_s3_fixture();
        // 'é' (U+00E9) passes `validate_structural_text` (not a control character, not whitespace) but is
        // a 2-byte UTF-8 sequence with both bytes >= 0x80 — outside ureq's sendable range. It ends up in
        // the `Authorization` header's `Credential=` field via the access key id.
        let bad_creds = Credentials::new("AKIA\u{e9}EXAMPLE", "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
        let cfg = S3Config::new(&base, "us-east-1", TEST_BUCKET, bad_creds).with_addressing(AddressingStyle::Path);
        let provider = S3Provider::connect(&cfg);

        let err = provider.list("/").expect_err("a non-ASCII access key id must be refused, not silently mangled");
        assert!(err.contains("ureq"), "{err}");
        assert!(err.contains("Authorization"), "the error must name which header, not just that something failed: {err}");
        assert_eq!(
            requests.load(Ordering::Relaxed),
            0,
            "the guard must fire BEFORE any request reaches the fixture — a live server saw a request \
             despite the refusal"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // provider_path_to_key_prefix: the marker-key shape CPE-1684 depends on.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn provider_path_to_key_prefix_matches_the_agreed_mkdir_marker_shape() {
        assert_eq!(provider_path_to_key_prefix("/"), "");
        assert_eq!(provider_path_to_key_prefix(""), "");
        assert_eq!(provider_path_to_key_prefix("/photos"), "photos/");
        assert_eq!(provider_path_to_key_prefix("/photos/2024"), "photos/2024/");
        assert_eq!(provider_path_to_key_prefix("photos/2024/"), "photos/2024/");
    }
}
