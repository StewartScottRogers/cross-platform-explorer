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
//! # Why these timeout values (CPE-1706 item 1) — what is protected, and what is tolerated
//! Until CPE-1706 this crate (and its sibling `cpe-webdav`) built `AgentBuilder::new().redirects(0)` and
//! nothing else. **`ureq` 2.x defaults `timeout_read`, `timeout_write` and the overall `timeout` all to
//! `None`** — the doc comments on `AgentBuilder::timeout_read`/`timeout_write` say it outright: *"requests
//! may block forever on reads by default"*. Every other bound in the listing path was real and verified
//! (an 8 MiB body cap, [`MAX_LIST_PAGES`], [`MAX_LIST_ENTRIES`], a nesting-depth guard): bytes and memory
//! were bounded, **time was not**. That matters here specifically because `list` runs on a
//! `spawn_blocking` thread, so a handful of slow peers can occupy the blocking pool with nothing to
//! reclaim them.
//!
//! Three knobs, and they are not interchangeable:
//!
//! - **[`TIMEOUT_READ`] / [`TIMEOUT_WRITE`] (30 s each)** are *per read/write*, not per request. The clock
//!   restarts on every byte, so this bounds a **stall** and never a slow-but-progressing transfer. That is
//!   exactly the property a large listing over a poor link needs: it may take as long as it takes,
//!   provided it keeps moving. 30 s is a wide margin over the time-to-first-byte of any real gateway
//!   (AWS's own SDKs default the same knob to 30–60 s), which is the number that actually has to be
//!   survivable — a server that has sent nothing for half a minute is not "slow", it is gone.
//! - **`ureq`'s overall `.timeout()` is deliberately NOT set.** It caps a whole request regardless of
//!   progress, so on the read path it would kill a legitimate multi-minute download of a large object over
//!   a bad connection — a real user, not a hypothetical. It also *replaces* the per-read bound rather than
//!   adding to it (`ureq` `agent.rs:476-477`: "takes precedence over `.timeout_read()`"), so setting it
//!   would trade a good bound for a worse one. And it would not even solve the problem it looks like it
//!   solves, because it is per **request** while the risk here is per **listing** — see the next point.
//! - **[`MAX_LIST_WALL_CLOCK`] (10 min)** is therefore the one that bounds the compound case: 1000 pages
//!   each stalling 30 s is ~8 hours of held thread, and no per-request knob can see that. A deadline over
//!   the whole `list` call can. What it tolerates, knowingly: a hostile endpoint can still hold one
//!   blocking thread for ten minutes. Bounded and survivable beats unbounded and unfalsifiable.
//!
//! `cpe-webdav` gets [`TIMEOUT_READ`]/[`TIMEOUT_WRITE`]'s equivalents for the same reasons but **no**
//! listing deadline — its `list` is a single `PROPFIND`, with no pagination loop to multiply anything, so
//! per-request bounds already bound it. See that crate's `connect` for the note.
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
//! to "fails a beat earlier, with a clearer, byte-and-offset-naming message, before touching the network at
//! all" — `ureq`'s own `Bad Header: invalid header '<full Authorization value>'` message is genuinely
//! usable but echoes the whole header including the signature, and does not say *which byte* made it
//! invalid. [`guard_header_sendable`]'s own message improves on both: it names the offending byte and its
//! offset into the value, and — an independent review of an earlier draft caught this — it must **not**
//! echo the value itself, since for `Authorization` that value carries the request signature and this
//! error reaches the caller (and any log) as an ordinary `Result<_, String>`. Refusing at this layer costs
//! nothing (no new dependency, one small pure function) and is strictly friendlier to debug than waiting
//! for `ureq` to say so. This is *not* the same decision the CPE-1684 warning asked for ("refuse loudly …
//! because the alternative is silent" no longer applies here); it is "refuse loudly because it is cheap and
//! clearer", which is a weaker but still positive case. **CPE-1684 should re-run this same measurement
//! against whatever code path its own PUT/HEAD/DELETE requests use** before repeating the silent-drop
//! framing verbatim — it may hold there, or it may not; this finding is scoped to `GET` via the builder
//! API, not to `ureq` in general.

use std::io::Read as _;
use std::time::{Duration, Instant};

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

/// Upper bound on how long the leaf name of a single `<Key>`/`<Prefix>` may be before the entry is dropped
/// (CPE-1706 item 3). **Real S3 caps an object key at 1024 bytes** — the protocol's own answer — so this
/// costs nothing for any key a conforming server can produce, while stopping a hostile endpoint from
/// spending its whole [`MAX_RESPONSE_BODY_BYTES`] budget on one ~8 MiB "filename" that then flows straight
/// into the UI as an entry name. Measured against the *leaf* (the part after the requested prefix), not the
/// whole key: the leaf is what becomes a displayed name, and it is always ≤ the key, so a 1024-byte leaf
/// bound admits every key a real bucket can hold. Dropped exactly like any other unsafe name — silently
/// skipped, never rendered — because a listing that quietly omits one absurd entry is better than one that
/// fails wholesale over a single bad row.
const MAX_KEY_LEAF_BYTES: usize = 1024;

/// How long a single socket read may make **no progress at all** before the request is abandoned
/// (CPE-1706 item 1). See this module's top doc, "Why these timeout values", for the full reasoning:
/// briefly, this is a *stall* detector, not a transfer budget — the clock restarts on every byte that
/// arrives, so it never penalises a slow-but-progressing link, and 30 s is a wide margin over the
/// time-to-first-byte of any real S3-compatible gateway (AWS's own SDKs use 30–60 s for the same knob).
const TIMEOUT_READ: Duration = Duration::from_secs(30);

/// The write-side twin of [`TIMEOUT_READ`]: how long one socket write may block with the peer's receive
/// window shut before the request is abandoned. Same value for the same reason; a `GET` request's bytes
/// are tiny, so this only ever fires against a peer that has stopped reading entirely.
const TIMEOUT_WRITE: Duration = Duration::from_secs(30);

/// Pinned explicitly rather than inherited: `ureq` 2.12.1 already defaults `timeout_connect` to 30 s
/// (`agent.rs:256`), so connect was the *one* phase that was never unbounded. Setting it to the same value
/// here changes nothing today and stops a future `ureq` default change from silently unbounding it.
const TIMEOUT_CONNECT: Duration = Duration::from_secs(30);

/// Wall-clock budget for **one whole `list` call**, across every page it follows (CPE-1706 item 1).
///
/// This is the bound [`TIMEOUT_READ`] cannot provide, and the reason it exists is arithmetic:
/// `timeout_read` is *per read*, so a server that emits one byte every 29 s never trips it, and
/// [`MAX_LIST_PAGES`] then multiplies that — 1000 pages × a 30 s stall each is ~8 hours of a held
/// `spawn_blocking` thread, which is not meaningfully better than unbounded. `ureq`'s own overall
/// `.timeout()` does not fix this either: it is **per request**, so the page loop multiplies it just the
/// same, and it *takes precedence over* `timeout_read`/`timeout_write` (`ureq` `agent.rs:476-477`) rather
/// than adding to them — choosing it means giving up the per-read stall bound, not gaining a second bound.
/// A deadline over the whole listing is the only knob whose units match the risk.
///
/// **10 minutes, chosen against the legitimate worst case, not the median.** A listing cannot legitimately
/// exceed [`MAX_LIST_ENTRIES`] (200 000) entries, which at `max-keys=1000` is 200 pages, not 1000. A user
/// on a genuinely poor link taking a punishing 2 s per page still finishes those 200 pages in ~400 s, well
/// inside this. The hostile case it does cut off is the compound one above: ~8 hours becomes 10 minutes.
/// What this deliberately tolerates: a hostile server may still hold one blocking thread for 10 minutes.
/// That is the price of not breaking the real user on the bad link, and it is bounded, which is the point.
const MAX_LIST_WALL_CLOCK: Duration = Duration::from_secs(600);

/// Build the `ureq::Agent` every request in this crate goes through — the single place the transport's
/// bounds are set, so `connect` and any test-injected variant cannot drift apart (only the two `Duration`s
/// differ between them). See [`TIMEOUT_READ`] and [`MAX_LIST_WALL_CLOCK`] for why these knobs and not
/// `ureq`'s overall `.timeout()`.
///
/// `redirects(0)` is the pre-existing CPE-1461 policy, kept: a SigV4 signature is computed for one exact
/// host and path, so following a server-supplied `3xx` would replay it against a target it was never
/// signed for.
fn build_agent(timeout_read: Duration, timeout_write: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(TIMEOUT_CONNECT)
        .timeout_read(timeout_read)
        .timeout_write(timeout_write)
        .build()
}

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
/// and-offset-naming message, for free".
///
/// **Never echoes `value` itself** (an independent review of an earlier draft caught this): this function
/// is called with `&signed.authorization`, which for `Authorization` carries the request's SigV4 signature
/// — `AWS4-HMAC-SHA256 Credential=<access key id>/…, Signature=<hex>` — and `S3Provider::list` returns
/// `Result<_, String>`, so whatever this function returns reaches the caller and any log verbatim. The
/// secret key itself is never in the value (SigV4 only ever puts the derived signature and the *public*
/// access key id in `Authorization`), but there is no reason to echo either one when the byte and its
/// offset already say everything needed to fix the input.
fn guard_header_sendable(label: &str, value: &str) -> Result<(), String> {
    if let Some((offset, b)) = value.bytes().enumerate().find(|(_, b)| !is_ureq_sendable_byte(*b)) {
        return Err(format!(
            "s3: the {label} header value contains byte {b:#04x} at offset {offset}, which ureq (this \
             crate's HTTP client) refuses to send — refusing here first, before the request is attempted, \
             with the specific byte and offset named rather than waiting for ureq's own (less specific) \
             refusal, and without echoing the header value itself."
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
/// - A leaf longer than [`MAX_KEY_LEAF_BYTES`] is dropped (CPE-1706 item 3): real S3 caps a key at 1024
///   bytes, so anything past that came from a server inventing names, and these names reach the UI.
/// - The remaining leaf name (the part after `key_prefix`, with a `CommonPrefixes` leaf's trailing `/`
///   also stripped) must pass [`cpe_server::transfer::is_safe_name`] — the same guard SFTP/WebDAV apply to
///   an attacker-controlled remote name, so a key carrying `../`, an embedded `/`, or a leading `/` cannot
///   produce an entry that escapes the listed prefix once rendered locally.
fn parse_list_bucket_result(xml: &str, key_prefix: &str) -> Result<ListPage, String> {
    if xml_nesting_too_deep(xml, MAX_XML_NESTING_DEPTH) {
        return Err("s3: ListObjectsV2 response XML nesting too deep".to_string());
    }
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("s3: bad ListObjectsV2 XML: {e}"))?;

    // CPE-1706 item 2: every field below is read from its *own* level of the document — the page-level
    // ones from `<ListBucketResult>`'s direct children, a `<Key>`/`<Size>`/`<Prefix>` from its own
    // container's. The previous whole-document `descendants()` search made an `<IsTruncated>` or
    // `<NextContinuationToken>` buried inside a `<Contents>` element eligible to be taken for the page's
    // own whenever the real top-level one was absent — the server controls every byte of both, so it could
    // choose where they appeared. Not exploitable given the caps, but it contradicted this module's own
    // stated principle (never assume a network-controlled response honours its own protocol), and
    // `children()` is both tighter and cheaper. `children()` yields text nodes too; their `tag_name()` is
    // empty, so the name comparisons below simply never match them.
    let root = doc.root_element();

    let is_truncated = root
        .children()
        .find(|n| n.tag_name().name() == "IsTruncated")
        .and_then(|n| n.text())
        .map(|t| t.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let next_token = root
        .children()
        .find(|n| n.tag_name().name() == "NextContinuationToken")
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let mut entries = Vec::new();

    for content in root.children().filter(|n| n.tag_name().name() == "Contents") {
        let Some(key) = content.children().find(|n| n.tag_name().name() == "Key").and_then(|n| n.text())
        else {
            continue;
        };
        let Some(leaf) = leaf_under_prefix(key, key_prefix) else { continue };
        if leaf.is_empty() {
            continue; // the directory's own zero-byte marker object, not a file inside it (AC4)
        }
        if leaf.len() > MAX_KEY_LEAF_BYTES {
            continue; // CPE-1706 item 3: longer than S3's own 1024-byte key limit — dropped as unsafe
        }
        if !cpe_server::transfer::is_safe_name(leaf) {
            continue; // AC5: a hostile/malformed key must not escape the listed prefix
        }
        let size = content
            .children()
            .find(|n| n.tag_name().name() == "Size")
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<u64>().ok())
            .unwrap_or(0);
        entries.push(ProviderEntry { name: leaf.to_string(), is_dir: false, size });
    }

    for cp in root.children().filter(|n| n.tag_name().name() == "CommonPrefixes") {
        let Some(prefix_text) = cp.children().find(|n| n.tag_name().name() == "Prefix").and_then(|n| n.text())
        else {
            continue;
        };
        let Some(leaf) = leaf_under_prefix(prefix_text, key_prefix) else { continue };
        let leaf = leaf.trim_end_matches('/');
        if leaf.is_empty() {
            continue;
        }
        if leaf.len() > MAX_KEY_LEAF_BYTES {
            continue; // CPE-1706 item 3, the directory-entry mirror of the Contents check above
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
    /// Wall-clock budget for one whole `list` call — [`MAX_LIST_WALL_CLOCK`] in production. A field
    /// rather than a bare constant read so the guard can be exercised in a second rather than ten
    /// minutes; `connect` is the only thing production calls, and it always installs the constant.
    list_deadline: Duration,
}

impl S3Provider {
    /// Build a provider for `config`. Does not perform a request; the first `list` (and, once CPE-1684
    /// lands, `stat`/`read`/…) issues one and surfaces addressing/auth/connection errors then.
    ///
    /// This is the constructor production uses, and it is the *only* place the shipped timeout values are
    /// chosen — see this module's top doc, "Why these timeout values".
    pub fn connect(config: &S3Config) -> Self {
        Self::connect_with_timeouts(config, TIMEOUT_READ, TIMEOUT_WRITE)
    }

    /// [`S3Provider::connect`] with the transport's stall bounds supplied by the caller instead of taken
    /// from [`TIMEOUT_READ`]/[`TIMEOUT_WRITE`].
    ///
    /// Public because a caller on a pathologically slow link has a legitimate reason to widen them, but
    /// its first use is this crate's own tests: a stalling-server test that had to wait out the shipped
    /// 30 s would cost 30 s of CI wall clock on three OSes, so the test injects a short bound and drives
    /// the *same* [`build_agent`] path production drives — only the `Duration`s differ. The shipped values
    /// themselves are pinned separately by
    /// `tests::the_shipped_timeout_values_are_finite_and_within_sane_bounds`.
    pub fn connect_with_timeouts(
        config: &S3Config,
        timeout_read: Duration,
        timeout_write: Duration,
    ) -> Self {
        S3Provider {
            config: config.clone(),
            agent: build_agent(timeout_read, timeout_write),
            list_deadline: MAX_LIST_WALL_CLOCK,
        }
    }

    /// Override the per-`list` wall-clock budget ([`MAX_LIST_WALL_CLOCK`] by default). Same rationale as
    /// [`S3Provider::connect_with_timeouts`]: production never calls this, and the guard it exposes would
    /// otherwise take ten minutes to observe firing.
    pub fn with_list_deadline(mut self, deadline: Duration) -> Self {
        self.list_deadline = deadline;
        self
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
        let started = Instant::now();
        loop {
            pages += 1;
            if pages > MAX_LIST_PAGES {
                return Err(format!(
                    "s3: list {path:?} exceeded {MAX_LIST_PAGES} ListObjectsV2 pages without finishing \
                     (the server kept answering IsTruncated=true) — refusing to keep following a possibly \
                     hostile or misbehaving server forever"
                ));
            }
            // The wall-clock bound the per-read socket timeout cannot give (CPE-1706 item 1): a server
            // that dribbles one byte per 29 s never trips `timeout_read`, and the page cap then multiplies
            // that stall by up to 1000. Checked between pages rather than mid-request, so an in-flight
            // page always completes or fails on its own socket timeout first — this bounds how many more
            // pages will be started, which is the compounding this is here to stop. See
            // `MAX_LIST_WALL_CLOCK`.
            let elapsed = started.elapsed();
            if elapsed > self.list_deadline {
                return Err(format!(
                    "s3: list {path:?} gave up after {elapsed:.1?} (budget {:.1?}) with the server still \
                     answering IsTruncated=true on page {pages} — a listing that has not finished inside \
                     its wall-clock budget is abandoned rather than allowed to hold this thread \
                     indefinitely",
                    self.list_deadline
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
    use std::panic::{self, AssertUnwindSafe};
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
    // CPE-1706 item 5: no test in this crate may be able to hang the CI job.
    // ---------------------------------------------------------------------------------------------

    /// Run `f` on a spawned thread and fail the test if it has not returned within `deadline`.
    ///
    /// **libtest has no per-test timeout.** Every other guard in this module reds in seconds when broken
    /// — a wrong count, a missing error, a fast crash — but the ones whose *whole purpose* is to stop an
    /// unbounded wait (the [`MAX_LIST_PAGES`] loop against a zero-growth endlessly-truncating server, and
    /// the [`TIMEOUT_READ`]/[`MAX_LIST_WALL_CLOCK`] bounds) regress into a **hang**, not a red: with the
    /// bound gone there is nothing left to end the call, so `cargo test` would sit there until the CI job's
    /// own six-hour limit killed it, reporting a timeout instead of a defect. Routing those calls through
    /// this helper converts that into a deterministic red naming what happened.
    ///
    /// The spawned thread is deliberately not joined on the failure path: it is, by construction, stuck in
    /// the very call that failed to return, and the panic here fails the test process anyway.
    fn call_with_deadline<T: Send + 'static>(
        what: &str,
        deadline: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        match rx.recv_timeout(deadline) {
            Ok(value) => value,
            Err(_) => panic!(
                "{what} did not return within {deadline:?}. The bound that was supposed to stop it is \
                 gone, so this call would have run forever — libtest has no per-test timeout, so without \
                 this deadline the CI job would have hung until its own limit rather than reporting a \
                 failure."
            ),
        }
    }

    /// A server that completes the TCP accept and then **never sends a byte**, holding every connection
    /// open forever — the shape that used to hold a `spawn_blocking` thread indefinitely, because `ureq`
    /// 2.x defaults `timeout_read` to `None` (CPE-1706 item 1). Holding the accepted streams in a `Vec` is
    /// what makes it a stall rather than a reset: dropping them would close the socket and the client would
    /// get a prompt EOF, proving nothing.
    fn spawn_a_server_that_accepts_and_never_answers() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                match stream {
                    Ok(s) => held.push(s),
                    Err(_) => break,
                }
            }
        });
        format!("http://{addr}")
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
    // B1 (independent-review finding): `xml_nesting_too_deep`/`MAX_XML_NESTING_DEPTH` shipped with zero
    // test coverage — the very guard that justified adding `xmlparser` as a dependency, ported from
    // `crates/webdav/src/lib.rs` (CPE-1398) without porting any of its five proofs. Ported back
    // near-verbatim, adapted from PROPFIND `<multistatus>` shape to `ListObjectsV2`'s `<ListBucketResult>`.
    // ---------------------------------------------------------------------------------------------

    /// Wrap `inner` in a well-formed `<ListBucketResult>` envelope, mirroring `crates/webdav`'s
    /// `multistatus_wrap`, so a case that isn't testing the envelope itself doesn't also trip on it.
    fn list_bucket_result_wrap(inner: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?><ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">{inner}</ListBucketResult>"#
        )
    }

    #[test]
    fn xml_nesting_guard_rejects_deep_nesting_before_it_reaches_roxmltree() {
        // Confirms the CPE-1398-derived stack-overflow guard actually fires here too: nesting past
        // MAX_XML_NESTING_DEPTH is rejected by the cheap pre-scan (no parse attempted), while a shallow,
        // realistic ListBucketResult body is not.
        let deep = format!(
            "<ListBucketResult>{}{}</ListBucketResult>",
            "<a>".repeat(4000),
            "</a>".repeat(4000)
        );
        assert!(xml_nesting_too_deep(&deep, MAX_XML_NESTING_DEPTH));
        assert!(parse_list_bucket_result(&deep, "").is_err());

        let shallow = list_bucket_result_wrap(
            "<IsTruncated>false</IsTruncated><Contents><Key>a.txt</Key><Size>1</Size></Contents>",
        );
        assert!(!xml_nesting_too_deep(&shallow, MAX_XML_NESTING_DEPTH));
        assert!(parse_list_bucket_result(&shallow, "").is_ok());
    }

    #[test]
    fn xml_nesting_guard_survives_the_quote_unaware_bypass() {
        // The CPE-1398 bypass: `<a b="/>">` is legal XML whose attribute value contains the literal bytes
        // `/>` — a quote-UNaware byte scan lands on that embedded `>`, sees the preceding `/`, and wrongly
        // concludes the tag is self-closing, so it never counts toward depth even though it is a real
        // child-bearing open element. `xml_nesting_too_deep` uses the real `xmlparser::Tokenizer`
        // (quote/comment/CDATA/PI-aware by construction), so this must be caught — under `catch_unwind`
        // too, as a defense-in-depth check that it is a graceful `Err`, not merely "the guard function
        // returns true in isolation".
        let n = 2000;
        let bypass =
            format!("<ListBucketResult>{}{}</ListBucketResult>", "<a b=\"/>\">".repeat(n), "</a>".repeat(n));
        assert!(
            xml_nesting_too_deep(&bypass, MAX_XML_NESTING_DEPTH),
            "the quote-unaware-scan bypass shape must be recognized as too deep"
        );
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_list_bucket_result(&bypass, "")));
        assert!(result.is_ok(), "parse_list_bucket_result must not panic/crash on the bypass payload");
        assert!(result.unwrap().is_err(), "parse_list_bucket_result must return Err for the bypass payload");
    }

    #[test]
    fn xml_nesting_guard_is_not_confused_by_gt_inside_comments_cdata_or_pis() {
        // Decoys containing a literal '>' inside constructs that don't nest (comments, CDATA, processing
        // instructions) must neither cause a false positive on shallow real input nor mask genuinely deep
        // real nesting.
        let shallow = "<?xml version=\"1.0\"?><!-- a > b --><ListBucketResult>\
             <![CDATA[ > ]]><?pi content > more?>\
             <IsTruncated>false</IsTruncated><!-- > -->\
             <Contents><![CDATA[>]]><Key>a.txt</Key><Size>5</Size></Contents>\
             </ListBucketResult>"
            .to_string();
        assert!(!xml_nesting_too_deep(&shallow, MAX_XML_NESTING_DEPTH), "decoys must not inflate depth");
        let page = parse_list_bucket_result(&shallow, "").expect("well-formed despite the decoys");
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].name, "a.txt");
        assert_eq!(page.entries[0].size, 5);

        // Genuinely deep real nesting (2000 levels) with the same kinds of decoys interleaved between real
        // tags: the decoys must NOT mask the real depth, and the call must still be a graceful Err under
        // catch_unwind, never a crash.
        let n = 2000;
        let open = "<a><!-- > --><![CDATA[>]]>".repeat(n);
        let close = "</a>".repeat(n);
        let deep_with_decoys = format!("<?xml version=\"1.0\"?><ListBucketResult>{open}{close}</ListBucketResult>");
        assert!(xml_nesting_too_deep(&deep_with_decoys, MAX_XML_NESTING_DEPTH), "decoys must not mask real depth");
        let result = panic::catch_unwind(AssertUnwindSafe(|| parse_list_bucket_result(&deep_with_decoys, "")));
        assert!(result.is_ok(), "must not panic/crash even with decoys present");
        assert!(result.unwrap().is_err());
    }

    // ---------------------------------------------------------------------------------------------
    // B2 (independent-review finding): MAX_LIST_PAGES and MAX_LIST_ENTRIES shipped as facts asserted in
    // the PR body with no test evidence. Both already worked; this is transcription, not new behaviour.
    // ---------------------------------------------------------------------------------------------

    /// Always answers `IsTruncated=true` with the same `NextContinuationToken` and `entries_per_page`
    /// freshly-formatted `<Contents>` rows, regardless of the request — simulates a server (hostile or
    /// merely broken) that never finishes a listing, so `S3Provider::list`'s two independent caps
    /// (`MAX_LIST_PAGES` for a server that never advances, `MAX_LIST_ENTRIES` for one that keeps growing)
    /// can each be forced and observed directly, without needing 200,000+ real files on disk.
    fn spawn_endlessly_truncated_server(entries_per_page: usize) -> String {
        spawn_endlessly_truncated_server_with_delay(entries_per_page, Duration::ZERO)
    }

    /// [`spawn_endlessly_truncated_server`] plus a fixed think-time before each response — a server that is
    /// answering correctly and *making progress*, just slowly, which is the only shape that can outrun a
    /// wall-clock budget without ever tripping a per-read socket timeout (CPE-1706 item 1). Used by
    /// [`a_listing_that_outruns_its_wall_clock_budget_is_abandoned`].
    fn spawn_endlessly_truncated_server_with_delay(entries_per_page: usize, delay: Duration) -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            let mut rows = String::new();
            for i in 0..entries_per_page {
                rows.push_str(&format!("<Contents><Key>f{i}.txt</Key><Size>1</Size></Contents>"));
            }
            let xml = format!(
                "<ListBucketResult><IsTruncated>true</IsTruncated>\
                 <NextContinuationToken>next</NextContinuationToken>{rows}</ListBucketResult>"
            );
            for req in server.incoming_requests() {
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml.clone()).with_header(ct));
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn a_server_that_never_stops_truncating_is_capped_by_max_list_pages() {
        // Zero new entries per page: MAX_LIST_ENTRIES can never trip, isolating MAX_LIST_PAGES.
        //
        // CPE-1706 item 5: this is the one test in the crate whose regression mode is an unbounded hang
        // rather than a red — against a zero-growth server MAX_LIST_ENTRIES can never fire, so with the
        // page cap gone the loop makes loopback requests forever and libtest, having no per-test timeout,
        // would let the CI job run to its own limit. `call_with_deadline` turns that into a red. 60 s is
        // ~235× the green path's measured cost (1001 sequential loopback round trips, ~255 ms), so it can
        // only fire on a genuine runaway, not on a loaded CI machine.
        let base = spawn_endlessly_truncated_server(0);
        let err = call_with_deadline(
            "S3Provider::list against a server that answers IsTruncated=true forever",
            Duration::from_secs(60),
            move || S3Provider::connect(&cfg(&base)).list("/"),
        )
        .expect_err("a server that answers IsTruncated=true forever must not be followed forever");
        assert!(
            err.contains(&format!("{MAX_LIST_PAGES} ListObjectsV2 pages")),
            "the error must name the page cap that actually fired: {err}"
        );
    }

    #[test]
    fn a_server_that_never_stops_growing_is_capped_by_max_list_entries() {
        // 1000 entries per page (S3's own per-page max) means the entries cap is reached in
        // MAX_LIST_ENTRIES / 1000 + 1 pages — comfortably under MAX_LIST_PAGES, so this test proves the
        // entries cap fires first for a genuinely growing listing, not merely that SOME cap eventually does.
        let base = spawn_endlessly_truncated_server(1000);
        let provider = S3Provider::connect(&cfg(&base));
        let err = provider
            .list("/")
            .expect_err("a server that keeps growing the listing forever must not be buffered forever");
        assert!(
            err.contains(&format!("{MAX_LIST_ENTRIES} entries")),
            "the error must name the entries cap, not the page cap (entries should be hit first): {err}"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // B3 (independent-review finding, "the most serious of the four"): `IsTruncated=true` with no
    // `NextContinuationToken` shipped untested. Replacing the error with a silent `break` leaves every
    // other test green while turning a hostile/broken server's malformed response into a silently
    // truncated listing presented as complete — exactly what CPE-1683's own ticket calls worse than
    // failing outright.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn is_truncated_true_with_no_continuation_token_is_refused_not_silently_truncated() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                requests_thread.fetch_add(1, Ordering::Relaxed);
                let xml = "<ListBucketResult><IsTruncated>true</IsTruncated>\
                           <Contents><Key>f0.txt</Key><Size>1</Size></Contents></ListBucketResult>";
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
            }
        });
        let base = format!("http://{addr}");

        let provider = S3Provider::connect(&cfg(&base));
        let err = provider.list("/").expect_err(
            "IsTruncated=true with no NextContinuationToken must be a loud error, not a silently \
             truncated-but-reported-complete listing",
        );
        assert!(err.contains("IsTruncated=true"), "{err}");
        assert!(err.contains("NextContinuationToken"), "{err}");
        assert_eq!(
            requests.load(Ordering::Relaxed),
            1,
            "must fail on the very first malformed page, not retry or silently accept what it has"
        );
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
        assert!(err.contains("byte") && err.contains("offset"), "the error must name the byte and its offset: {err}");
        // B4 (independent-review finding on an earlier draft): the value must never be echoed, because
        // this exact call site passes `&signed.authorization`, which for a real request carries the
        // request's SigV4 signature and access key id.
        assert!(!err.contains("Signature="), "the Authorization value leaked into the error: {err}");
        assert!(!err.contains("Credential="), "the Authorization value leaked into the error: {err}");
        assert!(!err.contains("AKIA"), "the access key id leaked into the error: {err}");
        assert_eq!(
            requests.load(Ordering::Relaxed),
            0,
            "the guard must fire BEFORE any request reaches the fixture — a live server saw a request \
             despite the refusal"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1706 item 1: wall-clock is bounded. Three separate bounds, three separate proofs.
    // ---------------------------------------------------------------------------------------------

    /// The per-read socket bound, driven through the production request path: `connect_with_timeouts` and
    /// `connect` differ only in where the two `Duration`s come from — both build the agent via
    /// [`build_agent`], and the call then goes through the same `list` → `signed_get` → `req.call()` that
    /// production uses. A short bound is injected because waiting out the shipped 30 s would cost 30 s of
    /// wall clock in every CI job on three OSes; the shipped values are pinned separately by
    /// [`the_shipped_timeout_values_are_finite_and_within_sane_bounds`].
    ///
    /// The error text itself is not asserted beyond the URL prefix: a socket read timeout surfaces through
    /// `std::io` differently per platform (`WouldBlock` — "Resource temporarily unavailable" — on Unix,
    /// `TimedOut` on Windows), and this repo runs a 3-OS CI matrix. What is asserted is the part that is
    /// the actual behaviour under test and is identical everywhere: it **returned, with an error, quickly**
    /// instead of blocking forever.
    #[test]
    fn a_server_that_accepts_the_connection_and_then_never_answers_is_cut_off_by_the_read_timeout() {
        let base = spawn_a_server_that_accepts_and_never_answers();
        let short = Duration::from_millis(300);
        let started = Instant::now();
        let err = call_with_deadline(
            "S3Provider::list against a server that accepts the connection and never answers",
            Duration::from_secs(30),
            move || S3Provider::connect_with_timeouts(&cfg(&base), short, short).list("/"),
        )
        .expect_err("a connection that is accepted and then stalled must surface as an error, not hang");
        let elapsed = started.elapsed();
        assert!(
            err.starts_with("s3: http://127.0.0.1"),
            "the error must name the endpoint that stalled, so a user knows which one to blame: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "the 300 ms read timeout — not some other accident — must be what ended the call, but it took \
             {elapsed:?}"
        );
    }

    /// [`a_server_that_accepts_the_connection_and_then_never_answers_is_cut_off_by_the_read_timeout`]
    /// proves the *mechanism* through the production builder with an injected `Duration`. This pins the
    /// *values* `connect` actually installs, which that test deliberately does not wait out. Together they
    /// cover the claim "`S3Provider::connect` produces an agent whose reads and writes are bounded by a
    /// finite, sane timeout": remove `.timeout_read(..)` from `build_agent` and the first test reds;
    /// change `TIMEOUT_READ` to something useless and this one does.
    #[test]
    fn the_shipped_timeout_values_are_finite_and_within_sane_bounds() {
        for (name, value) in
            [("TIMEOUT_READ", TIMEOUT_READ), ("TIMEOUT_WRITE", TIMEOUT_WRITE), ("TIMEOUT_CONNECT", TIMEOUT_CONNECT)]
        {
            assert!(
                value >= Duration::from_secs(5),
                "{name} = {value:?} is short enough to cut off a legitimately slow gateway's \
                 time-to-first-byte — this knob bounds a stall, not a transfer"
            );
            assert!(
                value <= Duration::from_secs(120),
                "{name} = {value:?} is long enough that a dead peer still holds a spawn_blocking thread \
                 for minutes, which is what CPE-1706 exists to stop"
            );
        }

        // The listing budget must clear the legitimate worst case by a real margin, and this is the
        // arithmetic MAX_LIST_WALL_CLOCK's doc comment claims: a listing cannot legitimately exceed
        // MAX_LIST_ENTRIES, which at max-keys=1000 is 200 pages, and 2 s per page is already a punishing
        // link. If someone tightens the budget below that, this fails and names the number it broke.
        let legitimate_worst_case = Duration::from_secs(2) * (MAX_LIST_ENTRIES / 1000) as u32;
        assert!(
            MAX_LIST_WALL_CLOCK >= legitimate_worst_case,
            "MAX_LIST_WALL_CLOCK = {MAX_LIST_WALL_CLOCK:?} would abandon a legitimate maximum-size listing \
             ({} pages at a poor-link 2 s each = {legitimate_worst_case:?})",
            MAX_LIST_ENTRIES / 1000
        );
        assert!(
            MAX_LIST_WALL_CLOCK <= Duration::from_secs(3600),
            "MAX_LIST_WALL_CLOCK = {MAX_LIST_WALL_CLOCK:?} is not a bound anyone would notice"
        );
    }

    /// The bound no per-request timeout can provide. The server here is neither hostile nor stalled — it
    /// answers every request correctly and promptly enough that no socket read ever times out — it simply
    /// never says it is finished. That is precisely the case `timeout_read` cannot see and the page cap
    /// bounds only in *page count*, not in time: 1000 pages × a 30 s stall each is hours.
    ///
    /// Deterministic in the safe direction: a slower machine only makes `elapsed` cross the budget sooner
    /// in page terms, never later, because the fixture's 60 ms think-time is a floor. The test asserts the
    /// wall-clock message fired and, explicitly, that the **page cap did not** — otherwise a passing
    /// assertion here would prove nothing new.
    #[test]
    fn a_listing_that_outruns_its_wall_clock_budget_is_abandoned() {
        let base = spawn_endlessly_truncated_server_with_delay(0, Duration::from_millis(60));
        let err = call_with_deadline(
            "S3Provider::list against a correct-but-endless server, under a 100 ms listing budget",
            Duration::from_secs(120),
            move || {
                S3Provider::connect(&cfg(&base))
                    .with_list_deadline(Duration::from_millis(100))
                    .list("/")
            },
        )
        .expect_err("a listing that outruns its wall-clock budget must be abandoned, not followed forever");
        assert!(err.contains("gave up after"), "the error must say the budget is what ended it: {err}");
        assert!(err.contains("budget"), "the error must name the budget it exceeded: {err}");
        assert!(
            !err.contains("ListObjectsV2 pages"),
            "the page cap fired, not the wall-clock budget — this test proves nothing about the budget: {err}"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1706 item 2: every field is read from its own level, not found anywhere in the document.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn an_is_truncated_nested_inside_contents_is_not_mistaken_for_the_pages_own() {
        // No page-level <IsTruncated> at all — the only ones in the document are buried inside a
        // <Contents>, where a whole-document `descendants()` search would happily find them. Both are
        // server-controlled, so this is the server choosing where its own page-level answer appears.
        let xml = "<ListBucketResult>\
                     <Contents><Key>a.txt</Key><Size>1</Size>\
                       <IsTruncated>true</IsTruncated>\
                       <NextContinuationToken>server-chosen</NextContinuationToken>\
                     </Contents>\
                   </ListBucketResult>";
        let page = parse_list_bucket_result(xml, "").unwrap();
        assert_eq!(page.entries.len(), 1, "the entry itself must still parse: {:?}", page.entries);
        assert!(
            !page.is_truncated,
            "an <IsTruncated> nested inside <Contents> was taken for the page's own — the page level said \
             nothing, so the answer must be `false`"
        );
        assert_eq!(
            page.next_token, None,
            "a <NextContinuationToken> nested inside <Contents> was taken for the page's own"
        );

        // Positive control: the page's own fields, at their own level, are still read.
        let real = "<ListBucketResult><IsTruncated>true</IsTruncated>\
                    <NextContinuationToken>page-level</NextContinuationToken></ListBucketResult>";
        let page = parse_list_bucket_result(real, "").unwrap();
        assert!(page.is_truncated);
        assert_eq!(page.next_token.as_deref(), Some("page-level"));
    }

    #[test]
    fn a_key_nested_below_contents_own_level_is_not_taken_for_the_entrys_key() {
        // `<Meta>` comes first in document order, so a `descendants()` search rooted at `<Contents>` finds
        // `decoy.txt` before the entry's real `<Key>`. Only the direct child is the entry's own key.
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                     <Contents><Meta><Key>decoy.txt</Key><Size>999</Size></Meta>\
                       <Key>real.txt</Key><Size>7</Size></Contents>\
                   </ListBucketResult>";
        let page = parse_list_bucket_result(xml, "").unwrap();
        assert_eq!(page.entries.len(), 1, "{:?}", page.entries);
        assert_eq!(page.entries[0].name, "real.txt", "a nested <Key> was taken for the entry's own");
        assert_eq!(page.entries[0].size, 7, "a nested <Size> was taken for the entry's own");
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1706 item 3: a key longer than S3's own key limit is dropped like any other unsafe name.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn a_key_leaf_longer_than_s3s_own_key_limit_is_dropped_like_any_other_unsafe_name() {
        let over = "a".repeat(MAX_KEY_LEAF_BYTES + 1);
        let at_cap = "b".repeat(MAX_KEY_LEAF_BYTES);
        let xml = format!(
            "<ListBucketResult><IsTruncated>false</IsTruncated>\
             <Contents><Key>photos/{over}</Key><Size>1</Size></Contents>\
             <Contents><Key>photos/{at_cap}</Key><Size>1</Size></Contents>\
             <CommonPrefixes><Prefix>photos/{over}/</Prefix></CommonPrefixes>\
             </ListBucketResult>"
        );
        let page = parse_list_bucket_result(&xml, "photos/").unwrap();
        let names: Vec<&str> = page.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![at_cap.as_str()],
            "exactly the at-the-cap key must survive: the over-cap <Key> and the over-cap \
             <CommonPrefixes> leaf are both dropped, and a key at real S3's own 1024-byte limit is not"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1706 item 6: MAX_RESPONSE_BODY_BYTES, the last of the five runtime defences with no test.
    // ---------------------------------------------------------------------------------------------

    /// An over-cap body must surface as an honest parse error — **never a partial listing sold as
    /// complete**, which is the failure this whole module is written against. The keys are large but each
    /// stays under [`MAX_KEY_LEAF_BYTES`], and the entry count stays far under [`MAX_LIST_ENTRIES`], so
    /// with the body cap removed the document parses cleanly and the call returns `Ok` — making the break
    /// unambiguous (a red `expect_err`, not a different cap's error text).
    #[test]
    fn a_response_body_over_the_cap_is_refused_as_a_parse_error_not_sold_as_a_complete_listing() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            let mut xml = String::from("<ListBucketResult><IsTruncated>false</IsTruncated>");
            let mut i = 0usize;
            while xml.len() < MAX_RESPONSE_BODY_BYTES + 1024 * 1024 {
                let name = format!("{i:08}-{}", "k".repeat(900));
                xml.push_str(&format!("<Contents><Key>{name}</Key><Size>1</Size></Contents>"));
                i += 1;
            }
            xml.push_str("</ListBucketResult>");
            assert!(i < MAX_LIST_ENTRIES, "the fixture must stay under the entries cap to isolate the body cap");
            for req in server.incoming_requests() {
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml.clone()).with_header(ct));
            }
        });
        let base = format!("http://{addr}");

        let err = call_with_deadline(
            "S3Provider::list against a server returning a body over MAX_RESPONSE_BODY_BYTES",
            Duration::from_secs(60),
            move || S3Provider::connect(&cfg(&base)).list("/"),
        )
        .expect_err("a body cut off at the cap is an incomplete document and must not parse as a listing");
        assert!(
            err.contains("bad ListObjectsV2 XML"),
            "the truncation must surface through the parser's own honest error: {err}"
        );
        assert!(
            err.contains("the root node was opened but never closed"),
            "the error must say the document was cut off, not something vaguer: {err}"
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
