//! `S3Provider`: the [`FileSystemProvider`] impl for an S3-compatible bucket (CPE-1683, epic CPE-1503).
//!
//! This is the first module in `cpe-s3` that actually talks to a server — everything in [`crate::sigv4`]
//! and [`crate::error`] is a pure function of its inputs, checkable against fixed vectors with no network
//! at all. `list` here is `ListObjectsV2` with `delimiter=/`, paginated to completion, presenting
//! `<CommonPrefixes>` as virtual directories (`ProviderCapabilities::has_real_dirs = false`) and
//! `<Contents>` as files. CPE-1684 then added the remaining ops — `stat` (HEAD + a prefix probe), `read`
//! (GET, chunked), `write` (PUT), `mkdir` (the zero-byte marker object), `delete` (exactly one key) — and
//! made `rename` an honest, permanent refusal.
//!
//! # The three decisions CPE-1684 made, so they are not silently re-litigated
//!
//! 1. **`rename` refuses, and always will.** S3 has no rename; copy-then-delete is not atomic, is O(size),
//!    and rewrites storage class and metadata. `capabilities().supports_rename` is `false` so a caller sees
//!    it coming. See [`FileSystemProvider::rename`] for the full reasoning.
//! 2. **`delete` removes exactly one key, and refuses a non-empty virtual directory.** S3 answers `204` for
//!    a key that never existed, so a single-key `DELETE` of a directory prefix would report success while
//!    the entire subtree stayed put. One prefix probe decides the case before anything is deleted; a
//!    recursive per-key loop is out of scope precisely because a half-failed one reports success. See
//!    [`FileSystemProvider::delete`].
//! 3. **A bodiless response is mapped by status *and method*, not by parsing.** HEAD responses carry no
//!    body, so the body-driven [`error::map_s3_error`] would answer "could not be read … refusing to guess"
//!    for every missing key. [`map_response_error`] applies the status+method rule when there is no body
//!    and defers to CPE-1682's parser when there is.
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
//! - **[`TIMEOUT_LIST_REQUEST`] (60 s)** bounds one `ListObjectsV2` request **end to end, body read
//!   included** — via `ureq::Request::timeout`, which is per *request* and so can be applied to this call
//!   site without touching the large-object `GET` that wants per-read semantics. This is the knob that
//!   closes the dribble: valid headers followed by one byte every 29 s defeats a per-read timeout
//!   completely, and a between-pages check cannot fire while that body is in flight.
//! - **[`MAX_LIST_WALL_CLOCK`] (10 min)** bounds the compound case the per-request deadline cannot see:
//!   how many more pages get started. 1000 pages × 60 s each would be 16.7 hours; this stops it.
//!
//! **The honest worst case for one `list` is `MAX_LIST_WALL_CLOCK + TIMEOUT_LIST_REQUEST` = 660 s ≈ 11
//! minutes**,
//! because a final page may begin just under the budget and then consume its own full deadline. An
//! earlier version of this module claimed 10 minutes *while permitting an unbounded single page* — the
//! claim was wrong in the direction that matters, and the pairing above is what makes the number real.
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
//! # The path grammar, and the two places a key can still be lost
//! **A provider path is `/` + the S3 key + an optional `/` addressing it as a directory** (CPE-1722).
//! One leading slash is stripped and at most one trailing slash; every other byte is key. That grammar
//! is decided in exactly one place — [`rooted_key_bytes`], shared by [`provider_path_to_object_key`] and
//! [`provider_path_to_key_prefix`] — and their agreement is asserted as a property, not a table.
//!
//! The one key shape this crate still cannot reach is a `.`/`..` path segment, because `ureq` 2 rewrites
//! it between signing and sending. **CPE-1721's decision, and the `ureq` 2 vs 3 measurement it rests
//! on, are recorded on [`guard_path_survives_the_client`]** — alongside the original CPE-1684
//! measurement they follow from, rather than split across two doc blocks.
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
/// both a successful `ListObjectsV2` page and a non-2xx error body. **Exceeding it is a loud error, never
/// a truncated body handed to the parser** — see [`S3Provider::signed_get`] for the counter-example
/// (CPE-1706 round 2) that proved inferring truncation from a parse failure is unsound. A real page of up to 1000 keys is a
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
/// bound admits every key a real bucket can hold. Enforced as one more arm of [`is_safe_s3_leaf`], so an
/// over-long key is dropped by exactly the path any other unsafe name takes — which, post-CPE-1704, means
/// it is *counted* into `filtered_count` and surfaced rather than silently vanishing, and means the bound
/// also applies through `S3Provider`'s `is_safe_leaf_name` override in `crates/vfs`.
const MAX_KEY_LEAF_BYTES: usize = 1024;

/// Bytes copied per iteration of [`S3Provider::read`]'s body loop (CPE-1684).
///
/// The ticket names the convention and where it came from: `cpe-ftp` settled on a **fixed 64 KiB chunk**
/// rather than one `read_to_end`, and this matches it byte for byte (`crates/ftp/src/lib.rs`'s
/// `READ_CHUNK_BYTES`). The property that matters is that no single `Read::read` call can be made to demand
/// an outsized allocation by whatever the server claims or streams — the buffer is a fixed stack array, so
/// what the peer sends changes how many iterations happen, never how much is asked for at once. It is also
/// what makes [`MAX_OBJECT_READ_BYTES`] enforceable *during* the transfer instead of after it.
///
/// **What this does NOT do, stated plainly** (see [`MAX_OBJECT_READ_BYTES`]): it does not stop the whole
/// object being in memory at the end. [`FileSystemProvider::read`] returns `Vec<u8>`, so by the time it
/// returns, it must. Chunking bounds the *incremental* demand and makes the cap fire mid-transfer; it is
/// not, and cannot be, a streaming read.
const READ_CHUNK_BYTES: usize = 64 * 1024;

/// Per-object byte cap for [`S3Provider::read`] — **2 GiB**, the same value and the same reasoning as the
/// sibling `cpe-webdav`'s `MAX_READ_BYTES`, so the two remote HTTP backends refuse at the same point rather
/// than each picking a number.
///
/// This is a **memory backstop, not a file-size policy**. `FileSystemProvider::read` hands back a whole
/// `Vec<u8>`, so at 2 GiB the whole-file-in-a-`Vec` contract is itself the problem; refusing there costs
/// nothing that works today. Deliberately separate from [`MAX_RESPONSE_BODY_BYTES`] (8 MiB), which bounds a
/// *listing* page — an 8 MiB ceiling on object reads would make the provider useless for ordinary files.
///
/// **An over-cap read is a loud `Err`, never a truncated `Vec` returned as if it were the file** — the same
/// distinction `cpe-webdav` documents, for the same consumer: `cpe_server::transfer`'s download sink writes
/// whatever comes back to disk as the finished file, so a silent truncation here is data loss wearing a
/// success. The cap is checked inside the [`READ_CHUNK_BYTES`] loop, so it fires while the transfer is still
/// running rather than after the process has already buffered everything.
const MAX_OBJECT_READ_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The largest body [`S3Provider::write`] will attempt in one `PUT` — **5 GiB**, S3's documented ceiling for
/// a single-part upload (CPE-1684; multipart upload is explicitly out of this ticket's scope).
///
/// Read as **5 GiB and not 5 GB on purpose.** AWS's docs say "5 GB" without saying which; refusing at
/// 5×10⁹ when the server would have accepted 5×2³⁰ would reject a legal upload *on our side*, which is the
/// one failure direction this refusal must not have. Erring high can only ever produce a server-side
/// rejection, which is the server's own answer rather than our guess. This is a courtesy refusal that names
/// the real ceiling a beat before the round trip; it is not a correctness guard.
const MAX_SINGLE_PUT_BYTES: u64 = 5 * 1024 * 1024 * 1024;

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

/// End-to-end deadline for **one `ListObjectsV2` request**, body read included (CPE-1706 round 2).
///
/// # This is the bound that closes the dribble hole, and why the other two could not
/// [`TIMEOUT_READ`] is per-*read*: its clock restarts on every byte, so a server that sends valid
/// `200 OK` headers and then emits **one byte every 29 s** never trips it. [`MAX_LIST_WALL_CLOCK`] is
/// checked between pages, so it cannot fire while a body is in flight. Between them, nothing bounded a
/// single page's body at all — an independent UAT measured a one-byte-per-5 s server holding a listing
/// thread indefinitely, and at 8 MiB × 29 s/byte the theoretical worst case is on the order of *years*.
/// A per-request deadline is the only one of the three whose clock does not restart and whose scope
/// covers a body already being read.
///
/// `ureq::Request::timeout` (`request.rs:60`) is the right mechanism because it is **per request**, not
/// per agent: `DeadlineStream::fill_buf` recomputes the remaining budget on *every* read
/// (`stream.rs:85-89`) and the deadline propagates into `Response::into_reader`, so it bounds the whole
/// exchange rather than each read of it. That is a different knob from the agent-level `.timeout()` this
/// module still declines — the agent-level one would apply to *every* request including a large-object
/// `GET`, which is exactly where per-read semantics are correct. The earlier mistake was flattening "which
/// request deserves which bound" into one all-or-nothing choice; the answer is per call site.
///
/// # This value has to do two jobs, because `ureq` makes it an either/or
/// Setting a per-request deadline **replaces** [`TIMEOUT_READ`] for that request (`stream.rs:433-436`
/// takes the deadline branch *instead of* `config.timeout_read`). So this number is not just "an outer
/// bound on top of 30 s" — for a `ListObjectsV2` request it is now the *only* bound, and it must
/// simultaneously:
///
/// 1. **exceed the slowest legitimate page**, or a real user on a bad link loses their listing, and
/// 2. **stay short enough that a dead share still fails promptly**, because it now governs the
///    accept-then-silence case too, which [`TIMEOUT_READ`] used to catch in 30 s.
///
/// **60 s.** A page is at most 1000 keys; at typical key lengths that is ~90 KB, which even on a
/// punishing 256 kbit/s link is under 3 s — better than 20× margin. (The pathological ceiling, every key
/// at the full [`MAX_KEY_LEAF_BYTES`], is ~1.1 MB ≈ 35 s on that same link, so even the absurd case fits;
/// the 8 MiB [`MAX_RESPONSE_BODY_BYTES`] cap is headroom, not a target.) Against job 2, 60 s doubles the
/// old dead-server wait rather than quadrupling it: **120 s was the first value tried here and rejected**,
/// because it bought no extra safety for any real listing and made a dead endpoint take two minutes to
/// report. This is the trade the either/or forces, made deliberately rather than inherited.
const TIMEOUT_LIST_REQUEST: Duration = Duration::from_secs(60);

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
/// exceed [`MAX_LIST_ENTRIES`] (200 000) entries; a gateway that honours the requested `max-keys=1000`
/// delivers those in 200 pages, and even at a punishing 2 s per page that is ~400 s, inside this budget.
///
/// **A caveat that an earlier version of this comment got wrong:** S3 does not *guarantee* it will return
/// as many keys as `max-keys` asks for — a gateway is free to return fewer. One returning 200 keys per
/// page needs 1000 pages for the same 200 000 entries, which at 2 s each is ~33 minutes and **would** be
/// abandoned by this budget. That is a real, accepted limitation rather than an impossible case: it needs
/// a gateway that both under-fills pages by 5× and a link that slow, and when it happens the error names
/// the budget as the cause, so it is diagnosable rather than mysterious. The alternative — a budget wide
/// enough to cover it — would be ~35 minutes of held thread for the hostile case, which is a worse trade.
///
/// # What is actually bounded, stated correctly (CPE-1706 round 2 correction)
/// This constant alone does **not** bound a `list` call, because it is checked between pages and cannot
/// fire while a body is in flight. Paired with [`TIMEOUT_LIST_REQUEST`], which bounds each individual
/// request end to end, the true worst case for one `list` is `MAX_LIST_WALL_CLOCK + TIMEOUT_LIST_REQUEST`
/// — a final page may start just under the budget and then take its own full deadline — i.e. **about 12
/// minutes**, not 10. An earlier version of this comment claimed 10 minutes while the code permitted an
/// unbounded single page; the number below is the honest one.
///
/// What this deliberately tolerates: a hostile server may still hold one blocking thread for ~12 minutes.
/// That is the price of not breaking the real user on the bad link, and it is genuinely bounded now.
const MAX_LIST_WALL_CLOCK: Duration = Duration::from_secs(600);

/// Build the `ureq::Agent` every request in this crate goes through — the single place the transport's
/// bounds are set, so `connect` and any test-injected variant cannot drift apart (only the two `Duration`s
/// differ between them). See [`TIMEOUT_READ`] and [`MAX_LIST_WALL_CLOCK`] for why these knobs and not
/// `ureq`'s overall `.timeout()`.
///
/// `redirects(0)` is the pre-existing CPE-1461 policy, kept: a SigV4 signature is computed for one exact
/// host and path, so following a server-supplied `3xx` would replay it against a target it was never
/// signed for.
/// `timeout_connect` is a parameter rather than a direct read of [`TIMEOUT_CONNECT`] for a specific
/// testing reason (CPE-1706 round 2): `ureq`'s own default for that knob is *also* 30 s, so an assertion
/// that the built agent has `timeout_connect: Some(30s)` passes identically whether the line is wired or
/// deleted — the first version of this guard was verified to red on `timeout_write` and **not** on
/// `timeout_connect` for exactly that reason. Taking it as a parameter lets the test pass a value nothing
/// else would produce, which is the only way the wiring is observable while the value matches the default.
fn build_agent(timeout_read: Duration, timeout_write: Duration, timeout_connect: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(timeout_connect)
        .timeout_read(timeout_read)
        .timeout_write(timeout_write)
        .build()
}

/// Read a response body, never buffering more than [`MAX_RESPONSE_BODY_BYTES`], returning the bytes and
/// whether the body ran **past** the cap.
///
/// # Why this is a function rather than two `.take()` calls
/// `signed_get` has two body-read sites (2xx and non-2xx), and CPE-1706 round 2's review found that
/// deleting the cap from the error-path one turned **no** test red — it is a pure memory guard, so
/// removing it changes nothing observable in any output, which makes it effectively untestable in place.
/// Rather than bolt on a test that cannot really fail, both sites now share this one function, so there
/// is exactly one `.take()` in the module and the success path's existing over-cap tests guard it for
/// both. A guard that cannot be tested is better removed *or* merged into one that can; this is the
/// second option.
///
/// Reads one byte MORE than the cap so "ran past the cap" is distinguishable from "was exactly cap-sized".
fn read_body_capped(reader: impl std::io::Read) -> std::io::Result<(Vec<u8>, bool)> {
    let mut buf = Vec::new();
    reader.take(MAX_RESPONSE_BODY_BYTES as u64 + 1).read_to_end(&mut buf)?;
    let over_cap = buf.len() > MAX_RESPONSE_BODY_BYTES;
    buf.truncate(MAX_RESPONSE_BODY_BYTES);
    Ok((buf, over_cap))
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

/// Whether `segment` is a path segment the WHATWG URL parser (and so the `url` crate, and so `ureq`)
/// treats as a dot segment and resolves away.
///
/// The spec's definitions are deliberately wider than a literal `.`/`..`: a **single-dot** segment is `.`
/// or an ASCII-case-insensitive `%2e`, and a **double-dot** segment is `..`, `.%2e`, `%2e.`, or `%2e%2e`.
/// Mirrored here rather than simplified, because a guard that only caught the literal forms would let the
/// encoded ones through and be wrong in the one direction that matters.
///
/// The `%2e` arms cannot fire from this crate's own encoder today ([`crate::sigv4::encode_path`] leaves `.`
/// alone as an unreserved character and escapes a literal `%` to `%25`), so they are pure future-proofing
/// against an encoder change — and they cannot produce a false refusal, because nothing this crate emits
/// can look like them by accident.
///
/// **That unreachability is measured, not assumed** (CPE-1684 round-2 review, recorded here because it was
/// briefly contested). Driven through `S3Provider`, the key `a/%2e%2e/b.txt` signs
/// `/test-bucket/a/%252e%252e/b.txt` and goes on the wire byte-identical, with this guard **not** firing —
/// the `%` is escaped before the path is ever built, so `%2e` cannot reach `ureq` as `%2e` through this
/// API at all. A separate probe that handed `ureq` a URL containing a literal `%2e` did see it rewritten,
/// which is true of `ureq` and irrelevant here: that URL is unreachable through `S3Provider`. Only a bare
/// `.`/`..` can reach the client, which is exactly what [`guard_path_survives_the_client`] refuses on the
/// evidence of its own measurement. Do not "correct" this paragraph to say the arms match a live defect on
/// this path — they do not, and that edit was proposed once and withdrawn.
fn is_url_dot_segment(segment: &str) -> bool {
    let unescaped = segment.to_ascii_lowercase().replace("%2e", ".");
    unescaped == "." || unescaped == ".."
}

/// Refuse, before any request is sent, a key whose path the HTTP client will rewrite between signing and
/// sending (CPE-1684).
///
/// # This is a measurement, not a precaution
///
/// The ticket flagged it as an unverified hypothesis ("**Test this first**: the HTTP client may rewrite the
/// path you signed"), explicitly labelled by the reviewer who raised it as something they could not check.
/// **Measured here against the real send path, it reproduces exactly.**
/// `tests::a_key_with_a_dot_segment_is_refused_because_ureq_resolves_it_away_before_sending` records the
/// observation: signing `/test-bucket/a/../b//c%252Fd.txt` and asking `ureq` 2.12.1 to send it puts
/// `/test-bucket/b//c%252Fd.txt` on the wire. `ureq` parses every URL through the `url` crate, which
/// implements WHATWG URL parsing, which resolves dot segments as part of parsing — before `ureq` has any
/// say in it.
///
/// **Only dot segments are affected**, and that is measured too, not assumed: in the same request, `//` and
/// `%25` both survived byte for byte, so the empty-segment and percent-encoding cases the ticket also asked
/// about are fine. `tests::a_key_with_a_double_slash_and_a_percent_encoded_slash_reaches_the_wire_intact`
/// pins that half.
///
/// # Why refuse rather than normalise, or rather than send it anyway
///
/// S3 keys are opaque byte strings: `a/../b.txt` is a **real, distinct object** from `b.txt`, and CPE-1689
/// established that this crate signs the canonical path unnormalised precisely so such a key is reachable.
/// Normalising to match what the client sends would silently address a different object — the exact
/// silently-wrong-object failure CPE-1689 exists to prevent. Sending it anyway produces
/// `SignatureDoesNotMatch`, with nothing in the message to say why, which is the opaque failure the whole
/// S3 slice has been working to eliminate.
///
/// So the honest answer is the third one: refuse, and name the cause. The crate's "one construction, so the
/// URL and the signature cannot disagree" guarantee ends at the crate boundary; this is where it ends, said
/// out loud. Actually reaching such a key needs an HTTP client that does not normalise URLs, which is
/// **CPE-1721**, filed from this measurement.
///
/// # CPE-1721's decision, and the measurement it rests on
///
/// CPE-1721 offered three options and asked for a decision rather than a fix. **Option 2 was measured
/// first, as that ticket instructed, and it works.**
///
/// **How to re-run this** — the table below is the whole evidentiary basis for the CPE-1800 migration
/// decision, so it must not be a number nobody can reproduce. Exact versions probed: **`ureq` 2.12.1**
/// (this crate's current pin, `Cargo.lock`) against **`ureq` 3.3.0** with `default-features = false`,
/// pulling **`ureq-proto` 0.6.0** and **`http` 1.5.0**, on Rust 1.97.0. The probe is a ~50-line throwaway
/// binary depending on both majors under renamed keys (`ureq2`/`ureq3`), asking each for the same
/// loopback URL and reading the request line off a bare `TcpListener`; it was NOT landed in the tree,
/// because adding a permanent dev-dependency on the major this crate has not adopted is precisely the
/// dependency-budget question CPE-1800 exists to decide. **Whoever takes CPE-1800 should land it there**,
/// as an `#[ignore]`d test alongside the migration, so the comparison becomes re-runnable at the moment
/// it starts mattering. Until then, reproducing it is: a two-dependency scratch crate and the loop below.
///
/// The same raw-socket technique as [`tests::spawn_a_request_line_recorder`], driven against both majors
/// from one probe, reports
///
/// ```text
/// ASKED /test-bucket/a/../b//c%252Fd.txt   ureq2 /test-bucket/b//c%252Fd.txt        REWROTE
///                                          ureq3 /test-bucket/a/../b//c%252Fd.txt   OK
/// ASKED /test-bucket/a/./b.txt             ureq2 /test-bucket/a/b.txt               REWROTE
///                                          ureq3 /test-bucket/a/./b.txt             OK
/// ASKED /test-bucket/a/%2e%2e/b.txt        ureq2 /test-bucket/b.txt                 REWROTE
///                                          ureq3 /test-bucket/a/%2e%2e/b.txt        OK
/// ```
///
/// `ureq` 3 routes the request target through `http::Uri` (via `ureq-proto`) instead of the `url` crate —
/// `url` is an *optional* dependency there — and `http::Uri` is a syntax-only parser that performs no
/// dot-segment removal. So the dot-segment gap is a property of `ureq` 2 specifically, not of HTTP.
///
/// **The same probe also closes the "just encode the dots" escape hatch**, which is the obvious cheap fix
/// and does not work: WHATWG URL parsing defines a double-dot segment as `..` *or* a case-insensitive
/// `.%2e` / `%2e.` / `%2e%2e`, so `%2e%2e` is resolved away too (measured above), and the only spelling
/// that survives — `%252e%252e` — percent-decodes at S3 to the key `%2e%2e`, a *different* object. There
/// is no encoding of `.` that `url` passes through and S3 decodes back to `.`.
///
/// **The swap is not made here.** It is a `ureq` major migration — a rewrite of this crate's whole request
/// layer (agent construction, the per-request vs per-read deadline split CPE-1706/1727 tuned, the gzip and
/// `Response` surfaces, the error mapping) — and it pulls a new dependency family (`ureq-proto`, `http`,
/// `rustls` 0.23) into a crate whose `Cargo.toml` records "adds NO new dependency family at all" as a
/// decision, while the sibling `cpe-webdav` still pins `ureq` 2. That is its own ticket with its own
/// dependency-budget question, not a rider on a path-grammar fix. What CPE-1721 asked for and what is
/// delivered here is the decision recorded against the measurement: **option 2 is viable and is the fix;
/// until it lands the refusal stands**, and — see [`S3Provider::list_with_filtered_count`] — `list` now
/// refuses such a prefix too, so the crate no longer offers a browsable folder whose every child it will
/// then decline to open.
fn guard_path_survives_the_client(encoded_path: &str) -> Result<(), String> {
    if let Some(segment) = encoded_path.split('/').find(|s| is_url_dot_segment(s)) {
        return Err(format!(
            "s3: this key contains the path segment {segment:?}, and this crate's HTTP client (ureq 2, \
             which parses URLs through the `url` crate) resolves dot segments away while parsing — \
             measured, not assumed: signing `/a/../b.txt` puts `/b.txt` on the wire. An S3 key is an \
             opaque byte string, so `a/../b.txt` is a real object distinct from `b.txt` (CPE-1689) and \
             this crate signs it unnormalised on purpose. The request would therefore be signed for one \
             key and sent for another, and the server would answer SignatureDoesNotMatch with nothing in \
             the message to say why. Refusing here and naming the cause instead. Reaching a key with a \
             dot segment needs an HTTP client that does not normalise URLs — see CPE-1721."
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
///
/// # One rooting step, two projections (CPE-1722)
///
/// This used to carry its own `path.trim_matches('/')`, identical to the one in the object-key helper.
/// Two copies of a path grammar is how the two drift apart, and CPE-1722's acceptance criterion is that
/// the grammar is decided **once, for the whole crate**, with `list`/`mkdir`/`stat`/`delete` all
/// agreeing. So both helpers now share [`rooted_key_bytes`] — the single place a provider path loses its
/// `/`-rooting — and differ only in what they do with a trailing slash: the object form strips at most
/// one, the prefix form ensures exactly one.
///
/// **They agree wherever both have an answer**: `prefix(p) == object_key(p) + "/"` for every path with an
/// object key, which [`tests::the_object_key_and_the_prefix_key_agree_on_every_path_that_has_both`]
/// asserts as a property rather than a table. The one path where they differ is `"//"`, which has **no**
/// object key (its key would be the zero-length one S3 does not have) but is a perfectly real prefix,
/// `"/"` — the virtual directory holding a key like `/lead.txt`. Deriving this function from the
/// object-key helper alone would make that directory unlistable while its contents stayed addressable,
/// which is the listing-vs-object asymmetry CPE-1721 is separately about.
pub fn provider_path_to_key_prefix(path: &str) -> String {
    let rooted = rooted_key_bytes(path);
    if rooted.is_empty() {
        // The bucket root: list the whole bucket. It has no marker object and is never filtered as one.
        String::new()
    } else if rooted.ends_with('/') {
        rooted.to_string()
    } else {
        format!("{rooted}/")
    }
}

/// Strip the `/`-rooting every provider path carries — **exactly one leading slash, never all of them** —
/// leaving the S3 key bytes untouched.
///
/// This is the single place that decision is made, shared by [`provider_path_to_object_key`] and
/// [`provider_path_to_key_prefix`] so the two cannot disagree about where a path's key starts. The second
/// slash of `//a.txt` is the first byte of the key `/a.txt`, not more rooting: an S3 key is an opaque byte
/// string, and `a.txt`, `/a.txt` and `//a.txt` are three different objects (CPE-1689, CPE-1722).
fn rooted_key_bytes(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

/// Convert a `/`-rooted provider path into the S3 key of **one object** (CPE-1684) — the `stat`/`read`/
/// `write`/`delete` counterpart to [`provider_path_to_key_prefix`], trimming the same slashes so the two
/// cannot disagree about where a path's key starts, and simply not appending the `/` that makes a prefix.
///
/// The bucket root has no object key, so `""`/`"/"` is an `Err` rather than a silent address of something
/// else — the same refusal, for the same reason, that [`S3Config::object_target`] already applies to an
/// empty key ("S3 has no zero-length key").
///
/// # The path grammar, decided once for the whole crate (CPE-1722)
///
/// **A provider path is `"/"` + the S3 key + an optional `"/"` marking that the key is being addressed
/// as a directory/prefix.** Exactly one leading slash is removed and at most one trailing slash; every
/// other byte, at either end or in the middle, is part of the key and survives untouched.
///
/// That makes every S3 key spellable, which is the whole point — a key is an opaque byte string, so
/// `a.txt`, `/a.txt` and `//a.txt` are three different objects and a bucket written by a tool that joined
/// paths carelessly genuinely holds all three:
///
/// | provider path  | object key | prefix (marker) key |
/// |----------------|------------|---------------------|
/// | `/a.txt`       | `a.txt`    | `a.txt/`            |
/// | `/a.txt/`      | `a.txt`    | `a.txt/`            |
/// | `//a.txt`      | `/a.txt`   | `/a.txt/`           |
/// | `/a//b.txt`    | `a//b.txt` | `a//b.txt/`         |
/// | `/a.txt//`     | `a.txt/`   | `a.txt//`           |
/// | `///`          | `/`        | `//`                |
/// | `/` or `//`    | *(none — the bucket root)* | *(empty)* |
///
/// # What this replaced, and why the old shape was a data-loss bug
///
/// Until CPE-1722 this was `path.trim_matches('/')`, which strips **every** leading and trailing slash.
/// [`S3Config::object_target`] one layer below has always been byte-exact (CPE-1689: `"/a.txt"` builds
/// `/bucket//a.txt`), so the guarantee was correct at the layer that documented it and thrown away at the
/// layer callers actually use. Measured to the wire by the CPE-1684 round-2 UAT: provider path `"//a.txt"`
/// reached the socket as `/test-bucket/a.txt`. `write("//report.pdf", …)` therefore **overwrote**
/// `report.pdf` — the silently-wrong-object failure CPE-1689 exists to prevent, arriving one layer up
/// from where that ticket looked.
///
/// # Why a *trailing* slash is still not part of the object key
///
/// This is the one deliberate asymmetry, and the reason is a specific, checkable one: **the shared
/// `cpe-vfs` layer puts that slash there.** `cpe_vfs::connect::join_remote` appends a trailing `/` to
/// every directory child URI it builds, for every remote backend, so a directory path arriving at any
/// provider routinely carries one. A trailing slash therefore means "address this as a directory" — and
/// on S3 that is exactly what a key ending in `/` *is*, the zero-byte marker convention this module's
/// own `mkdir` writes.
///
/// Do **not** restate this as "every other provider trims a trailing slash" or "the siblings build a
/// `format!("{key}/")` probe". Both are false, and two successive versions of this comment said them —
/// so here are the actual call sites, checked one at a time rather than described by shape:
///
/// - `cpe-ftp` (`crates/ftp/src/lib.rs:301`) and `cpe-webdav` (`crates/webdav/src/lib.rs:292`) trim a
///   trailing slash inside `stat`.
/// - `cpe-sftp` (`crates/sftp/src/lib.rs:334`) takes the last non-empty segment instead
///   (`path.rsplit('/').find(|s| !s.is_empty())`) and contains **no `trim_end_matches` at all**.
///
/// All three derive a **display name only**; none normalises the path before the wire. And no sibling
/// builds a `format!("{key}/")` probe: the one slash-appending construction in the family,
/// `crates/webdav/src/lib.rs:370`, is the RFC 4918 §8.3 DELETE redirect retry, not an existence check.
///
/// `crates/vfs/src/connect.rs`'s `join_remote` carries a comment written specifically to retract this
/// same overclaim — including the live WebDAV `delete` bug it was hiding — and records there that
/// `list`/`mkdir`/`delete`/`rename` send the path verbatim on every backend. The convention is real; the
/// uniform-trimming story about it is not. **If you edit this paragraph, re-grep the three files first**;
/// it has now been wrong twice, each time by being restated from memory of its shape.
///
/// Nothing is lost by it, because the rule is total: a key that genuinely ends in `/` is spelled with the
/// slash doubled (`/a.txt//` → key `a.txt/`), which is just the general rule `"/" + key + "/"` applied to
/// a key whose last byte happens to be a slash. So `stat`/`delete` keep the grammar every other backend
/// speaks, and no S3 key becomes unreachable.
///
/// # No `is_safe_s3_leaf` pass here, and this is a deliberate decision, not an omission
///
/// [`S3Provider::list_with_filtered_count`]'s doc instructs whoever wires these ops up not to accept "ANY
/// provider-supplied name as a literal key without going through the same [`is_safe_s3_leaf`] check `list`
/// already applies". Applied **literally to the whole path**, that instruction would break CPE-1689: it
/// would split `/a/../b.txt` into the segments `a`, `..`, `b.txt`, refuse the `..`, and so refuse a key
/// that CPE-1689 established is a real, distinct, legitimately addressable object — the crate signs
/// `/a/../b.txt` canonically and unnormalised precisely so it can be reached.
///
/// The two are reconciled by noticing what [`is_safe_s3_leaf`] is *for*: it decides whether a leaf may be
/// **surfaced as a navigable child name**, not whether a key is addressable. Those are different questions
/// with legitimately different answers. And the instruction's own premise — "provider-supplied" — cannot
/// arise here: a leaf that guard refuses never becomes a [`ProviderEntry`] at all, so no path built by
/// navigating a listing can contain one. A path containing `..` therefore only ever arrives because a
/// caller typed that exact key, which is the case CPE-1689 says must work.
///
/// What genuinely must not happen — a raw control byte or a `/` injected into the request line or a signed
/// header — is closed where it actually lives: [`crate::sigv4::encode_path`] percent-encodes every byte
/// outside the unreserved set into the path (so a `\n` in a key travels as `%0A`, never as a request-line
/// break), and [`guard_header_sendable`] covers the header side.
fn provider_path_to_object_key(path: &str) -> Result<String, String> {
    let rooted = rooted_key_bytes(path);
    // At most ONE trailing slash, which is the "address this as a directory" marker described above —
    // never all of them, so `/a.txt//` keeps the key `a.txt/`.
    let key = rooted.strip_suffix('/').unwrap_or(rooted);
    if key.is_empty() {
        return Err(format!(
            "s3: {path:?} addresses the bucket itself, which is not an object — S3 has no zero-length \
             key, so there is nothing here to stat, read, write or delete"
        ));
    }
    Ok(key.to_string())
}

/// CPE-1748: does `path` (the caller's ORIGINAL, un-stripped provider path) explicitly address a
/// directory, per the same trailing-`/` convention `cpe_vfs::connect::join_remote`/`with_dir_suffix`
/// build a directory child's URI with (CPE-1737)?
///
/// [`provider_path_to_object_key`] strips that marker to compute `key`, on purpose — object addressing
/// is deliberately blind to it, which is exactly what makes it lose the CPE-1737 distinction: `stat`/
/// `read`/`write`/`delete` end up with an **identical** `key` for `/photos` (the object) and `/photos/`
/// (the same-named prefix), because the one byte that told them apart is already gone by the time `key`
/// exists. Callers that need to keep telling them apart (`stat`/`read`/`delete`) have to read the
/// distinction off `path` itself, before it is discarded — this is that read.
///
/// **Single vs double trailing slash matters, and this is why it is `!key.ends_with('/')` and not just
/// `path.ends_with('/')`.** A path with exactly one trailing slash (`/photos/`) strips down to a `key`
/// with none (`photos`) — pure directory-marker, no information lost by treating it as directory intent.
/// A path with *two* trailing slashes (`/a.txt//`) strips down to a `key` that STILL ends in `/`
/// (`a.txt/`) — CPE-1722's real, doubled-slash key shape, an object whose key is genuinely `a.txt/`, not
/// a directory-marker at all. Collapsing that case into "directory" here would make that legitimate key
/// permanently unreachable through `stat`/`read`/`delete`, which is the opposite of this ticket's goal.
fn path_addresses_a_directory(path: &str, key: &str) -> bool {
    path.ends_with('/') && !key.ends_with('/')
}

/// Turn a non-2xx response into an error, choosing the rule by whether the response actually carried a
/// body — the CPE-1682 error path when there is something to parse, an explicit status+method rule when
/// there is not (CPE-1684).
///
/// # Why a bodiless rule has to exist at all
///
/// [`error::map_s3_error`] is body-driven and correct: with no `<Code>` to read it says *"HTTP 404 and the
/// response body could not be read as an S3 error … refusing to guess which cause applies"*. That is the
/// right answer for a **parser**. But **HTTP HEAD responses never carry a body** (RFC 9110 §9.3.2), and
/// every existence check in this module is HEAD-shaped, so routing them through the parser alone would make
/// "could not be read" the majority experience for the single most common failure in the whole provider —
/// precisely where a missing key and a denied key have to stay distinguishable.
///
/// So: when `body` is empty, the status **and** the method are the evidence, and they are enough. Nothing
/// is being guessed — an empty body from a `HEAD` is what the protocol *requires*, not a symptom of a
/// broken server, so there is no missing information to be honest about. When there IS a body, this defers
/// to [`error::map_s3_error`] unchanged; that line was drawn by CPE-1682 and is not redrawn here.
fn map_response_error(method: &str, status: u16, body: &[u8], what: &str) -> String {
    if !body.is_empty() {
        return format!("s3: {what}: {}", error::map_s3_error(status, body));
    }
    match status {
        404 => format!(
            "s3: {what}: not found (HTTP 404 to a {method} request, which carries no response body by \
             protocol — for a {method} on an object key, 404 is S3's answer for a key that does not exist)"
        ),
        // 403 is NOT simply "denied", and saying so flatly would be a confident wrong answer. Documented
        // AWS behaviour: when the caller lacks `s3:ListBucket` on the bucket, S3 answers **403 for a key
        // that does not exist** rather than 404, specifically so that a probe cannot enumerate keys. So a
        // bodiless 403 genuinely means "denied, or missing and you are not permitted to be told which" —
        // and the honest message says both, rather than picking one. It remains cleanly distinguishable
        // from the 404 arm above, which is what the AC asks for.
        401 | 403 => format!(
            "s3: {what}: access denied (HTTP {status} to a {method} request, which carries no response \
             body by protocol). Note S3 also answers {status} rather than 404 for a key that does NOT \
             exist when the credentials lack s3:ListBucket on the bucket, so this may mean the key is \
             missing and the server is declining to say so — check the credentials and the bucket policy \
             before concluding the object is there"
        ),
        _ => format!(
            "s3: {what}: HTTP {status} with an empty response body, so there is no S3 error code to read \
             and nothing beyond the status to go on — refusing to guess which cause applies"
        ),
    }
}

/// True if `status` is a 2xx HTTP status that is not the exact `200` a `ListObjectsV2` reply requires
/// (CPE-1740). `203` (RFC 9110 §15.3.4 makes it the standards-legitimate answer from a transforming
/// proxy — reachable behind a corporate MITM proxy or a CDN) and `206` (a partial/range reply) both
/// satisfy `(200..300).contains(&status)` while being neither complete nor authoritative: the shape this
/// whole ticket is about. `ListObjectsV2` has exactly one success status, so anything else in this range
/// is refused rather than rendered.
fn is_non_canonical_listing_status(status: u16) -> bool {
    status != 200 && (200..300).contains(&status)
}

/// The reason text for a `ListObjectsV2` reply (or the identical request the `start-after` belt sends)
/// whose status [`is_non_canonical_listing_status`] — deliberately **not** routed through
/// [`error::map_s3_error`] or [`map_response_error`], because both hunt the body for an S3 `<Error>`
/// document's `<Code>` element, and a 203/206 body is not an S3 error at all: it is a listing, or (on a
/// 206) a legitimate fragment of one, that answered successfully by HTTP's own rules while failing the
/// one rule this client enforces — exactly `200`.
///
/// Before this, the not-200 case on both call sites reused the S3-error path, which read a genuine
/// listing body looking for an error code and reported *"the response body could not be read as an S3
/// error … refusing to guess which cause applies"* about a body that read perfectly fine and was never an
/// error to begin with. Measured on the belt at PR #903's round-5 UAT (CPE-1740).
///
/// **The status-specific sentence names only the cause that actually applies** (PR #911 review + UAT,
/// independently): the first draft appended BOTH the 203 sentence and the 206 sentence unconditionally,
/// so a 203 reply's message described a range response that never happened, and a 206 reply's message
/// described a transforming proxy that never touched it. `status` decides which one sentence belongs.
fn non_canonical_listing_status_cause(status: u16) -> String {
    let explanation = match status {
        // RFC 9110 §15.3.4: the standards-legitimate answer from a transforming proxy — reachable behind
        // a corporate MITM proxy or a CDN. Nothing about a range request; nothing here sent one.
        203 => " RFC 9110 §15.3.4 makes 203 a standards-legitimate answer from a transforming proxy \
                 between here and the origin."
            .to_string(),
        // A partial/range reply. This client never sends a `Range` header on a ListObjectsV2 GET, so an
        // unsolicited 206 means something between here and the origin is rewriting the response, not that
        // a range was ever requested.
        206 => " 206 means the reply is a partial/range response — this client never sends a Range \
                 header on a listing request, so nothing here asked for one."
            .to_string(),
        // Any other non-canonical 2xx (201/202/204/205): none of them is a documented ListObjectsV2
        // outcome, so no more specific cause is claimed than the fact itself.
        _ => String::new(),
    };
    format!(
        "HTTP {status}: ListObjectsV2 has exactly one success status, 200, and this reply answered \
         {status} instead of it. That is not an S3 error to be parsed — the body may be (or contain a \
         fragment of) a perfectly well-formed listing — so it is refused rather than rendered as this \
         path's complete contents.{explanation}"
    )
}

/// True if `status` is a 2xx HTTP status that is not the exact `200` a `GetObject`/`HEAD` reply requires
/// (CPE-1749). Same shape as [`is_non_canonical_listing_status`], reused for a different pair of verbs:
/// `read`'s `GetObject` and `stat`'s `HEAD` each have exactly one success status too, and `206` in
/// particular is legitimate only in answer to a `Range` header — this crate's `signed_request` never sends
/// one on either verb (see [`S3Provider::read`] and [`S3Provider::stat`]), so an unsolicited `206` here
/// means the reply is, by definition, not the whole object.
fn is_non_canonical_object_status(status: u16) -> bool {
    status != 200 && (200..300).contains(&status)
}

/// The reason text for a `GetObject` reply whose status [`is_non_canonical_object_status`] (CPE-1749) —
/// deliberately **not** routed through [`map_response_error`]/[`error::map_s3_error`], for the same reason
/// [`non_canonical_listing_status_cause`] exists: a 203/206 body is not an S3 error document, it is (a
/// fragment of) the real object, and hunting it for an `<Error>`'s `<Code>` would misdiagnose it exactly
/// the way CPE-1740 measured for a listing.
///
/// # Why 200-only is right here, not merely consistent
///
/// [`FileSystemProvider::read`]'s whole contract is "the complete object, or a loud `Err`" — its own doc
/// says a silent truncation is data loss wearing a success. `206 Partial Content` is an HTTP-legal answer
/// **only** to a request carrying a `Range` header, and this client sends none on a `GetObject`, so a
/// `206` reply is not a partial answer to something asked for — it is proof the object handed back is
/// short. Measured reproduction: an unsolicited `206` with body `HALF` used to come back as
/// `Ok([72, 65, 76, 70])`, which `cpe_server::transfer`'s download sink would write to disk as the
/// finished file.
fn non_canonical_read_status_cause(status: u16) -> String {
    let explanation = match status {
        203 => " RFC 9110 §15.3.4 makes 203 a standards-legitimate answer from a transforming proxy \
                 between here and the origin — the body may have been rewritten in transit, so it is not \
                 provably the object's own bytes either."
            .to_string(),
        206 => " 206 means the reply is a partial/range response, and this client never sends a Range \
                 header on a GetObject request — nothing here asked for a fragment of the object, so an \
                 unsolicited 206 means the body handed back is short of the whole object."
            .to_string(),
        _ => String::new(),
    };
    format!(
        "HTTP {status}: GetObject has exactly one success status, 200, and this reply answered {status} \
         instead of it. A complete object body was required — reading it as the object's contents would \
         risk writing a truncated file to disk and reporting it as the finished download — so it is \
         refused rather than returned.{explanation}"
    )
}

/// The reason text for a `HEAD` reply whose status [`is_non_canonical_object_status`] (CPE-1749) —
/// [`S3Provider::stat`]'s sibling of [`non_canonical_read_status_cause`]. A HEAD carries no body by
/// protocol, so the harm here is not a truncated body but a wrong `Content-Length`: under a `206` that
/// header is the RANGE length, not the object's full size, and reporting it as the size would be exactly
/// the "confident wrong answer" [`S3Provider::stat`]'s own doc refuses to give for a missing
/// `Content-Length` on a 200.
fn non_canonical_stat_status_cause(status: u16) -> String {
    let explanation = match status {
        203 => " RFC 9110 §15.3.4 makes 203 a standards-legitimate answer from a transforming proxy \
                 between here and the origin."
            .to_string(),
        206 => " 206 means the reply is a partial/range response, and this client never sends a Range \
                 header on a HEAD request — nothing here asked for one, so an unsolicited 206's \
                 Content-Length is the RANGE length, not the object's size."
            .to_string(),
        _ => String::new(),
    };
    format!(
        "HTTP {status}: HEAD has exactly one success status, 200, and this reply answered {status} \
         instead of it. Content-Length under a non-canonical 2xx cannot be trusted as the object's full \
         size, so this is refused rather than reported as one.{explanation}"
    )
}

/// Wrap a failed [`S3Provider::probe_prefix`] so the error names **the probe** as the thing that failed,
/// instead of surfacing a bare access-denied about a prefix the user never typed (CPE-1723 item 1).
///
/// # The bug this fixes
///
/// `delete` and `stat` each run one `ListObjectsV2` against `key/` to tell a single object from a virtual
/// directory, and both used to propagate its failure with a plain `?`. `probe_prefix` formats its errors
/// with the *prefix* as the subject, so a credential holding `s3:DeleteObject` but not `s3:ListBucket`
/// asked to delete `/photos/a.jpg` got back an access-denied naming `"photos/a.jpg/"` — trailing slash and
/// all. A path the user never wrote, about an operation they never asked for, for an op they were fully
/// entitled to perform. Mostly masked on AWS proper (without `s3:ListBucket` a HEAD answers 403, so `stat`
/// never reaches its probe) but reachable on MinIO and Ceph, which is what a self-hoster points this at.
///
/// # Why naming the probe, and not falling back to the un-probed single-key path
///
/// CPE-1723 offers both shapes and does not choose. This is the deliberate choice, and the reason is that
/// the two ops are asymmetric in what a fallback would cost, while a message is symmetric and free:
///
/// - **`delete` must not fall back.** The probe is the *only* thing that distinguishes an object from a
///   directory. "The probe was denied" therefore means "I cannot tell which of the two this is" — and
///   deleting on that guess is precisely the failure [`FileSystemProvider::delete`] refuses: S3's `DELETE`
///   answers `204` for a prefix as readily as for an object, so a fallback would report `/photos/2024`
///   deleted while every object under it stayed exactly where it was. That refusal exists because S3 has no
///   atomic multi-key delete; letting a *denied* probe re-enable the very delete a *successful* probe
///   forbids would make the guard weaker the less the server is willing to tell us, which is backwards.
/// - **`stat` gains nothing from falling back.** Its probe only runs after a 404 on the object key, so the
///   fallback answer would be "not found" — a claim about a path that may well be a directory the caller
///   simply may not enumerate. That is the confident wrong answer, not the absence of one.
/// - **A message costs nothing and fixes the actual complaint.** The ticket's symptom is a *message*
///   defect: an error the user cannot act on because it is about a string they never typed. Naming the
///   probe, saying nothing was deleted, and pointing at `s3:ListBucket` turns it into an actionable one,
///   with no behaviour change to review and no way for it to make a delete less safe.
///
/// The wrapper fires on **every** probe failure, not only a 403 — a timeout or a 500 produced the same
/// mystery prefix, and "specifically denied" is not a distinction the reader of the message needs us to
/// make before it becomes readable.
fn probe_failure(op: &str, path: &str, key_prefix: &str, why: &str, cause: &str) -> String {
    format!(
        "s3: {op} {path:?}: {why} That check is one ListObjectsV2 request for the prefix {key_prefix:?}, \
         and it is that request which failed — so the prefix named in the underlying error below is this \
         check's own, not a path you asked for. If the credentials carry the per-object permission but not \
         s3:ListBucket on the bucket, that is the likeliest cause, and granting s3:ListBucket is the fix: \
         S3 has no real directories, so listing a prefix is the only way to see one, and answering without \
         it would mean guessing. Underlying error: {cause}"
    )
}

/// The refusal [`FileSystemProvider::delete`] gives for a virtual directory that has content under it.
///
/// One function rather than two literals because CPE-1727 added a second place that reaches this verdict —
/// the `start-after` belt — and a user must not be able to tell which check refused them by the wording.
fn directory_with_content_refusal(path: &str) -> String {
    format!(
        "s3: delete {path:?}: this is a virtual directory with content under it, and S3 has no recursive \
         or atomic multi-key delete. Removing it means deleting every key beneath the prefix one at a \
         time, and a loop that failed halfway would leave part of the tree gone while reporting success — \
         so it is refused rather than faked, for the same reason rename is. Delete the objects inside it \
         first."
    )
}

/// The failure message for [`FileSystemProvider::delete`]'s `start-after` belt — deliberately **not**
/// [`probe_failure`], because that function's permission diagnosis is *provably false* at this call site.
///
/// # Why this cannot reuse `probe_failure`
///
/// `probe_failure`'s text is unconditional: *"if the credentials carry the per-object permission but not
/// `s3:ListBucket`, that is the likeliest cause, and granting `s3:ListBucket` is the fix"*. That is fair
/// where it is used — a probe that failed as the *first* listing of the call, where a missing
/// `s3:ListBucket` really is the likeliest explanation.
///
/// The belt is reachable **only after `probe_prefix` returned `Ok`**: the same `ListObjectsV2`, the same
/// prefix, the same credential, moments earlier. So `s3:ListBucket` is demonstrably held, and the
/// "likeliest cause" is the one cause the call has already ruled out. Sending a user to change a bucket
/// policy that is provably not the problem is the same defect [`list_failure`] gates against by
/// conditioning its permission sentence on 401/403 — and worse here, because the condition is not merely
/// unknown but known false. Found by the PR #903 review.
///
/// What this says instead is what is actually actionable: the parameter that was added to the request.
/// A gateway that does not implement `start-after` is a real possibility (it is one of the newer
/// `ListObjectsV2` parameters), and that is a fact about the *server*, not about the credential.
///
/// # Why it reads `status`, and the round-2 defect that made it necessary (PR #903 UAT)
///
/// Round 2 of this function asserted, unconditionally, *"this is not a permissions problem: the identical
/// listing … had just succeeded, so `s3:ListBucket` is demonstrably held"*. That is **past evidence
/// stated as a present fact**, and it is false on a reachable path: the two listings are back to back but
/// **not atomic**, so a credential's authority can change between them. An **expired STS session token**
/// is the obvious case — a hard cliff that surfaces as a 403 on whichever request crosses it, and this
/// belt is *by construction* the second request of a pair — with a revoked or edited bucket policy right
/// behind it. The UAT built both, and the message contradicted itself inside one paragraph: "not a
/// permissions problem", four lines above a body reading "the bucket policy or IAM policy denies this
/// request".
///
/// That is round 1's defect mirrored. Round 1 asserted a cause that was provably false at this site;
/// round 2 asserted the *absence* of a cause, provably false on a 403. **"It is not X" is a claim and
/// needs the same evidence as "it is X"** — so the category is now decided by the status the server
/// actually sent, exactly the way [`list_failure`] decides its permission sentence, and on a denial the
/// message names the thing that really did change: the credential's authority, between two requests.
///
/// `status` is `None` when no HTTP status was read at all (transport failure, unparseable body). That is
/// not a denial either, but it is not evidence of one *not* having happened, so it takes the neutral arm's
/// wording without the "the server answered N" clause.
///
/// # The `refused_before_body_read` flag (CPE-1740, PR #911 review)
///
/// A bare `Option<u16>` cannot tell "refused on status alone" apart from "the body was engaged with and
/// THAT is what failed" — and both can carry the identical non-canonical-2xx status. `signed_exchange`
/// treats every `2xx` as `ok`, so its own body-read failure or over-cap refusal under, say, a `203`
/// returns `Err((Some(203), …))` from `signed_get` — a status this module refuses, but one whose body WAS
/// read, up to the point reading it failed. [`ProbeRefusal::refused_before_body_read`] is `true` only for
/// [`S3Provider::probe_prefix_after`]'s own `status != 200` short-circuit, where the body is fully in hand
/// and simply never parsed. Selecting the "before its body was ever read" wording on status alone — the
/// round-1 CPE-1740 fix's own bug — reproduces exactly the contradiction this ticket exists to remove,
/// mirrored: a 203 whose body was read to an 8 MiB cap and refused for THAT reason still claimed its body
/// was "never read", one sentence above an "Underlying error" naming the byte cap.
fn marker_confirmation_failure(path: &str, key_prefix: &str, refusal: &ProbeRefusal) -> String {
    let preamble = format!(
        "s3: delete {path:?}: nothing has been deleted. The first listing said this prefix holds nothing \
         but its own directory marker, which is the one verdict on which a single key really can be \
         removed — so it is confirmed with a second ListObjectsV2 for the prefix {key_prefix:?}, this one \
         carrying start-after set to the marker key, and that request failed."
    );
    let diagnosis = match refusal.status {
        // A denial on the SECOND request of a pair whose first was allowed. The interesting fact is not
        // "you lack a permission" — that was true a moment ago and is not what changed — it is that the
        // authority itself moved underneath the call.
        Some(s @ (401 | 403)) => format!(
            " The server answered HTTP {s}, a denial — and the same listing, same prefix and same \
             credential, differing only by the start-after parameter, had succeeded moments earlier. \
             **The server's own error code below names what changed.** A code about the \
             credential's authority — an expired session token (an STS token hits its expiry as a cliff, \
             on whichever request crosses it), or a policy revoked or edited in between — means \
             re-authenticate or refresh the token and try again. A code about the *signature* or the \
             clock means this request was refused on how it was formed rather than on who sent it; note \
             that the second request differs from the first only by the start-after parameter, so a \
             gateway that signs an unexpected parameter differently lands here too."
        ),
        // The server answered exactly 200, and answered *successfully* — the failure was ours, reading
        // what came back (bad UTF-8, malformed XML). Round 3 had no such arm, so this fell to `None` and
        // claimed the server never answered while quoting a parse of its body. It is a distinct diagnosis
        // from both the others: nothing is known about permissions, and nothing is implied about
        // `start-after` either.
        Some(200) => " The server answered HTTP 200 — it did reply, and successfully; what failed was \
             reading the reply. That says nothing about permissions, and nothing about whether \
             start-after is supported: the response arrived and could not be understood."
            .to_string(),
        // A 2xx that is not 200 — 203 or 206 among them — refused ON ITS STATUS, before the body was ever
        // inspected: `refused_before_body_read` is `true` only for `probe_prefix_after`'s own
        // `status != 200` short-circuit, so the body genuinely was never parsed here. The wording this
        // replaces claimed "reading … failed" for every non-canonical 2xx unconditionally and routed the
        // body through the S3 error parser, which reported "no <Code> element" about a body that was
        // never an error (CPE-1740, PR #903 round-5 UAT).
        Some(s) if is_non_canonical_listing_status(s) && refusal.refused_before_body_read => format!(
            " The server answered HTTP {s} — it did reply, and successfully by HTTP's own rules, but a \
             listing requires exactly HTTP 200 and this answered {s} instead, so it was refused on its \
             status before its body was ever inspected. That says nothing about permissions, and nothing \
             about whether start-after is supported."
        ),
        // A 2xx that is not 200, where the body WAS engaged with — a read failure or the over-cap refusal
        // — and THAT is why the reply was refused, not merely its status. Conflating this with the arm
        // above (claiming "before its body was ever read" here too) is the PR #911 review finding: the
        // mirror image of the original defect, on the identical status. The underlying error below
        // already names the real reason; this arm only needs to avoid the false claim.
        Some(s) if is_non_canonical_listing_status(s) => format!(
            " The server answered HTTP {s}, and its reply could not be used as a listing for the reason \
             given below. {s} is not the HTTP 200 a listing requires either way, so this reply would have \
             been refused regardless of what its body held. That says nothing about permissions, and \
             nothing about whether start-after is supported."
        ),
        // A status that is not S3's way of saying "no". Note the claim is scoped to what the status
        // licenses — the server did not *refuse* this request — rather than to "this is not a
        // permissions problem", which the 401/403 list cannot support on its own: a gateway reporting
        // an expired token as 400 would land here, and the stronger sentence would then be a guess
        // dressed as a finding. (PR #903 round-3 review.)
        Some(s) => format!(
            " The server answered HTTP {s} — not one of the statuses S3 denies with. What the second \
             request adds is the start-after parameter, so a server that does not implement it — or a \
             transient failure — is a likely explanation; the server's own error code below is the \
             better evidence."
        ),
        None => " No HTTP status was obtained at all, so the request failed before any reply — most \
                 likely transport. Nothing here is evidence about permissions either way."
            .to_string(),
    };
    format!("{preamble}{diagnosis} Underlying error: {}", refusal.message)
}

/// Wrap a failed `ListObjectsV2` from [`S3Provider::list_with_filtered_count`] so the message names **the
/// operation, the path, and — on a denial — the permission** (CPE-1727 item 3).
///
/// # What it was
///
/// `list` was the last non-2xx path in the provider still surfacing `error::map_s3_error`'s output raw:
///
/// ```text
/// s3: HTTP 403 AccessDenied: Access Denied. — the credentials are valid but the bucket policy or IAM
/// policy denies this request
/// ```
///
/// No path, no operation, and no mention of `s3:ListBucket`. That is an odd gap, because `list` is the one
/// operation that **genuinely always** needs `s3:ListBucket` — `stat`/`read`/`write`/`delete` all have a
/// per-object path that works without it — and it is the first thing a user hits when browsing a bucket.
/// [`probe_failure`] had already been given this treatment for the *internal* probes; this is the same
/// treatment for the listing the user actually asked for.
///
/// # Why the permission sentence is conditional on the status
///
/// A 500 or a 503 is not an entitlement problem, and telling a user to grant `s3:ListBucket` when their
/// gateway is merely down is a confident wrong answer of exactly the kind this crate is written against.
/// The operation and the path are named for every status; the permission is named only for 401/403, where
/// it really is the likeliest cause. The underlying error is appended verbatim in both cases, so nothing
/// `map_s3_error` diagnosed is lost.
fn list_failure(path: &str, key_prefix: &str, status: u16, cause: &str) -> String {
    let permission = if matches!(status, 401 | 403) {
        " Listing is the one operation that always needs s3:ListBucket on the bucket: S3 has no real \
         directories, so a ListObjectsV2 request is the only way to see one, and no per-object permission \
         substitutes for it. If the credentials carry s3:GetObject but not s3:ListBucket — an ordinary \
         MinIO/Ceph policy — that is the likeliest cause, and granting s3:ListBucket on this bucket is the \
         fix."
    } else {
        ""
    };
    format!(
        "s3: list {path:?}: this listing is a ListObjectsV2 request for the prefix {key_prefix:?}, and \
         that request failed, so nothing about the contents of this path is known.{permission} Underlying \
         error: {cause}"
    )
}

/// Whether `len` bytes exceed what one S3 `PUT` can carry ([`MAX_SINGLE_PUT_BYTES`]).
///
/// A separate predicate purely so the boundary is testable without allocating five gigabytes — the guard it
/// backs is otherwise only reachable by actually having a 5 GiB slice in hand. Takes `u64` rather than
/// `usize` so the boundary can be named exactly on a 32-bit target too, where a `usize` could not hold it.
fn too_big_for_single_put(len: u64) -> bool {
    len > MAX_SINGLE_PUT_BYTES
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

/// True if `leaf` — the part of an S3 key after the requested `key_prefix` (see [`leaf_under_prefix`]) —
/// is safe for [`S3Provider::list`] to surface as a [`ProviderEntry`]'s `name`.
///
/// This is a SIBLING of `cpe_server::transfer::is_safe_name` (CPE-1704), not a call into it. `is_safe_name`
/// is the traversal guard for local paths, SFTP and WebDAV (CPE-1461), where it correctly refuses any leaf
/// containing `:` — a Windows drive-letter/NTFS alternate-data-stream hazard — and any leaf *starting with*
/// `..` (an ADS trick: `..::$DATA`). Neither rule means anything in an S3 keyspace: there are no drive
/// letters, no alternate data streams, and `:` is a completely ordinary key byte. Reusing that guard here
/// was CPE-1704's bug — a legal key like `colon:name.txt` vanished from every listing with no error, no
/// warning, nothing.
///
/// **Correction (CPE-1811):** this used to say the function checks *only* what is about a leaf escaping
/// the listed prefix, and left `!leaf.is_empty()` off the list below entirely — the arm CPE-1801's fix now
/// leans on (see [`parse_list_bucket_result`]'s `CommonPrefixes` loop). That framing was never true of
/// `!leaf.is_empty()`, which is not an escaping check: an empty leaf has nothing in it to escape *with*.
/// The seven arms, complete:
///
/// - An embedded or leading path separator (`/` or `\`), and a leaf that is exactly `..` or `.` — these
///   four are genuinely about escaping the listed prefix once round-tripped back through
///   [`provider_path_to_key_prefix`] into a later `list`/`stat`/`read` call's `path`.
/// - A control byte (NUL, a raw newline/CR, …) that could corrupt a rendered listing or a log line — not
///   an escape, but still unsafe to surface verbatim.
/// - An *empty* leaf — also not an escape, but never addressable on its own: once
///   `S3Provider::is_safe_leaf_name` (CPE-1704 round 3, below) made this guard run on every name
///   `crates/vfs::connect::remote_dir_entries` sees, an accepted `""` would resolve to a self-referential
///   row pointing back at the directory being listed, not merely fail to display inside
///   `parse_list_bucket_result`.
/// - A leaf longer than [`MAX_KEY_LEAF_BYTES`] — the length bound, documented in its own section below
///   (CPE-1706 item 3) rather than repeated here.
///
/// # CPE-1704 round 2: no percent-decoding here, on purpose
///
/// An earlier version of this function additionally percent-decoded the leaf once and re-checked the
/// decoded form, on the theory that `..%2f` — no raw `/` byte, but decodes to `../` — could reach some
/// downstream consumer that itself percent-decodes a display name. **That theory was checked and found
/// false**, and the check reintroduced this ticket's own bug: S3 key bytes are never implicitly
/// percent-decoded anywhere in this crate or its callers — [`provider_path_to_key_prefix`] and
/// [`leaf_under_prefix`] are both plain byte/string operations, `crates/vfs` passes `path` straight
/// through, and nothing in `src/`/`src-tauri/` decodes an entry's `name` — and CPE-1684 documents the same
/// design intent explicitly ("S3 must **not** normalise dot segments: key `a/../b.txt` is a real, distinct
/// key"). A percent-decode pass could not tell *"a key whose bytes happen to look like an encoded slash"*
/// from *"a key that legitimately contains the four characters `%2f`"* — because both are just text, and
/// S3 never treats one differently from the other. Refusing on the decoded shape therefore hid a real,
/// harmless key (`report%2ffinal.txt`) exactly like the `:` bug this ticket exists to fix, for a threat
/// that cannot occur on any path this leaf actually travels. If a *future* consumer is added that does
/// decode an entry name (a URL builder, a proxy), the guard belongs at that decode site, where the actual
/// risk lives — not here, where it can only cost legal keys.
///
/// A leaf this refuses is a real S3 key that will not appear in the listing as itself; see
/// [`S3Provider::list_with_filtered_count`]'s doc for how that is surfaced instead of silently dropped.
///
/// # CPE-1704 round 3: this is now also `S3Provider`'s [`FileSystemProvider::is_safe_leaf_name`] override
///
/// `crates/vfs::connect::remote_dir_entries` — the ONE code path a user's remote listing actually takes —
/// used to re-filter every provider's entries through the hardcoded `cpe_server::transfer::is_safe_name`,
/// regardless of which backend produced them. That silently re-dropped `colon:name.txt` a second time,
/// downstream of this function, even after this function had correctly let it through — the ticket's
/// opening bug, surviving one layer further out. `is_safe_leaf_name` is now a
/// [`cpe_server::provider::FileSystemProvider`] trait method (default: `is_safe_name`, unchanged for
/// local/SFTP/WebDAV/FTP) that a provider overrides to state its own rule; `S3Provider` overrides it to
/// call this function, so `remote_dir_entries` asks the right question instead of assuming one answer
/// fits every backend.
/// # CPE-1706 item 3: the length bound belongs here, not at the call sites
///
/// [`MAX_KEY_LEAF_BYTES`] is checked as one more arm of this same function rather than as a separate
/// `if` in [`parse_list_bucket_result`], for two reasons. First, the ticket's own wording — "drop
/// over-long keys **the way any other unsafe name is dropped**" — and post-CPE-1704 that way includes
/// being *counted* into `filtered_count` rather than silently vanishing, which a separate `continue`
/// would not have been. Second, this function is also `S3Provider`'s
/// [`cpe_server::provider::FileSystemProvider::is_safe_leaf_name`] override, so putting the bound here
/// makes `crates/vfs::connect::remote_dir_entries` apply it too, on the one code path a user's remote
/// listing actually takes; a check sitting in the parser would have been invisible from there.
///
/// Note this is the *one* arm that is a length bound rather than a byte-class rule, and it is set at real
/// S3's own documented key limit — so unlike a shape rule it can never refuse a key a conforming server
/// could produce. See [`MAX_KEY_LEAF_BYTES`].
fn is_safe_s3_leaf(leaf: &str) -> bool {
    !leaf.is_empty()
        && leaf.len() <= MAX_KEY_LEAF_BYTES
        && leaf != ".."
        && leaf != "."
        && !leaf.contains('/')
        && !leaf.contains('\\')
        && !leaf.chars().any(|c| c.is_control())
}

/// One page of a `ListObjectsV2` response, parsed.
struct ListPage {
    entries: Vec<ProviderEntry>,
    /// How many `<Contents>`/`<CommonPrefixes>` entries on this page were dropped by [`is_safe_s3_leaf`]
    /// (CPE-1704) — NOT counting entries dropped for being outside the requested prefix (a different,
    /// server-misbehaviour case; see [`parse_list_bucket_result`]'s doc).
    /// [`S3Provider::list_with_filtered_count`] sums this across every page and returns it as a real,
    /// honest `usize` alongside the entries — never a synthetic row mixed into the `Vec` itself (CPE-1704
    /// round 3: an earlier version of this fix did exactly that, and it turned out to be worse than the
    /// silent drop it replaced — see that method's doc).
    filtered_count: usize,
    /// How many `<Contents>`/`<CommonPrefixes>` entries on this page genuinely sat under the requested
    /// prefix, counted **before** the marker filter and **before** [`is_safe_s3_leaf`] — i.e. the raw
    /// question *"does anything at all exist under this prefix?"* (CPE-1684).
    ///
    /// `entries` cannot answer that, and the gap is not hypothetical: a directory holding only its own
    /// zero-byte marker — exactly the shape [`FileSystemProvider::mkdir`] writes — yields **zero**
    /// `entries`, because the marker is correctly filtered as the directory itself rather than as a file
    /// inside it. Using `entries.is_empty()` as "this directory does not exist" would therefore report a
    /// directory this module had just created as missing.
    ///
    /// Entries the server returned *outside* the requested prefix are deliberately not counted: a server
    /// answering outside its own advertised prefix is misbehaving, and letting that invent a directory
    /// would be trusting exactly the response this module refuses to trust everywhere else.
    raw_entries: usize,
    is_truncated: bool,
    next_token: Option<String>,
}

/// The failure [`S3Provider::probe_prefix_after`] returns — the HTTP status (when one was obtained)
/// alongside **whether the refusal happened on that status alone, before the reply's body was ever
/// inspected**.
///
/// [`marker_confirmation_failure`] needs that distinction and cannot recover it from a bare
/// `Option<u16>`: `signed_exchange` treats every `2xx` as `ok` (its over-cap/body-read guards only fire
/// inside its own `Ok` return path), so a genuine body-read failure or the over-cap refusal under a
/// NON-canonical 2xx — a `203` whose body happens to be larger than [`MAX_RESPONSE_BODY_BYTES`], say —
/// comes back from `signed_get` as `Err((Some(203), "…exceeded the …-byte cap…"))`: a status this module
/// refuses, but NOT one refused "before its body was ever read" — the body WAS engaged, and reading it is
/// exactly what failed. Conflating the two produced the mirror image of the defect CPE-1740 exists to
/// remove: the old wording claimed "reading … failed" for a status refused untouched; this claimed
/// "refused … before its body was ever read" for a body that had just been read to the cap and rejected.
/// Found on PR #911's review of CPE-1740's first attempt.
#[derive(Debug)]
struct ProbeRefusal {
    /// The HTTP status the server answered, when one was obtained at all (`None` on a transport failure
    /// or a signing error — the request never got that far).
    status: Option<u16>,
    /// `true` only when [`S3Provider::probe_prefix_after`]'s OWN `status != 200` check is what refused
    /// the reply — the one case where the body genuinely was never inspected. `false` for every other
    /// failure (transport, a real S3 denial, a body-read failure, the over-cap refusal, unparseable
    /// UTF-8/XML) — even when `status` happens to be a non-canonical 2xx, because in every one of those
    /// the body was at least attempted.
    refused_before_body_read: bool,
    /// The underlying diagnosis — `map_response_error`'s wording, `non_canonical_listing_status_cause`'s,
    /// or `signed_exchange`'s own body-read/over-cap/transport message — appended verbatim by
    /// [`marker_confirmation_failure`].
    message: String,
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
///   also stripped) must pass [`is_safe_s3_leaf`] — an S3-appropriate SIBLING of
///   `cpe_server::transfer::is_safe_name` (CPE-1704), not that function itself: `is_safe_name` is the
///   traversal guard for local paths, SFTP and WebDAV, where a `:` is a Windows drive-letter/NTFS
///   alternate-data-stream hazard and is correctly refused; S3 has no such concept and `:` is a completely
///   legal key byte, so reusing that guard here was silently dropping every legal `colon:name.txt`-shaped
///   key from a listing with no error, no warning, nothing (CPE-1704). [`is_safe_s3_leaf`] keeps every
///   check that is actually about a leaf escaping the listed prefix once round-tripped back through
///   [`provider_path_to_key_prefix`] into a later request — an embedded or leading path separator, a
///   literal `..` segment, a control byte, and (CPE-1706) a leaf past [`MAX_KEY_LEAF_BYTES`] — and drops
///   the filesystem-only rules (`:`, and "starts_with('..')" ADS hardening) that don't apply to a keyspace
///   with no drive letters and no alternate data streams.
///
///   **Correction (CPE-1706 round 2):** this sentence used to add *"including its percent-encoded form,
///   e.g. `..%2f`"*. That was never true of the shipped code and is the opposite of what CPE-1704
///   decided — [`is_safe_s3_leaf`]'s own doc explains at length why the percent-decode pass was removed,
///   and `keys_that_only_look_like_traversal_once_percent_decoded_are_kept` deliberately asserts that
///   `report%2ffinal.txt` and friends are **accepted**. The behaviour is right; the doc was left behind
///   by the edit that fixed it. A wrong comment on a security guard is worse than none: the next reader
///   believes a case is covered and stops checking.
/// - A leaf this guard genuinely must refuse is counted in `ListPage::filtered_count` rather than silently
///   vanishing (CPE-1704 — see [`S3Provider::list`]'s doc for what happens to that count).
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

    // **The document must actually BE a listing.** Nothing checked the root element, so any well-formed
    // XML read as a listing — and one with no `<Contents>` read as an *empty* one. A proxy's
    // `<html>…</html>` error page therefore meant "this prefix holds nothing", which on the delete belt
    // is the single verdict that permits removing a key. Measured by the round-4 UAT: the marker deleted
    // on the strength of an HTML error page.
    //
    // This is the same principle the module already states and CPE-1706 already applied one level down —
    // *never assume a network-controlled response honours its own protocol.* The page-level fields were
    // taught to read from their own container; this teaches the parser to check it has the right
    // container at all. Fixed here rather than deferred to CPE-1740 because the belt's half of that
    // ticket is a deletion path, and it is one condition. CPE-1740 keeps the rest.
    if !root.has_tag_name("ListBucketResult") {
        return Err(format!(
            "s3: the response parsed as XML but its root element is <{}>, not <ListBucketResult> — \
             this is not a ListObjectsV2 listing, and an unrecognised document must not be read as an \
             empty one",
            root.tag_name().name()
        ));
    }

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
    let mut filtered_count = 0usize;
    let mut raw_entries = 0usize;

    for content in root.children().filter(|n| n.tag_name().name() == "Contents") {
        let Some(key) = content.children().find(|n| n.tag_name().name() == "Key").and_then(|n| n.text())
        else {
            continue;
        };
        let Some(leaf) = leaf_under_prefix(key, key_prefix) else { continue };
        // Counted here, before every filter below — see `ListPage::raw_entries`. The marker entry (whose
        // leaf is empty) is counted deliberately: it is the one thing that proves an otherwise-empty
        // directory exists.
        raw_entries += 1;
        if leaf.is_empty() {
            continue; // the directory's own zero-byte marker object, not a file inside it (AC4)
        }
        if !is_safe_s3_leaf(leaf) {
            // AC5/CPE-1704: a hostile/malformed key must not escape the listed prefix. Counted, not
            // silently dropped — see ListPage::filtered_count and S3Provider::list. CPE-1706 item 3's
            // length cap lives inside `is_safe_s3_leaf` rather than here, so an over-long key is dropped
            // — and counted — by exactly the same path as any other unsafe name.
            filtered_count += 1;
            continue;
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
        raw_entries += 1;
        let leaf = leaf.trim_end_matches('/');
        // CPE-1801: there used to be a separate `if leaf.is_empty() { continue; }` right here, dropping
        // a slashes-only key (its `CommonPrefixes` rollup trims down to `""`) *before* the counting arm
        // below — invisible AND uncounted, silently breaking CPE-1704's `entries.len() + filtered_count`
        // delete-path total. Unlike `Contents`' own `leaf.is_empty()` arm just above (that one is the
        // directory's own marker object — a distinct, legitimate object deliberately tracked via
        // `raw_entries` instead, per `ListPage::raw_entries`'s doc), there is no marker here to exclude:
        // a `CommonPrefixes` entry only exists when something sits beyond the requested prefix, so an
        // empty leaf here means the object itself — the slashes-only key — has no displayable name, full
        // stop. `is_safe_s3_leaf` already refuses `""` as one of its own arms, so removing the special
        // case routes it through the same refusal as any other unsafe leaf: still invisible, but counted.
        if !is_safe_s3_leaf(leaf) {
            // AC5, the directory-entry mirror of the Contents check above; also counted. The CPE-1706
            // item 3 length cap is inside `is_safe_s3_leaf`, so it mirrors here for free — and, since
            // CPE-1801, so does the empty-leaf (slashes-only key) case.
            filtered_count += 1;
            continue;
        }
        entries.push(ProviderEntry { name: leaf.to_string(), is_dir: true, size: 0 });
    }

    Ok(ListPage { entries, filtered_count, raw_entries, is_truncated, next_token })
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
    /// End-to-end deadline for one `ListObjectsV2` request — [`TIMEOUT_LIST_REQUEST`] in production, a
    /// field for the same reason `list_deadline` is. It has to be overridable *separately* from the
    /// agent's `timeout_read`, because setting a per-request deadline replaces that read timeout rather
    /// than layering on it, so a test cannot reach this bound by shrinking the other one.
    request_deadline: Duration,
    /// Per-object byte cap for `read` — [`MAX_OBJECT_READ_BYTES`] in production, a field for the same
    /// reason the two deadlines are: so the refusal can be observed firing against a few kilobytes rather
    /// than by streaming two gigabytes through a test. Mirrors `cpe-webdav`'s `read_cap` exactly.
    read_cap: u64,
}

impl S3Provider {
    /// Build a provider for `config`. Does not perform a request; the first `list`/`stat`/`read`/… issues
    /// one and surfaces addressing/auth/connection errors then.
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
            agent: build_agent(timeout_read, timeout_write, TIMEOUT_CONNECT),
            list_deadline: MAX_LIST_WALL_CLOCK,
            request_deadline: TIMEOUT_LIST_REQUEST,
            read_cap: MAX_OBJECT_READ_BYTES,
        }
    }

    /// Override the per-object `read` byte cap ([`MAX_OBJECT_READ_BYTES`] by default). Production never
    /// calls this; it exists so the cap can be observed refusing an over-size object without a test having
    /// to push 2 GiB through a socket. Same rationale, same shape, as `cpe-webdav`'s `with_read_cap`.
    pub fn with_read_cap(mut self, cap: u64) -> Self {
        self.read_cap = cap;
        self
    }

    /// Override the per-request end-to-end deadline ([`TIMEOUT_LIST_REQUEST`] by default). Production
    /// never calls this; it exists so the dribble guard can be observed firing in a second instead of a
    /// minute. See [`S3Provider::connect_with_timeouts`] for the same reasoning applied to the agent.
    pub fn with_request_deadline(mut self, deadline: Duration) -> Self {
        self.request_deadline = deadline;
        self
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
    /// `request_deadline` bounds **this one request end to end**, including reading its body, and is the
    /// bound that closes the dribble hole (CPE-1706 round 2). It must be `Some` for a small,
    /// already-byte-capped response like `ListObjectsV2` and **`None` for a large-object `GET`** — see
    /// [`TIMEOUT_LIST_REQUEST`] for why that distinction is the whole point.
    ///
    /// The error carries `Option<u16>`: **`Some` when the server answered and the failure came after**
    /// (a body-read failure, the over-cap refusal), `None` when no status was ever obtained. Only
    /// [`S3Provider::probe_prefix_after`] reads it — every other caller drops it with
    /// `.map_err(|(_, m)| m)` — but it must be produced *here*, because here is the only place that
    /// knows. Round 4 of CPE-1727 mapped these failures to `None` and produced a message saying the
    /// request "failed before any reply" while quoting a failure to read the reply. See
    /// [`marker_confirmation_failure`].
    fn signed_get(
        &self,
        target: &RequestTarget,
        query: &[(&str, &str)],
        request_deadline: Option<Duration>,
    ) -> Result<(u16, Vec<u8>), (Option<u16>, String)> {
        self.signed_exchange("GET", target, query, None, request_deadline)
    }

    /// Sign one request and build the `ureq::Request` for it, without sending — the single place every verb
    /// in this crate (`GET`, `HEAD`, `PUT`, `DELETE`) turns a [`RequestTarget`] into something sendable, so
    /// no verb can drift into signing one thing and sending another. Returns the URL alongside the request
    /// because every error message downstream wants it and rebuilding it would reintroduce exactly the
    /// two-constructions hazard [`RequestTarget`] exists to remove.
    ///
    /// `payload_hash` is the SigV4 `x-amz-content-sha256` value: [`sigv4::EMPTY_PAYLOAD_SHA256`] for a
    /// bodiless verb, `sha256_hex(body)` for a `PUT`. It is both signed and sent, so the two cannot
    /// disagree — passing it as one parameter used twice is the point.
    ///
    /// `request_deadline` bounds the whole exchange, body read included, and is a **per-call-site
    /// decision**: `Some(..)` for the small metadata exchanges (a `ListObjectsV2` page, a `HEAD`, a
    /// `DELETE`, the zero-byte marker `PUT`), and **`None` for a bulk transfer** — a large-object `GET` or
    /// `PUT` is legitimately slow over a poor link and must keep [`TIMEOUT_READ`]'s per-read stall
    /// semantics instead. See [`TIMEOUT_LIST_REQUEST`] for why that distinction is the whole point.
    fn signed_request(
        &self,
        method: &str,
        target: &RequestTarget,
        query: &[(&str, &str)],
        payload_hash: &str,
        request_deadline: Option<Duration>,
    ) -> Result<(String, ureq::Request), String> {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("s3: system clock reads before the Unix epoch: {e}"))?
            .as_secs();
        let amz_date = sigv4::amz_date_from_unix(secs as i64);

        let signer = sigv4::Signer::new(&self.config.credentials, &self.config.region)?;
        let signed = signer.sign(&sigv4::SigningInput {
            method,
            encoded_path: &target.encoded_path,
            query,
            headers: &[
                ("host", target.host.as_str()),
                ("x-amz-date", amz_date.as_str()),
                ("x-amz-content-sha256", payload_hash),
            ],
            payload_hash,
            amz_date: &amz_date,
        })?;

        // Guard every header that will actually be sent, INCLUDING the `Host` ureq derives automatically
        // from the URL (never explicitly `.set()` below, but signed above and just as capable of being
        // silently dropped if `target.host` ever carried a byte outside ureq's sendable set — see this
        // module's top doc, "The ureq-header-drop decision"). Checked here rather than left to `S3Config`
        // validation because it is a property of the transport, not of addressing correctness.
        guard_header_sendable("Host (in the request URL's authority)", &target.host)?;
        guard_header_sendable("x-amz-date", &amz_date)?;
        guard_header_sendable("x-amz-content-sha256", payload_hash)?;
        guard_header_sendable("Authorization", &signed.authorization)?;
        // The path half of the same problem, and the one the ticket told this worker to measure first:
        // `ureq` rewrites a path carrying a dot segment between here and the wire, so what is signed above
        // would not be what is sent below. Checked in this one place, so no verb can miss it.
        guard_path_survives_the_client(&target.encoded_path)?;

        let url = target.url_with_query(query);
        let mut req = self
            .agent
            .request(method, &url)
            .set("x-amz-date", &amz_date)
            .set("x-amz-content-sha256", payload_hash)
            .set("Authorization", &signed.authorization);
        // `ureq::Request::timeout` is per-REQUEST and overrides the agent's per-read setting for this one
        // call (`request.rs:60`, "overriding agent's configuration if any"). Unlike the agent-level
        // `.timeout()` this crate still declines, applying it here is a per-call choice, so the large
        // object GET/PUT keeps the per-read semantics it actually wants.
        if let Some(deadline) = request_deadline {
            req = req.timeout(deadline);
        }
        Ok((url, req))
    }

    /// Send `req` and hand back the status **and the response**, never deciding success or failure itself:
    /// a non-2xx comes back as `Ok((status, resp))` rather than an `Err`, so every caller reads the body
    /// through the same code and sees the exact bytes the server sent.
    ///
    /// `body` distinguishes a bodiless verb (`None` → `.call()`) from a `PUT` (`Some` → `.send_bytes`).
    /// Both funnel through `ureq`'s `do_call`, which validates every outbound header **before** any socket
    /// work — see this module's top doc for why that matters and what was measured about it.
    fn send_capturing_status(
        req: ureq::Request,
        body: Option<&[u8]>,
        url: &str,
    ) -> Result<(u16, ureq::Response), String> {
        let sent = match body {
            Some(bytes) => req.send_bytes(bytes),
            None => req.call(),
        };
        match sent {
            Ok(resp) => Ok((resp.status(), resp)),
            // A non-2xx status still carries whatever body the server chose to send (S3's error detail
            // lives there); it is the caller's job to interpret, not this function's.
            Err(ureq::Error::Status(code, resp)) => Ok((code, resp)),
            Err(ureq::Error::Transport(t)) => Err(format!("s3: {url}: {t}")),
        }
    }

    /// Sign, send, and read a **small** response body to completion, capped by
    /// [`MAX_RESPONSE_BODY_BYTES`] — the shared path for `ListObjectsV2`, `HEAD`, `DELETE` and the marker
    /// `PUT`. Never used for a large-object `GET`, whose body has its own, much larger cap and its own
    /// chunked loop ([`S3Provider::read`]).
    fn signed_exchange(
        &self,
        method: &str,
        target: &RequestTarget,
        query: &[(&str, &str)],
        body: Option<&[u8]>,
        request_deadline: Option<Duration>,
    ) -> Result<(u16, Vec<u8>), (Option<u16>, String)> {
        let payload_hash = match body {
            Some(bytes) => sigv4::sha256_hex(bytes),
            None => sigv4::EMPTY_PAYLOAD_SHA256.to_string(),
        };
        // No status yet at either of these: signing has not sent anything, and `send_capturing_status`
        // only errors when no response came back at all (it maps `Error::Status` to `Ok`).
        let (url, req) = self
            .signed_request(method, target, query, &payload_hash, request_deadline)
            .map_err(|e| (None, e))?;
        let (status, resp) = Self::send_capturing_status(req, body, &url).map_err(|e| (None, e))?;
        // CPE-1749 re-check: deliberately still any `2xx`, not narrowed to `200`. This flag only gates
        // whether an over-cap/read-failure is treated as *this call's* failure (see `over_cap && ok`
        // below); it never decides success for a document a caller reads for completeness — every actual
        // consumer of a body this exchange produces (`list`, `probe_prefix`/`probe_prefix_after`) re-checks
        // `status != 200` itself on top of this, and `write`/`mkdir`/`delete` below check `(200..300)`
        // directly against a response that carries no document whose completeness matters (see those
        // call sites' own CPE-1749 notes).
        let ok = (200..300).contains(&status);
        // On the success path a body-read failure is the call's failure. On the error path it is not:
        // `map_response_error`/`error::map_s3_error` already handle an empty/truncated/garbled body
        // honestly, and failing there would replace S3's own diagnosis with a transport complaint.
        let (buf, over_cap) = match read_body_capped(resp.into_reader()) {
            Ok(v) => v,
            // `Some(status)` — the server DID answer; this failure is downstream of that. Round-4 UAT.
            Err(e) if ok => {
                return Err((
                    Some(status),
                    format!("s3: {url}: reading the response body failed: {e}"),
                ))
            }
            Err(_) => (Vec::new(), false),
        };
        // Deliberately only on the success path, unchanged from CPE-1706: an over-cap ERROR body is not
        // worth failing the call over, because it is already being interpreted best-effort.
        if over_cap && ok {
            return Err((
                Some(status),
                format!(
                    "s3: {url}: the response body exceeded the {MAX_RESPONSE_BODY_BYTES}-byte cap \
                     without finishing — refusing rather than parsing a truncated body, which can \
                     look like a complete but much shorter listing"
                ),
            ));
        }
        Ok((status, buf))
    }

    /// One `ListObjectsV2` request against `key_prefix` asking for a single key, used purely to answer
    /// *"does anything exist under this prefix?"* — the question that separates a virtual directory from a
    /// path that is simply not there (CPE-1684).
    ///
    /// Returns `(raw_entries, real_entries, is_truncated)`: the first counts everything under the prefix
    /// **including the zero-byte marker object**, the second counts only entries that would be shown as
    /// children, and the third is the page's own `IsTruncated`. Together they distinguish the three cases
    /// `stat` and `delete` need — nothing there, an empty directory holding only its own marker, and a
    /// directory with real content. See [`ListPage::raw_entries`].
    ///
    /// # The marker always comes back first, which is why one key is not enough to look at
    ///
    /// S3 returns keys in **lexicographic order**, and a directory's own marker key (`photos/2024/`) is a
    /// strict prefix of every key beneath it — so the marker is *always* the first entry returned for its
    /// own prefix. A caller that looked only at the first key would see nothing but the marker for a
    /// directory holding a thousand objects, conclude "empty", and let `delete` remove the marker and
    /// report success.
    ///
    /// # `IsTruncated` is the load-bearing half; `max-keys=2` is a second, independent belt
    ///
    /// Stated in that order because it is what was measured, not what reads best. `IsTruncated=true` says
    /// there is more under this prefix no matter how few keys came back, and a marker-only page that is
    /// also truncated is therefore treated as a non-empty directory. That alone is sufficient **for a
    /// conforming server**, and the negative-control probe confirms it: removing the `IsTruncated` term
    /// reds `tests::a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete` on
    /// its own, while `max-keys` was still 2.
    ///
    /// `max-keys=2` is the second belt, and it is worth being exact about which failure it actually
    /// catches — an earlier version of this comment named the wrong one. It is **not** the gateway that
    /// under-fills: a server that returns one key when asked for two returns one key when asked for one
    /// too, so asking for more buys nothing against it. What the second key defends against is a gateway
    /// that **honours `max-keys` and lies about `IsTruncated`** — it hands back both keys it was asked
    /// for, so the second one is right there in the page, while its `IsTruncated=false` would have made a
    /// one-key probe conclude the directory was empty. That is precisely the server
    /// `tests::a_server_that_denies_being_truncated_is_still_seen_as_a_non_empty_directory` stands up, and
    /// with `max-keys` set to `1` that test — and only that test — reds.
    ///
    /// So the two halves cover genuinely different servers: `IsTruncated` covers every conforming one,
    /// and the second key covers one that reports its own pagination falsely. Neither subsumes the other,
    /// and neither is decoration.
    ///
    /// # What this cannot defend against, stated rather than left to be rediscovered (CPE-1723 item 6)
    ///
    /// A gateway that **under-fills a page AND denies being truncated** defeats both belts *in this
    /// function*. There is now a third belt, but it lives in [`FileSystemProvider::delete`] rather than
    /// here (it is one more request, and only `delete` needs it) — see the section below. The residual
    /// hole after that belt is narrower still: a server that under-fills, denies truncation **and ignores
    /// `start-after`**, which
    /// `tests::an_underfilling_server_that_also_denies_truncation_defeats_both_belts_and_that_is_recorded_not_fixed`
    /// measures going through — deleting the marker while the object under it survives.
    ///
    /// **The withdrawn claim, and the belt that replaced it (CPE-1727 item 2).** This said "there is no
    /// third question to ask", reasoning that every signal — key count, `IsTruncated`, continuation token
    /// — is a claim by the same server, so one willing to misreport two can misreport a third as cheaply.
    /// **A third question exists**, the CPE-1723 UAT built it, and CPE-1727 shipped it: on the marker-only
    /// verdict *and only there*, [`FileSystemProvider::delete`] re-lists the same prefix with
    /// **`start-after`** set to the marker key, via [`S3Provider::probe_prefix_after`]. That is not the
    /// same question asked again. Under-filling a page is *legal* S3 latitude, so the first lie costs the
    /// server nothing; returning **zero keys with `IsTruncated=false` when keys exist beyond the marker**
    /// is a flat protocol violation. The belt forces a strictly stronger lie rather than re-asking a
    /// question already answered falsely — which is precisely the move the withdrawn reasoning claimed was
    /// unavailable.
    ///
    /// **Why an earlier review concluded the opposite, recorded so it is not re-derived.** A naive belt
    /// counting `raw_entries` gets a false positive from the re-returned marker and reds
    /// `delete_of_an_empty_directory_removes_its_marker_key_because_that_really_is_one_key` — the honest
    /// server's empty-directory delete. Counting `entries.len() + filtered_count` instead removes it, and
    /// that is what the belt does. The `std::fs`-backed fixture used to **ignore `start-after` entirely**;
    /// CPE-1727 taught it the parameter, pinned by
    /// `tests::the_fixture_honours_start_after_so_the_belt_is_not_measured_against_a_server_that_ignores_it`.
    ///
    /// **Be exact about what that teaching buys, because the PR #903 review found the first wording one
    /// step past its evidence.** The belt's *catch* is measured against a bespoke server that never
    /// touches `list_page_xml`, so the fixture's support is not what proves the belt works. What it does
    /// prove is the other half — **no false positive**: `delete_of_an_empty_directory_...` runs through
    /// the fixture, so with the teaching in place that test really does exercise a server implementing
    /// `start-after`, instead of one that ignores the parameter and would have said "nothing past the
    /// marker" for free.
    ///
    /// # Scoped honestly — the belt has TWO limits, one permissive and one restrictive
    ///
    /// **Permissive.** It catches a server that under-fills, denies truncation, and then **honours
    /// `start-after`** (`tests::an_underfilling_server_that_denies_truncation_is_caught_by_the_start_after_belt`).
    /// It does **not** catch one that ignores `start-after` and re-serves the same marker-only page; that
    /// server is still undefended, and
    /// `tests::an_underfilling_server_that_also_denies_truncation_defeats_both_belts_and_that_is_recorded_not_fixed`
    /// measures it going through. Nor does it run at all on the `raw_entries == 0` verdict — see the gate
    /// comment in [`FileSystemProvider::delete`] for why that is a deliberate cost trade.
    ///
    /// **Restrictive, and this one is new behaviour rather than an unclosed gap — added after the PR #903
    /// review measured it.** The belt makes an **optional** `ListObjectsV2` parameter into a hard
    /// dependency of `delete`. A gateway that is fully permissive and entirely honest but simply does not
    /// implement `start-after` (answering, say, `400 InvalidArgument`) now **refuses an empty-directory
    /// delete that succeeded before this belt existed**:
    /// `tests::a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow`
    /// measures exactly that, on a credential holding every permission. Refusing is the conservative
    /// call and is kept deliberately — the alternative, treating a failed confirmation as consent, is
    /// how CPE-1723's original bug is written — but it is a *trade*, not a free belt, and the shape of
    /// the loss (an entitled operation removed by a new guard) is the very mistake CPE-1727 exists to
    /// undo, so it is recorded here rather than left to be discovered by a user.
    ///
    /// No gateway's real behaviour has been measured (see item 7); everything here is `tiny_http` on one
    /// developer machine, which is precisely why the `start-after` support question is unresolved.
    ///
    /// # Everything above is measured in-process only (CPE-1723 item 7)
    ///
    /// Every claim in this doc, and in CPE-1684's, comes from `tiny_http` and raw-socket fixtures over
    /// `ureq` on one developer machine. **No real S3, MinIO, Ceph or QNAP has been exercised.** Real
    /// pagination behaviour and real 403-vs-404 policy are unverified, which matters most for the
    /// `s3:ListBucket` reasoning at this function's two call sites. Standing caveat, not a TODO on this
    /// function.
    fn probe_prefix(&self, key_prefix: &str) -> Result<(usize, usize, bool), String> {
        self.probe_prefix_after(key_prefix, None).map_err(|refusal| refusal.message)
    }

    /// [`S3Provider::probe_prefix`] with an optional `start-after`: the same probe, asked only about keys
    /// **strictly after** `start_after` in S3's lexicographic order (CPE-1727 item 2).
    ///
    /// This exists for exactly one caller — [`FileSystemProvider::delete`]'s third belt, which passes the
    /// directory's own marker key and asks *"is there anything past the marker?"*. It is a separate
    /// question from the one `probe_prefix` asks, not a retry of it: a page may legally be under-filled,
    /// but a page that comes back empty and untruncated when keys exist beyond the marker is a flat
    /// protocol violation. See `probe_prefix`'s doc for why that distinction is what makes the belt worth
    /// a request.
    ///
    /// The return value keeps `probe_prefix`'s shape: the belt reads the second and third fields
    /// (`entries.len() + filtered_count`, and `IsTruncated`) and deliberately **not** `raw_entries` — a
    /// server that re-returns the marker itself despite `start-after` would otherwise make every honest
    /// empty-directory delete fail.
    ///
    /// # The error carries the HTTP status, and that is load-bearing (PR #903 UAT, round 3)
    ///
    /// `Err` is a [`ProbeRefusal`]: the status when the server answered one at all (`None` when the
    /// request never got that far — transport failure, a signing error), a `message`, and (CPE-1740, PR
    /// #911 review) whether the refusal happened on the status alone before the body was ever inspected.
    /// [`probe_prefix`] drops all but the message, because its callers already have a wording that fits
    /// every cause.
    ///
    /// The belt cannot. [`marker_confirmation_failure`] has to distinguish **"the server does not
    /// implement `start-after`"** from **"this credential's authority changed between the two
    /// listings"** — an expired STS session token or a revoked policy, which is a genuine 403 on the
    /// second request of a pair that is not atomic. Without the status the message can only guess, and
    /// the round-2 version guessed wrong in the safest-sounding direction: it asserted *"this is not a
    /// permissions problem"* on top of a body reading *"the bucket policy or IAM policy denies this
    /// request"*. Passing the status is what makes that message an observation instead of an inference
    /// from what happened a moment earlier.
    fn probe_prefix_after(
        &self,
        key_prefix: &str,
        start_after: Option<&str>,
    ) -> Result<(usize, usize, bool), ProbeRefusal> {
        let no_status = |message: String| ProbeRefusal {
            status: None,
            refused_before_body_read: false,
            message,
        };
        let target = self.config.bucket_target().map_err(no_status)?;
        let mut query: Vec<(&str, &str)> =
            vec![("list-type", "2"), ("delimiter", "/"), ("max-keys", "2")];
        if !key_prefix.is_empty() {
            query.push(("prefix", key_prefix));
        }
        if let Some(after) = start_after {
            query.push(("start-after", after));
        }
        // `signed_get`'s `Err` is ALWAYS downstream of an actual attempt to obtain the body — a body-read
        // failure or the over-cap refusal, both only reachable once a status has been read, or a
        // transport/signing failure that never got a status at all. Either way it is never "refused on
        // its status before its body was ever read" (see `ProbeRefusal::refused_before_body_read`'s doc;
        // conflating this with the check below is the PR #911 review finding).
        let (status, body) = self
            .signed_get(&target, &query, Some(self.request_deadline))
            .map_err(|(status, message)| ProbeRefusal {
                status,
                refused_before_body_read: false,
                message,
            })?;
        // **`200` exactly, not `2xx`.** A `206 Partial Content` says by definition that the reply is
        // incomplete, and `parse_list_bucket_result` will happily read a well-formed fragment of a
        // listing as a *complete* one — which on this path means "nothing past the marker" and the
        // DELETE goes out. The round-4 UAT measured exactly that: a 206 carrying an empty listing, and
        // `*** DELETE WENT THROUGH (Ok) ***`.
        //
        // `signed_exchange`'s over-cap guard already refuses truncation in so many words — "refusing
        // rather than parsing a truncated body, which can look like a complete but much shorter
        // listing" — and this is the identical hazard arriving by status code instead of by byte count.
        // A listing that is not whole cannot answer a question about absence.
        //
        // The wider family — any well-formed XML being accepted as a listing, `text/html` included —
        // is crate-wide, predates the belt, and was CPE-1740 (`FileSystemProvider::list`'s own pass over
        // the same hazard).
        //
        // A non-canonical 2xx (203/206) is diagnosed by `non_canonical_listing_status_cause`, not
        // `map_response_error`: the body here is not an S3 error to be parsed, it may well be a genuine
        // listing, and routing it through the error path produced exactly the wrong complaint — "no
        // <Code> element was found" about a body that has none because it was never an error (CPE-1740,
        // PR #903 round-5 UAT). THIS is the one branch where `refused_before_body_read` is genuinely
        // `true`: `body` is fully in hand (the `signed_get` call above already succeeded), and this
        // return is refusing it on `status` alone, without ever parsing it as a listing.
        if status != 200 {
            let (message, refused_before_body_read) = if is_non_canonical_listing_status(status) {
                (
                    format!(
                        "s3: probing {key_prefix:?}: {}",
                        non_canonical_listing_status_cause(status)
                    ),
                    true,
                )
            } else {
                (map_response_error("GET", status, &body, &format!("{key_prefix:?}")), false)
            };
            return Err(ProbeRefusal { status: Some(status), refused_before_body_read, message });
        }
        // **`Some(status)`, not `no_status`, from here down** — the server has answered by this point.
        // Round 3 used `no_status` for both, so a 200 whose body failed to parse produced a message
        // saying the request "failed before the server could answer" with an appended cause describing
        // the body it had read. `None` must mean *no status was ever obtained*, not *we discarded it*.
        // (PR #903 round-3 UAT.)
        let text = std::str::from_utf8(&body).map_err(|e| ProbeRefusal {
            status: Some(status),
            refused_before_body_read: false,
            message: format!("s3: probing {key_prefix:?}: response body was not valid UTF-8: {e}"),
        })?;
        let page = parse_list_bucket_result(text, key_prefix).map_err(|message| ProbeRefusal {
            status: Some(status),
            refused_before_body_read: false,
            message,
        })?;
        // `entries.len() + filtered_count`, NOT `entries.len()` — the round-2 UAT's sharpest finding.
        //
        // `entries` is what `list` would DISPLAY; `filtered_count` is what `is_safe_s3_leaf` refused to
        // display. Both are real objects sitting under this prefix: that guard's own doc says so outright
        // ("A leaf this refuses is a real S3 key"). The shapes that actually reach here are `\`, a leaf
        // that is exactly `..` or `.`, and a **tab or LF** — every one of them legal in an S3 key.
    //
    // Not CR, and the distinction was measured (CPE-1723 review): XML 1.0 §2.11 normalises CR→LF in
    // element content, so a CR-bearing key comes back from the parser as byte `0x0A`, not `0x0D`. The
    // reachable control-byte set **at this guard** is `{0x09, 0x0A}` — three bytes leave the server,
    // two arrive. `filtered_count` still moves either way, so the correction above is unaffected; this
    // names what the guard sees rather than what was sent, because that is the thing the sentence is
    // about.
        //
        // CPE-1723 item 8 flagged the earlier unqualified "control bytes" in this list as over-broad, on
        // the grounds that a raw control character makes the listing non-XML and `roxmltree` rejects the
        // whole document, so it fails loudly instead of being miscounted. That is right for most of the C0
        // range and **wrong for exactly three bytes**: XML 1.0 §2.2 exempts `0x09`/`0x0A`/`0x0D`, and
        // `char::is_control` says `true` for all three, so a tab-bearing leaf parses, is refused, and IS
        // counted. Measured, not reasoned about, by
        // `tests::a_tab_in_a_key_is_a_control_byte_that_really_does_reach_the_filtered_count_unlike_a_nul`,
        // which pins both halves. So the list above is four shapes wide, with the control byte narrowed to
        // the two that reach the guard (a CR arrives as LF -- XML 1.0 2.11) rather than dropped. Asking `entries.len()` alone answers
        // "what can I show the user here", when the question this function exists to answer is "is there
        // anything here at all". A directory whose only content was a filtered leaf therefore read as
        // empty, and `delete` removed its marker and reported success — on a fully conforming server.
        //
        // Not `raw_entries` either: that counts the directory's own marker object, which is the one key a
        // `delete` of an empty directory is legitimately allowed to remove.
        Ok((page.raw_entries, page.entries.len() + page.filtered_count, page.is_truncated))
    }

    /// `HEAD` the key itself and report whether the server answered 2xx — i.e. whether **an object exists
    /// at exactly this key** (CPE-1727 item 1).
    ///
    /// This is the one question about object-ness that a credential holding `s3:GetObject` but **not**
    /// `s3:ListBucket` is entitled to ask, and it is the question [`FileSystemProvider::delete`] falls back
    /// to when its prefix probe is denied. The property that makes it safe is a fact about the keyspace,
    /// not a claim by the server: a *pure prefix* has no object at its own key — `photos/2024` is not a
    /// key at all when the objects are `photos/2024/a.jpg` and `photos/2024/b.jpg` — so a virtual directory
    /// answers 404 (or 403) here and can never answer 200. A 2xx therefore proves the DELETE that follows
    /// removes one object and cannot silently orphan a subtree.
    ///
    /// **Scope that claim the way the PR body scopes it.** "A virtual directory can never answer 200" is a
    /// statement about the keyspace that follows from S3's data model, but as *evidence* it is only as good
    /// as what has been measured, and what has been measured is the in-process fixture, which 404s a HEAD
    /// on a directory. **No real S3, MinIO, Ceph or QNAP has been exercised**, so a gateway that
    /// synthesises a 200 for a prefix — inventing a directory object the way some S3 façades over a real
    /// filesystem do — would defeat this, and nothing here would notice. That is the first thing to check
    /// when a real endpoint becomes available (CPE-1518).
    ///
    /// **Only 2xx is `true`.** A 403 is not "probably an object": AWS answers 403 rather than 404 for a
    /// key that does not exist when the caller lacks `s3:ListBucket`, which is precisely the credential
    /// this path exists for, so treating 403 as proof would turn "I may not be told" into "delete it".
    /// A transport failure is an `Err` and never a `true` either.
    ///
    /// **What it does not prove**, stated because [`FileSystemProvider::delete`]'s doc argues against
    /// HEAD-before-DELETE and that argument still stands for what it was about: this cannot prove a
    /// deletion *happened*, and it is racy against a concurrent writer — the object can appear or vanish
    /// between the HEAD and the DELETE. It is used only to answer *"does this key name an object?"*, which
    /// is a question with a trustworthy answer, never *"did the delete remove something?"*.
    fn head_proves_object(&self, key: &str) -> Result<bool, String> {
        let target = self.config.object_target(key)?;
        let (url, req) = self.signed_request(
            "HEAD",
            &target,
            &[],
            sigv4::EMPTY_PAYLOAD_SHA256,
            // A metadata exchange, not a transfer: bounded end to end, exactly like `stat`'s HEAD.
            Some(self.request_deadline),
        )?;
        let (status, _resp) = Self::send_capturing_status(req, None, &url)?;
        // CPE-1749 re-check: deliberately still any `2xx`, not narrowed to `200`. Unlike `stat`'s HEAD,
        // this one reads no document at all — the response body is discarded (`_resp`) and no header off
        // it is trusted as a measurement, only the fact that *some* 2xx came back for this exact key. A
        // 206 answering a HEAD carries no body to be a fragment of and no `Content-Length` this function
        // reads, so there is no completeness question here to get wrong.
        Ok((200..300).contains(&status))
    }
}

impl S3Provider {
    /// `ListObjectsV2` with `delimiter=/`, paginated to completion via `continuation-token`
    /// (CPE-1683 AC2), reporting how many entries were filtered by [`is_safe_s3_leaf`] alongside the
    /// entries themselves. See this module's top doc for the marker-filtering and traversal guards
    /// [`parse_list_bucket_result`] applies to every entry before it is returned.
    ///
    /// # CPE-1704: a key [`is_safe_s3_leaf`] refuses is reported, not dropped silently
    ///
    /// **Round 1** of this fix returned `Result<Vec<ProviderEntry>, String>` unchanged and simply dropped
    /// refused leaves, silently — the ticket's own bug, just narrowed. **Round 2** appended a synthetic,
    /// non-`is_dir` `ProviderEntry` to the `Vec` naming the count. Review found that worse than the drop
    /// it replaced: a REAL object can be named exactly the marker's own text (nothing stops it — it is
    /// just a string), so the only "N filtered" row a user could ever trust-see was, in fact, the one an
    /// attacker planted; the marker's `is_dir: false, size: 0` claimed to be a real zero-byte file; an S3
    /// `DELETE` of a nonexistent key returns `204`, so "deleting" the marker would silently report success
    /// while the real files stayed hidden; and it landed in `out` after the [`MAX_LIST_ENTRIES`] check, so
    /// it could push a listing one over the cap unaccounted-for. A fabricated row is not "not silent" — it
    /// is a second bug wearing the first one's fix as a costume.
    ///
    /// **This version (round 3)** returns the count as a real, honest `usize` field, never mixed into the
    /// entries themselves — nothing can spoof it, because nothing a server sends ever reaches this field;
    /// it is computed here, in this process, from what this function itself dropped.
    /// [`FileSystemProvider::list`] (the trait's required method, still returning plain
    /// `Vec<ProviderEntry>` for every provider that doesn't need more) is now a thin wrapper over this
    /// method that discards the count, kept only so existing/other callers of the trait's `list` don't
    /// need to change. `crates/vfs::connect::remote_dir_entries` is the one caller that actually reads the
    /// count today (CPE-1704 round 3, see that function's doc); this crate's own `stat`/`read`/`delete`
    /// are still CPE-1684 stubs, but whoever wires them up must not accept the marker text — there is none
    /// any more — or ANY provider-supplied name as a literal key without going through the same
    /// [`is_safe_s3_leaf`] check `list` already applies, since a filtered leaf was never a real, addressable
    /// key to begin with.
    ///
    /// Recorded decision on `a/../b.txt` (CPE-1704's second named case): the deeper leaf (literally `..`
    /// once the `a/` prefix is stripped) is still, correctly, refused — it cannot be surfaced as itself
    /// without misrepresenting the virtual-directory structure. It is **not reachable** under an escaped
    /// display name in this ticket: `ProviderEntry` carries only a display `name`, no separate "real key" a
    /// later `stat`/`read` could resolve back from an escaped form, and inventing that round-trip is
    /// CPE-1684's (stat/read/write) concern. It is **visibly explained**: the directory's `filtered_count`
    /// is non-zero instead of the listing reading as an ordinary, indistinguishable-from-real empty folder.
    pub fn list_with_filtered_count(&self, path: &str) -> Result<(Vec<ProviderEntry>, usize), String> {
        let key_prefix = provider_path_to_key_prefix(path);
        // CPE-1721 (folded in from CPE-1723 item 2): close the query-string/path asymmetry.
        //
        // `list` puts the prefix in the QUERY STRING, which `ureq`/`url` does not normalise, so
        // `list("/a/../b")` used to succeed and hand back a folder that opened and browsed normally.
        // `stat`/`read`/`write` put the key in the PATH, which *is* normalised, so every single file
        // inside that folder was then refused by `guard_path_survives_the_client`. A browsable folder in
        // which nothing is openable is a worse answer than a refusal that names the cause, so the same
        // guard is applied here, to the object target a child of this prefix would be fetched under.
        //
        // Checked against the real object target rather than by re-scanning the raw key, so it is the
        // one guard making the decision in both places: `encode_path` escapes a literal `%` on the way
        // in, so a raw key segment of `%2e` becomes `%252e` and is correctly NOT a dot segment, while a
        // bare `.`/`..` stays one. A hand-rolled second check here would have to re-derive that and
        // would eventually re-derive it wrong.
        if !key_prefix.is_empty() {
            guard_path_survives_the_client(&self.config.object_target(&key_prefix)?.encoded_path)
                .map_err(|e| {
                    format!(
                        "s3: list {path:?}: this prefix would list children that this crate then could \
                         not open. {e}"
                    )
                })?;
        }
        let target = self.config.bucket_target()?;

        let mut out = Vec::new();
        let mut filtered_total = 0usize;
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
            // Bounds how many MORE pages will be started — the compounding a per-request deadline cannot
            // see. It is checked here, between pages, and therefore CANNOT fire while a body is in
            // flight; that is a real limitation of this check, not a design nicety, and an earlier
            // version of this code relied on it as if it were a whole-listing bound. It is not. The
            // in-flight page is bounded by `TIMEOUT_LIST_REQUEST` on the request itself, and only the two
            // together bound a `list` call — worst case `MAX_LIST_WALL_CLOCK + TIMEOUT_LIST_REQUEST`,
            // since a final page may start just under the budget and then take its own full deadline.
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

            // `Some(..)`: a ListObjectsV2 page is small and already byte-capped, so an end-to-end deadline
            // is exactly right for it. CPE-1684's large-object GET must pass `None` — see
            // `TIMEOUT_LIST_REQUEST`.
            let (status, body) = self
                .signed_get(&target, &query, Some(self.request_deadline))
                .map_err(|(_, m)| m)?;
            // **`200` exactly, not `2xx`** (CPE-1740). This loop used to accept any `2xx`, so a `203`
            // (a transforming proxy — RFC 9110 §15.3.4 — reachable behind a corporate MITM proxy or a
            // CDN) or a `206 Partial Content` reply parsed cleanly and rendered as the folder's complete
            // contents: measured at the PR #903 round-5 UAT as `list under a 203 = Ok(["a.jpg"])`. That is
            // the identical hazard `probe_prefix_after`'s belt already refuses (CPE-1727 item 2) and
            // `signed_exchange`'s over-cap guard refuses in its own words, arriving here by status code
            // instead of by byte count or truncation flag. `ListObjectsV2` has exactly one success status.
            if status != 200 {
                // CPE-1683 AC6: every non-2xx response goes through the one shared error path, never an
                // ad-hoc string built here. CPE-1727 item 3 wraps — not replaces — that shared diagnosis,
                // so the operation, the path and (on a denial) `s3:ListBucket` are named while everything
                // `map_s3_error` read out of the body is still there verbatim. See `list_failure`.
                //
                // A non-canonical 2xx is the one exception: its body is not an S3 error to be parsed (and
                // may well be a genuine listing), so it gets `non_canonical_listing_status_cause` instead
                // of being routed through `map_s3_error`, which would hunt it for an `<Error>`'s `<Code>`
                // and report "no <Code> element" about a body that was never an error.
                let cause = if is_non_canonical_listing_status(status) {
                    non_canonical_listing_status_cause(status)
                } else {
                    error::map_s3_error(status, &body)
                };
                return Err(list_failure(path, &key_prefix, status, &cause));
            }

            let text = std::str::from_utf8(&body)
                .map_err(|e| format!("s3: list {path:?}: response body was not valid UTF-8: {e}"))?;
            let page = parse_list_bucket_result(text, &key_prefix)?;
            filtered_total += page.filtered_count;

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

        Ok((out, filtered_total))
    }
}

impl FileSystemProvider for S3Provider {
    /// The trait's required entry point: [`S3Provider::list_with_filtered_count`] with the count
    /// discarded. See that method's doc for the full CPE-1704 history and for the caller
    /// (`crates/vfs::connect::remote_dir_entries`) that uses the count instead of this.
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        Ok(self.list_with_filtered_count(path)?.0)
    }

    /// **This override is load-bearing and its absence is invisible to a normal test.** Added by the
    /// Foreman from the PR #890 round-3 UAT.
    ///
    /// [`S3Provider::list_with_filtered_count`] is an *inherent* method. Rust resolves inherent methods
    /// before trait methods, so every test that calls it on a concrete `S3Provider` gets the real one and
    /// passes — all 122 of this crate's tests did. But `crates/vfs::connect::remote_dir_entries` takes
    /// `&dyn FileSystemProvider`, and a trait object can only reach what the `impl` block declares. Without
    /// this line the vtable resolves to the trait's default, which hardcodes a count of `0`.
    ///
    /// The consequence was that the filtered count — the entire mechanism this ticket added so a refused
    /// key would not vanish *silently* — read `0` through the only path a user takes, no matter how many
    /// keys were dropped. `remote_list_dir_impl`'s `if listing.filtered > 0` could never fire. The
    /// "not silent" fix was itself silent.
    ///
    /// Note what this means for testing, because it is the third time this one ticket has been caught by
    /// it: a test on the concrete type proves nothing about dispatch. `dyn_dispatch_reaches_the_real_...`
    /// below exercises this through a trait object deliberately, and it is the only test that can fail if
    /// this line is deleted.
    fn list_with_filtered_count(&self, path: &str) -> Result<(Vec<ProviderEntry>, usize), String> {
        S3Provider::list_with_filtered_count(self, path)
    }

    /// S3's own leaf-safety rule (CPE-1704): overrides the trait's default (`is_safe_name`, correct for
    /// filesystem-shaped backends) with [`is_safe_s3_leaf`], so `crates/vfs::connect::remote_dir_entries`
    /// — the one path a user's remote listing actually takes — asks the RIGHT question about an S3 leaf
    /// instead of assuming every backend means the same thing by "safe". See [`is_safe_s3_leaf`]'s doc.
    fn is_safe_leaf_name(&self, name: &str) -> bool {
        is_safe_s3_leaf(name)
    }

    /// `HEAD` on the object key, falling back to a prefix probe so a virtual directory is reported as one
    /// rather than as missing (CPE-1684).
    ///
    /// # A missing key and a denied key must stay distinguishable, and HEAD makes that hard
    ///
    /// **HEAD responses carry no body by protocol**, so [`error::map_s3_error`] — which is body-driven and
    /// correct as a parser — would answer *"the body could not be read … refusing to guess"* for every
    /// missing key, i.e. for the single most common failure in the whole provider. [`map_response_error`]
    /// is the rule that fixes it: with no body, the **status and the method** are the evidence. Nothing is
    /// guessed, because an empty body from a HEAD is what the protocol requires rather than a symptom.
    ///
    /// **The 403 arm is not simply "denied", and says so.** AWS answers 403 rather than 404 for a key that
    /// does not exist when the caller lacks `s3:ListBucket`, so a bodiless 403 genuinely means "denied, or
    /// missing and you may not be told which". The message states both instead of picking one; it is still
    /// cleanly distinct from the 404 arm, which is what the criterion asks for.
    ///
    /// # The directory fallback, and why `entries.is_empty()` could not be it
    ///
    /// S3 has no directories, so a 404 on the object key does **not** mean the path is absent — a virtual
    /// directory is just a prefix with keys under it. On a 404 (and only a 404 — a denial is reported as a
    /// denial, never softened into "not found") this probes `key/` via [`S3Provider::probe_prefix`] and
    /// reports a directory if anything at all sits under it. That must count the zero-byte marker object,
    /// which the listing parser correctly filters out of `entries`; otherwise a directory this very module
    /// had just created with `mkdir` would stat as missing. See [`ListPage::raw_entries`].
    ///
    /// # A 200 with no usable `Content-Length` is refused, not reported as zero
    ///
    /// The size is the one thing a HEAD exists to learn. A server that answers 200 and does not supply a
    /// parseable `Content-Length` has not told us, and `size: 0` would be an invented measurement — the
    /// exact class of confident wrong answer this crate is written against. (Note `ureq` reports a
    /// *malformed* response header as absent, so "absent" here covers both; the message says so.)
    fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
        let key = match provider_path_to_object_key(path) {
            Ok(key) => key,
            // The bucket root: not an object, and no HEAD addresses it. Rather than fabricate a success
            // for a path nothing was asked about, prove it — a one-key listing exercises the endpoint,
            // the addressing and the credentials, and fails loudly if any of them is wrong.
            Err(_) => {
                self.probe_prefix("").map_err(|e| {
                    probe_failure(
                        "stat",
                        path,
                        "",
                        "the bucket itself is not an object and no HEAD addresses it, so this proves the \
                         endpoint, the addressing and the credentials by listing the bucket root rather \
                         than fabricating a success for a path nothing was asked about — and that listing \
                         failed.",
                        &e,
                    )
                })?;
                return Ok(ProviderEntry { name: "/".to_string(), is_dir: true, size: 0 });
            }
        };
        // Trim the trailing slash BEFORE taking the last segment. A key ending in `/` is a shape
        // CPE-1722 made reachable for the first time (`/trail.txt//` addresses the key `trail.txt/`),
        // and `rsplit('/')` on it yields `""` — so `stat` reported an empty display name for an object
        // that `list` shows as `trail.txt`, i.e. the two projections this ticket exists to unify
        // disagreeing about the same object. Mirrors `parse_list_bucket_result`'s `<CommonPrefixes>` arm
        // and the sibling precedent in `crates/webdav/src/lib.rs`'s `stat`. For a key that is only
        // slashes the leaf is genuinely empty, which is the same answer the listing gives.
        let name = key.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();

        // CPE-1748 — the bug this ticket exists to close. `path` explicitly addressing a directory
        // (a trailing `/`, CPE-1737's convention) must resolve as the PREFIX, full stop: never fall
        // through to the HEAD-on-object-key logic below, which would happily report `is_dir: false`
        // for `/photos/` whenever an unrelated object named `photos` also exists — exactly the
        // directory-collapses-onto-its-same-named-file collision this ticket is about. The bare path
        // (no trailing slash) is untouched below: it still resolves the OBJECT first, falling back to
        // a directory only when no object exists at that exact key.
        if path_addresses_a_directory(path, &key) {
            let key_prefix = format!("{key}/");
            let (raw_entries, _, _) = self.probe_prefix(&key_prefix).map_err(|e| {
                probe_failure(
                    "stat",
                    path,
                    &key_prefix,
                    "the path explicitly addresses a directory (a trailing '/'), so this checks whether \
                     anything exists under that prefix — and the check failed.",
                    &e,
                )
            })?;
            return if raw_entries > 0 {
                Ok(ProviderEntry { name, is_dir: true, size: 0 })
            } else {
                Err(format!(
                    "s3: stat {path:?}: not found — no object or marker exists under this prefix \
                     (checked as a directory because the path explicitly addresses one with its \
                     trailing '/', even though an object of the same bare name may exist)"
                ))
            };
        }

        let target = self.config.object_target(&key)?;
        let (url, req) = self.signed_request(
            "HEAD",
            &target,
            &[],
            sigv4::EMPTY_PAYLOAD_SHA256,
            // A metadata exchange, not a transfer: bounded end to end.
            Some(self.request_deadline),
        )?;
        let (status, resp) = Self::send_capturing_status(req, None, &url)?;
        // **`200` exactly, not `2xx`** (CPE-1749). A `206` here answers with a RANGE's `Content-Length`,
        // not the object's — this client sends no `Range` header on a HEAD, so an unsolicited 206's
        // length is provably not the size being asked for. See `is_non_canonical_object_status` and
        // `non_canonical_stat_status_cause` for the full reasoning and the sibling `read` fix.
        if status == 200 {
            let size = resp
                .header("Content-Length")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .ok_or_else(|| {
                    format!(
                        "s3: stat {path:?}: the server answered HTTP {status} but sent no usable \
                         Content-Length (absent, malformed, or not a number), so the object's size is \
                         unknown — refusing to report 0 as if it had been measured"
                    )
                })?;
            return Ok(ProviderEntry { name, is_dir: false, size });
        }
        // A HEAD response has no body to read; this is empty by protocol, which is exactly what routes
        // `map_response_error` to its status+method rules.
        let (body, _) = read_body_capped(resp.into_reader()).unwrap_or_default();
        if status == 404 {
            let key_prefix = format!("{key}/");
            let (raw_entries, _, _) = self.probe_prefix(&key_prefix).map_err(|e| {
                probe_failure(
                    "stat",
                    path,
                    &key_prefix,
                    "there is no object at this key, so the remaining question is whether the path is a \
                     virtual directory instead — a prefix with keys under it — and the check that answers \
                     it failed, leaving this unable to say whether the path exists at all.",
                    &e,
                )
            })?;
            if raw_entries > 0 {
                return Ok(ProviderEntry { name, is_dir: true, size: 0 });
            }
        }
        // A non-canonical 2xx (203/206) is diagnosed by `non_canonical_stat_status_cause`, not
        // `map_response_error`: a HEAD carries no body by protocol either way, but the cause is not "no
        // S3 error code to read" — it is that Content-Length under this status cannot be trusted as the
        // object's size, which is a different, more specific claim.
        if is_non_canonical_object_status(status) {
            return Err(format!("s3: stat {path:?}: {}", non_canonical_stat_status_cause(status)));
        }
        Err(map_response_error("HEAD", status, &body, path))
    }

    /// `GET` the object, read in fixed [`READ_CHUNK_BYTES`] chunks, capped at `read_cap` (CPE-1684).
    ///
    /// # What "bounded" honestly means here — the ticket's wording is not achievable as written
    ///
    /// The acceptance criterion says *"`read` of a large object never holds the whole body in memory at
    /// once"*. **It cannot**, and no implementation of this trait method could: `FileSystemProvider::read`
    /// returns `Vec<u8>`, so by the time it returns, the whole object is in memory by construction. A
    /// genuinely streaming read is a trait-level change across all four remote backends — `cpe-ftp`'s
    /// module doc already names it as the real fix and puts it out of scope, and `cpe-webdav` says the same.
    ///
    /// What IS delivered, and what the tests actually assert:
    ///
    /// - **No unbounded single allocation.** The buffer is a fixed 64 KiB stack array, matching `cpe-ftp`'s
    ///   [`READ_CHUNK_BYTES`] convention exactly. What the server sends changes how many iterations happen,
    ///   never how much is demanded in one call — so a huge declared `Content-Length` cannot itself make
    ///   this ask for a huge allocation.
    /// - **The cap fires DURING the transfer, not after it.** Checked per chunk, before the bytes are
    ///   appended, so an endless or over-size body is refused at `read_cap` + one chunk rather than after
    ///   the process has already buffered everything. That is the difference a `read_to_end` cannot make,
    ///   and it is what the negative-control test measures.
    /// - **An over-cap read is a loud `Err`, never a truncated `Vec`.** `cpe_server::transfer`'s download
    ///   sink writes whatever comes back to disk as the finished file, so a silent truncation here would be
    ///   data loss wearing a success.
    ///
    /// # No end-to-end deadline, deliberately
    ///
    /// This is the call site [`S3Provider::signed_request`]'s `None` exists for. An overall
    /// `ureq::Request::timeout` would kill a legitimately slow multi-minute download of a large object over
    /// a bad link — it bounds elapsed time regardless of progress, and it *replaces* rather than
    /// supplements [`TIMEOUT_READ`]. A per-read stall bound is the right one here: it fires when the server
    /// stops sending, not when the file is big. Memory is bounded separately, by `read_cap` above.
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let key = provider_path_to_object_key(path)?;
        // CPE-1748: `path` explicitly addressing a directory (a trailing '/', CPE-1737's convention)
        // must never silently GET the same-named object instead — a directory cannot be read as a
        // file, and without this check `read("/photos/")` and `read("/photos")` would fetch the exact
        // same bytes whenever an unrelated object named `photos` also exists at the colliding keyspace.
        if path_addresses_a_directory(path, &key) {
            return Err(format!(
                "s3: read {path:?}: this path explicitly addresses a directory (a trailing '/'), and a \
                 directory cannot be read as a file — even though an object of the same bare name may \
                 exist"
            ));
        }
        let target = self.config.object_target(&key)?;
        let (url, req) =
            self.signed_request("GET", &target, &[], sigv4::EMPTY_PAYLOAD_SHA256, None)?;
        let (status, resp) = Self::send_capturing_status(req, None, &url)?;
        // **`200` exactly, not `2xx`** (CPE-1749). This used to accept any `2xx`, so an unsolicited `206`
        // — this client sends no `Range` header on a GetObject — parsed as a normal body and came back
        // `Ok` with whatever fragment the server sent: measured as `READ 206 >>> Ok([72, 65, 76, 70])`
        // against a 4-byte `HALF` body. `cpe_server::transfer`'s download sink writes whatever `read`
        // returns to disk as the finished file, so that was a truncated file reported as a success. See
        // `is_non_canonical_object_status` and `non_canonical_read_status_cause`.
        if status != 200 {
            let (body, _) = read_body_capped(resp.into_reader()).unwrap_or_default();
            let message = if is_non_canonical_object_status(status) {
                // Not `map_response_error`: the body here is not an S3 error to be parsed — it may well
                // be (a fragment of) the real object — and routing it through the S3 error parser would
                // misdiagnose it exactly the way CPE-1740 measured for a listing's 203/206.
                format!("s3: read {path:?}: {}", non_canonical_read_status_cause(status))
            } else {
                map_response_error("GET", status, &body, path)
            };
            return Err(message);
        }

        let mut reader = resp.into_reader();
        let mut out: Vec<u8> = Vec::new();
        let mut chunk = [0u8; READ_CHUNK_BYTES];
        loop {
            let n = reader
                .read(&mut chunk)
                .map_err(|e| format!("s3: read {path:?}: reading the object body failed: {e}"))?;
            if n == 0 {
                break;
            }
            // Checked BEFORE the append, so the cap bounds what is ever held rather than describing what
            // already is. Deleting this check turns `a_read_past_the_cap_is_refused_...` red.
            if out.len() as u64 + n as u64 > self.read_cap {
                return Err(format!(
                    "s3: read {path:?}: the object ran past the {}-byte read cap without finishing — \
                     refusing rather than returning a truncated object as if it were the whole file",
                    self.read_cap
                ));
            }
            out.extend_from_slice(&chunk[..n]);
        }
        Ok(out)
    }

    /// `PUT` the whole body under the object key (CPE-1684).
    ///
    /// **Multipart upload is out of scope by decision, not by omission.** The trait hands over a complete
    /// `&[u8]` that is already in memory, so S3's 5 GB single-`PUT` ceiling is nowhere near the binding
    /// constraint — `read_cap`'s 2 GiB memory backstop and the caller's own RAM bind first. A body past
    /// [`MAX_SINGLE_PUT_BYTES`] is refused here, before the round trip, purely so the ceiling is named
    /// rather than arriving as a server-side rejection.
    ///
    /// No end-to-end deadline, for the same reason [`FileSystemProvider::read`] has none: uploading a large
    /// object over a poor link is legitimately slow, and [`TIMEOUT_WRITE`]'s per-write stall bound is the
    /// one that distinguishes "slow" from "gone".
    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let key = provider_path_to_object_key(path)?;
        if too_big_for_single_put(data.len() as u64) {
            return Err(format!(
                "s3: write {path:?}: {} bytes exceeds the {MAX_SINGLE_PUT_BYTES}-byte ceiling for a \
                 single S3 PUT, and multipart upload is not implemented — refusing here rather than \
                 sending an upload the server will reject",
                data.len()
            ));
        }
        let target = self.config.object_target(&key)?;
        let (status, body) = self
            .signed_exchange("PUT", &target, &[], Some(data), None)
            .map_err(|(_, m)| m)?;
        // CPE-1749 re-check: deliberately still `(200..300)`, not narrowed to `200`. `write` sends a
        // document, it does not read one back — `body` here is S3's response to the PUT (error detail on
        // failure, nothing this method uses on success), never the object's own bytes. There is no
        // completeness question a non-canonical 2xx could get wrong for a caller that consumes nothing
        // from the reply.
        if !(200..300).contains(&status) {
            return Err(map_response_error("PUT", status, &body, path));
        }
        Ok(())
    }

    /// `PUT` a zero-byte object at exactly the key [`provider_path_to_key_prefix`] produces — the marker
    /// convention every S3 client uses to make an empty virtual directory visible (CPE-1684).
    ///
    /// **The key shape is not re-derived here on purpose.** CPE-1683 settled it and made that function
    /// `pub` for this exact call: `/photos/2024` → `photos/2024/`, no leading slash, one trailing slash,
    /// nothing else. `parse_list_bucket_result` filters a `<Contents>` entry whose `<Key>` equals the
    /// requested prefix, so writing the marker at precisely that key is what keeps it from coming back as
    /// a phantom zero-byte *file* inside its own directory. Deriving the same string a second way here —
    /// off by one slash — is exactly how that phantom gets created.
    ///
    /// The bucket root is refused rather than given a marker: it needs none (it always exists), and an
    /// object key of `""` is not a thing S3 has.
    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        let marker_key = provider_path_to_key_prefix(path);
        if marker_key.is_empty() {
            return Err(format!(
                "s3: mkdir {path:?}: that is the bucket root, which always exists and has no marker \
                 object — S3 has no zero-length key to write one under"
            ));
        }
        let target = self.config.object_target(&marker_key)?;
        // A zero-byte marker is a metadata-sized exchange, not a transfer: bounded end to end.
        let (status, body) =
            self.signed_exchange("PUT", &target, &[], Some(&[]), Some(self.request_deadline))
                .map_err(|(_, m)| m)?;
        // CPE-1749 re-check: deliberately still `(200..300)`, same reasoning as `write`'s own note — a
        // zero-byte marker PUT reads nothing back from a non-canonical 2xx that could be wrong.
        if !(200..300).contains(&status) {
            return Err(map_response_error("PUT", status, &body, path));
        }
        Ok(())
    }

    /// `DELETE` exactly **one** key. A virtual directory with content under it is refused, not walked
    /// (CPE-1684).
    ///
    /// # What the user is told, given that S3 answers 204 for a key that never existed
    ///
    /// S3's `DELETE` is idempotent: a missing key returns `204 No Content`, identically to a key that was
    /// really removed. So a 2xx here proves *"that key is absent now"* and **not** *"an object was
    /// deleted"*, and this method claims only the former. It deliberately does **not** add a
    /// `HEAD`-before-`DELETE` *to prove a deletion happened*: that would be an extra round trip which is
    /// racy by construction (the key can appear or vanish between the two calls) and still could not prove
    /// the `DELETE` removed anything — paying for a stronger-sounding claim that is not actually stronger.
    ///
    /// That is about **proving an effect**, and it is unchanged. It says nothing about using a `HEAD` to
    /// ask a different question — *"is this key an object?"* — which has a trustworthy answer and is what
    /// the denied-probe path below uses (CPE-1727 item 1, [`S3Provider::head_proves_object`]).
    ///
    /// That is benign for a single object — a file the user could see, which is already gone, reports
    /// success — but it would be **actively dangerous for a directory**, which is where the ticket's
    /// warning bites: a plain single-key `DELETE` of `photos/2024` returns 204 while every object under
    /// `photos/2024/` stays exactly where it was. "Deleted" would be a flat lie about a whole subtree.
    ///
    /// # So the directory case is decided first, with one request, and refused
    ///
    /// [`S3Provider::probe_prefix`] answers all three cases in one `ListObjectsV2`:
    ///
    /// - **real entries under the prefix** → refused, and the message says why. S3 has no recursive or
    ///   atomic multi-key delete; a per-key loop that fails halfway leaves part of the tree gone while
    ///   reporting success, which is the same class of confident-wrong-answer as a copy+delete pretending
    ///   to be a rename. Recursive delete is out of scope for v1 for that reason, exactly as the ticket
    ///   offers as one of its two options.
    /// - **only the zero-byte marker** (an empty directory, the shape `mkdir` writes) → that *is* a single
    ///   key, so it is deleted honestly, at the marker key `key/`.
    /// - **nothing under the prefix** → an ordinary object; delete the key itself.
    ///
    /// The probe costs one extra request per delete. That is the price of never reporting a subtree
    /// deleted that is still entirely there.
    ///
    /// # The probe needs `s3:ListBucket`; a HEAD answers the narrower question without it
    ///
    /// A credential with `s3:DeleteObject` but not `s3:ListBucket` — ordinary on MinIO and Ceph — cannot
    /// run that probe. CPE-1723 refused the delete outright at that point, and refusing the *un-probed
    /// single-key DELETE* was right: it would report `/photos/2024` deleted while every object under it
    /// stayed put. But that left a credential holding `s3:GetObject` + `s3:DeleteObject` unable to delete
    /// **anything at all**, including the ordinary object it is plainly entitled to remove.
    ///
    /// **CPE-1727 item 1** restores it without weakening the guard, using the third option the CPE-1723
    /// UAT found: on a failed probe, delete only when a `HEAD` on the key itself answers 2xx — a question
    /// `s3:GetObject` permits, and one a virtual directory can never answer 200 to, because a pure prefix
    /// has no object at its own key. See [`S3Provider::head_proves_object`]. The DELETE that follows is
    /// always for the **exact key that HEADed 200**, never the `key/` marker form, so it removes precisely
    /// the object whose existence was proved. Measured on a fixture serving a bodiless 403 to every
    /// `ListObjectsV2`:
    ///
    /// - virtual directory with content → `Err`, and both objects still on disk;
    /// - a real object → `Ok(())`, and the object gone from disk;
    /// - a key that does not exist → `Err`.
    ///
    /// **The one case where "HEAD says object" and "the prefix has content" are both true** is an object
    /// and a prefix sharing a name (`photos` the object, `photos/…` the objects). Measured in
    /// `tests::an_object_and_a_prefix_sharing_a_name_deletes_only_the_object_the_head_proved`: the object
    /// `photos` is deleted and `photos/a.jpg` and `photos/b.jpg` are untouched. That is the honest outcome
    /// for a client that may not list — it removes exactly the key it proved — but it is worth being
    /// plain that a user who believed they were deleting the *folder* gets a success and a folder that is
    /// still there. Nothing available to a credential that cannot list can distinguish those two
    /// intentions, and the alternative (refusing) is the CPE-1723 behaviour this ticket exists to undo.
    ///
    /// # Two asymmetries between credentials, written down because they are surprising
    ///
    /// The PR #903 UAT built the mirror of the collision case above and found the divergence in (1). Both
    /// are recorded rather than designed away, and both are `delete`-only.
    ///
    /// 1. **On a name collision, the MORE privileged credential loses.** Over the keyspace
    ///    `["photos", "photos/a.jpg", "photos/b.jpg"]`, a credential **with** `s3:ListBucket` runs the
    ///    probe, sees real entries under `photos/`, and refuses — so the object `photos` is **permanently
    ///    undeletable** by it through this client. A credential **without** `s3:ListBucket` HEADs, gets a
    ///    200, and deletes that object successfully. Granting a permission therefore makes an operation
    ///    impossible. `tests::the_credential_that_can_list_is_the_one_that_cannot_delete_the_colliding_object`
    ///    pins both halves. On `origin/main` both were refused, so CPE-1727 creates this divergence.
    ///
    ///    Deliberately not "fixed" by extending the HEAD proof to the privileged path — but **not for the
    ///    reason given in round 2 of this PR, which the UAT measured and disproved.** That reason was that
    ///    the listing renders `photos` as a folder, so deleting the object of the same name would report
    ///    the clicked row deleted while the folder stands. The listing actually renders `photos`
    ///    **twice**, a file row and a folder row, which is what real S3 returns and what
    ///    `tests::a_name_collision_lists_as_two_rows_so_the_user_has_already_said_which_one_they_meant`
    ///    now pins. So a user who clicks the `is_dir: false` row **has** said which one they meant,
    ///    unambiguously; refusing that delete because a different row shares its name is simply a wrong
    ///    answer, with no relocation about it. The relocated-wrong-answer argument holds only for the
    ///    *folder* row.
    ///
    ///    The real obstacle is narrower and structural: the distinguishing bit exists at the **row**, and
    ///    `delete(path)` never receives it — `ProviderEntry` carries a display `name` and an `is_dir`, and
    ///    the trait's `delete` takes a path string that is identical for both rows. Nothing inside this
    ///    function can recover which row was clicked, and inventing an answer is what this crate refuses to
    ///    do. Carrying that bit through is a `FileSystemProvider`-shaped change touching every backend, so
    ///    it is filed (CPE-1735) rather than guessed at here.
    ///
    /// 2. **"Nothing was there" is `Ok` for one credential and `Err` for the other.** With `s3:ListBucket`,
    ///    deleting a key that does not exist probes clean, DELETEs, gets S3's idempotent 204, and returns
    ///    `Ok` — long-standing behaviour, and the reason this doc says a 2xx means *"that key is absent
    ///    now"*. Without it, the HEAD answers 404, nothing proves object-ness, and the delete is refused.
    ///    The HEAD path is therefore **stricter** about the identical situation;
    ///    `tests::deleting_a_key_that_does_not_exist_is_ok_with_listbucket_and_refused_without_it` pins it.
    ///    Defensible — the un-listable credential genuinely cannot tell "absent" from "a directory I may
    ///    not enumerate" — but it means the same call on the same bucket answers differently depending on
    ///    the policy attached to the credential, and a caller that treats `delete` as idempotent must know
    ///    that.
    ///
    /// When the HEAD does not prove an object, the delete is **refused, not guessed**, and the message
    /// names the probe as what failed rather than surfacing a bare denial about a prefix the caller never
    /// typed (CPE-1723 item 1). [`probe_failure`] holds the full reasoning.
    ///
    /// # The `start-after` third belt, on the marker-only verdict only (CPE-1727 item 2)
    ///
    /// When the probe says *"only the marker is here"* — the one verdict on which this method deletes
    /// something that could have a subtree under it — it is re-asked with `start-after` set to the marker
    /// key: *is there anything past the marker?* A conforming server answers "no" for a genuinely empty
    /// directory, so no honest case pays anything but the request. A server that under-filled the first
    /// page and denied being truncated has to escalate to a strictly stronger lie — zero keys with
    /// `IsTruncated=false` while keys exist beyond the marker, a flat protocol violation — to keep the
    /// delete going. See [`S3Provider::probe_prefix`] for why that is a different question rather than the
    /// same one re-asked, and for the hole that remains (a server that ignores `start-after`).
    fn delete(&mut self, path: &str) -> Result<(), String> {
        let key = provider_path_to_object_key(path)?;
        // Checked BEFORE the probe, unlike the other ops, which reach `signed_request`'s copy of this
        // guard on their first request. `delete`'s first request is the prefix probe, whose path is the
        // bucket root (the key travels as a query parameter, which is not normalised) — so the probe would
        // sail through and only the DELETE itself would be refused, after a round trip spent deciding the
        // shape of something that can never be deleted through this client anyway. See
        // `guard_path_survives_the_client`.
        guard_path_survives_the_client(&self.config.object_target(&key)?.encoded_path)?;
        let key_prefix = format!("{key}/");
        // CPE-1723 item 1: the probe's failure is reported as the PROBE's, never as a bare access-denied
        // about a prefix the caller never typed. See `probe_failure` for why this names the check rather
        // than falling back to an un-probed single-key DELETE.
        let probed = self.probe_prefix(&key_prefix);
        let (raw_entries, real_entries, more_pages) = match probed {
            Ok(v) => v,
            Err(probe_error) => {
                // CPE-1727 item 1: the probe is not the only question that can establish object-ness, and
                // the other one needs no `s3:ListBucket`. A 2xx HEAD on the key itself proves this key
                // names an object, which a pure prefix can never do — so the DELETE below removes exactly
                // one object and cannot orphan a subtree. Anything else (404, 403, a transport failure)
                // leaves the question unanswered, and an unanswered question is refused, not guessed.
                //
                // CPE-1748: that HEAD-proves-object fallback is only sound when `path` left it OPEN which
                // of the two colliding rows was meant. When `path` explicitly addresses the DIRECTORY (a
                // trailing '/', CPE-1737's convention) the caller has already said which one they meant —
                // and it was not the object — so a probe failure here must stay a refusal, never fall
                // through to silently deleting the same-named FILE instead. Without this guard, an
                // explicit directory delete whose ListBucket probe happens to fail (e.g. a permissions
                // change, a transient 5xx) would delete an unrelated object the user never selected.
                if !path_addresses_a_directory(path, &key) && self.head_proves_object(&key).unwrap_or(false) {
                    let target = self.config.object_target(&key)?;
                    let (status, body) = self
                        .signed_exchange("DELETE", &target, &[], None, Some(self.request_deadline))
                        .map_err(|(_, m)| m)?;
                    // CPE-1749 re-check: deliberately still `(200..300)`, same reasoning as `write`'s own
                    // note — a DELETE reads no document back whose completeness matters.
                    if !(200..300).contains(&status) {
                        return Err(map_response_error("DELETE", status, &body, path));
                    }
                    return Ok(());
                }
                return Err(probe_failure(
                    "delete",
                    path,
                    &key_prefix,
                    "nothing has been deleted. S3 has no directories, and a single-key DELETE answers 204 \
                     for a prefix just as readily as for an object, so before removing anything this has \
                     to establish whether the path is one object or a virtual directory whose contents \
                     such a delete would silently leave behind — and that check failed. A HEAD on the key \
                     itself was then tried as the one question about object-ness that needs no \
                     s3:ListBucket, and it did not answer 2xx either, so nothing here says this key names \
                     an object.",
                    &probe_error,
                ));
            }
        };
        // `more_pages`: a marker-only page that is ALSO truncated means the server under-filled a page it
        // was allowed to under-fill and there is more beneath the prefix — treat it as content, never as
        // an empty directory. See `probe_prefix`.
        if real_entries > 0 || (raw_entries > 0 && more_pages) {
            return Err(directory_with_content_refusal(path));
        }
        // CPE-1727 item 2, the third belt. `raw_entries > 0` here is the marker-only verdict: the ONE
        // verdict on which this method deletes a key that could have a subtree under it. Re-ask with
        // `start-after` set to the marker — a different question, not a retry (see `probe_prefix`). Only
        // `beyond_entries` (`entries.len() + filtered_count`) and the truncation flag are read; counting
        // `raw_entries` here would take a re-returned marker as content and red every honest
        // empty-directory delete, which is why an earlier review concluded no belt was possible.
        //
        // # Why the gate is `raw_entries > 0` and not "always", written down because the PR #903 UAT had
        // to derive it
        //
        // The same doubly-lying server one row lighter — under-filling to ZERO rows, denying truncation,
        // with `photos/a.jpg` really there and no marker — reaches the `raw_entries == 0` verdict, so the
        // belt never runs and the delete goes through. Measured by
        // `tests::a_zero_row_under_filler_reaches_the_object_verdict_where_the_belt_does_not_run`. That
        // hole is milder by construction (the DELETE goes to `photos`, not `photos/`, so no key that
        // exists is destroyed — S3 answers 204 for a key that was never there) and it predates this belt.
        //
        // The cost of closing it is the reason it stays open: the `raw_entries == 0` verdict is the
        // **ordinary object delete**, by far the common case, so extending the belt there would add a
        // second `ListObjectsV2` to every single-file delete a user ever performs — a permanent tax on the
        // hot path to defend against a server telling two lies at once, in the one shape where its lie
        // cannot cost a key. The marker-only verdict is different: it is rare, and it is the only verdict
        // on which a DELETE targets a key that could have a subtree under it.
        if raw_entries > 0 {
            let (_, beyond_entries, beyond_more) = self
                .probe_prefix_after(&key_prefix, Some(&key_prefix))
                // NOT `probe_failure`: its permission diagnosis is provably false here, because the
                // identical listing succeeded forty lines ago with the same credential. But the opposite
                // claim is not free either — the two listings are not atomic, so a 403 here is a real
                // "the authority changed under us". The status decides which. See
                // `marker_confirmation_failure` (PR #903 review finding 1, UAT round 3).
                .map_err(|refusal| marker_confirmation_failure(path, &key_prefix, &refusal))?;
            // `beyond_more` is the truncation half: a belt page with no visible rows but `IsTruncated=true`
            // still says there is something past the marker. Exercised by
            // `tests::a_belt_page_with_no_rows_but_is_truncated_still_refuses_the_delete`.
            if beyond_entries > 0 || beyond_more {
                return Err(directory_with_content_refusal(path));
            }
        }
        // `raw_entries > 0` with no real entries means the prefix holds only its own zero-byte marker —
        // an empty directory, which really is one key and really can be deleted.
        let doomed_key = if raw_entries > 0 { format!("{key}/") } else { key };
        let target = self.config.object_target(&doomed_key)?;
        let (status, body) =
            self.signed_exchange("DELETE", &target, &[], None, Some(self.request_deadline))
                .map_err(|(_, m)| m)?;
        // CPE-1749 re-check: deliberately still `(200..300)`, same reasoning as `write`'s own note — a
        // DELETE reads no document back whose completeness matters.
        if !(200..300).contains(&status) {
            return Err(map_response_error("DELETE", status, &body, path));
        }
        Ok(())
    }

    /// **Refused. S3 has no rename, and faking one would be a confident wrong answer** (CPE-1684 — this is
    /// the decision the ticket exists to make, and `ProviderCapabilities::supports_rename = false` is how a
    /// caller sees it coming rather than discovering it here).
    ///
    /// The tempting implementation is `CopyObject` then `DeleteObject`. Every part of it is wrong for a
    /// thing presented to the user as a rename:
    ///
    /// - **Not atomic.** Two independent requests with no transaction between them. If the delete fails
    ///   after the copy succeeds, the user now has two copies and believes they have one. If the copy
    ///   half-fails on a large object, they may have neither.
    /// - **O(size), not O(1).** Renaming a 40 GB object copies 40 GB, server-side but not free — and it can
    ///   time out, in which case see the previous point.
    /// - **It silently rewrites the object.** Storage class, server-side-encryption settings and
    ///   user metadata do not survive a naive `CopyObject` unchanged; a "rename" that quietly moves an
    ///   object from Glacier to Standard, or drops its metadata, has done something the user never asked
    ///   for and was never told about.
    ///
    /// No request of any kind is issued: this returns before touching the network, and a test asserts the
    /// fixture's request counter is still `0` afterwards.
    ///
    /// **What that counter is actually for**, stated precisely because an earlier description of this got
    /// it wrong: a *working* copy-then-delete is caught first and more simply by `expect_err`, since it
    /// returns `Ok`. The counter is load-bearing for the **dangerous** variant — an emulation whose copy
    /// lands and which then returns an honest-looking `Err` (the delete failed, or a later step did).
    /// `expect_err` is satisfied by that one, and the user is left with two objects believing they have
    /// one, which is the precise failure this refusal exists to prevent. Only "zero requests were sent"
    /// distinguishes "refused" from "half-did it and then reported a problem".
    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        Err(format!(
            "s3: rename {from:?} -> {to:?} is not supported: S3 has no rename operation and no atomic \
             multi-object operation to build one from. The usual emulation — copy the object to the new \
             key, then delete the old one — is refused here deliberately: it is not atomic (a delete that \
             fails after the copy succeeds silently leaves two copies), it costs time proportional to the \
             object's size rather than being instant, and it rewrites storage class and metadata behind \
             the user's back. capabilities().supports_rename is false so this can be seen before it is \
             attempted. Copy the object to the new key and delete the old one explicitly if that is really \
             what you want."
        ))
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
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

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
    ///
    /// `start_after` is real S3's `start-after` (CPE-1727 item 2): only keys **strictly greater** than it
    /// are returned. It is honoured here rather than ignored because the `delete` belt that uses it would
    /// otherwise be measured against a fixture that answers identically with and without the parameter —
    /// i.e. against nothing at all.
    fn list_page_xml(
        root: &Path,
        prefix: &str,
        start_at: usize,
        max_keys: usize,
        start_after: Option<&str>,
    ) -> String {
        let dir = if prefix.is_empty() { root.to_path_buf() } else { root.join(prefix.trim_end_matches('/')) };
        let mut rows: Vec<(String, String)> = Vec::new();
        let has_marker = dir.join(".s3marker").is_file();
        if has_marker {
            rows.push((
                prefix.to_string(),
                format!("<Contents><Key>{prefix}</Key><Size>0</Size></Contents>"),
            ));
        }
        // A sentinel file `.s3unsafe` makes the page additionally emit a `<Contents>` whose leaf is a
        // **perfectly legal S3 key that `is_safe_s3_leaf` refuses** — here one containing a backslash. A
        // real filesystem cannot hold such a name (on Windows `\` is a separator), which is exactly why it
        // needs a sentinel: without one, no test can reach the case where a directory's only content is an
        // object that `list` filters out. That is the shape the UAT found `delete` reporting success on.
        // It sorts immediately after the marker, matching S3's lexicographic order for this key.
        if dir.join(".s3unsafe").is_file() {
            rows.push((
                format!("{prefix}holiday\\2024.jpg"),
                format!("<Contents><Key>{prefix}holiday\\2024.jpg</Key><Size>7</Size></Contents>"),
            ));
        }
        let mut names: Vec<(String, bool, u64)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name == ".s3marker" || name == ".s3unsafe" {
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
                rows.push((
                    format!("{key}/"),
                    format!("<CommonPrefixes><Prefix>{key}/</Prefix></CommonPrefixes>"),
                ));
            } else {
                rows.push((
                    key.clone(),
                    format!("<Contents><Key>{key}</Key><Size>{size}</Size></Contents>"),
                ));
            }
        }

        // CPE-1727 item 2: `start-after` returns only keys **strictly greater** than it, in the same
        // lexicographic order the rows are already in. Taught to the fixture before the belt that uses it
        // was written, because a fixture that ignores a parameter answers identically with and without it
        // — and a test against such a fixture measures nothing. Applied to `<CommonPrefixes>` rows on
        // their prefix string too, which is how real S3 orders them.
        let rows: Vec<(String, String)> = match start_after {
            Some(after) => rows.into_iter().filter(|(key, _)| key.as_str() > after).collect(),
            None => rows,
        };
        let rows: Vec<String> = rows.into_iter().map(|(_, xml)| xml).collect();

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
    /// # Two limits of this harness, named by the PR #903 UAT rather than left to be rediscovered
    ///
    /// Both are **pre-existing** and neither is a defect in what it does test; they bound what any test
    /// built on it can claim. Filed as CPE-1736.
    ///
    /// - **Object paths are never percent-decoded here.** `real` is built from the raw request path, so a
    ///   key needing encoding (`my photos/x`, `café/x`, `100%pure`) is served under its *encoded* spelling
    ///   and never round-trips as itself. No test in this crate therefore exercises an encodable object key
    ///   through this fixture — the encoding is covered separately, at the `sigv4`/`RequestTarget` level.
    /// - **[`list_page_xml`] interpolates key text into XML unescaped**, so a key containing `&` or `<`
    ///   produces a document `roxmltree` rejects. That makes those two bytes unreachable end-to-end
    ///   through the listing path, which is why no delete/belt test uses them.
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
            let xml = list_page_xml(root, &prefix, start_at, max_keys, param("start-after"));
            let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
            let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
            return;
        }

        let real = root.join(path.trim_start_matches('/'));
        let leaf = path.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();

        // CPE-1684 sentinel: a key whose LAST segment starts with `deny-` gets a **bodiless 403**, the
        // shape S3 sends for a denied key (and, per AWS's documented behaviour, for a *missing* key when
        // the caller lacks `s3:ListBucket`). Keyed off the last segment so an ordinary key cannot trip it
        // by accident. A `std::fs`-backed fixture has no other way to produce a denial.
        //
        // The other shape `stat` has to handle — a 200 with **no** `Content-Length` — deliberately is NOT
        // a sentinel here: `tiny_http` always emits a `Content-Length` (or chunked encoding) and drops a
        // header it cannot parse as one (`response.rs:266-270`), so this fixture *cannot* produce that
        // response. `spawn_a_server_that_answers_head_200_with_no_content_length` writes the response bytes
        // over a raw socket instead. Trying it here would have produced `Content-Length: 0` and a test that
        // passed while measuring nothing.
        if leaf.starts_with("deny-") {
            let _ = req.respond(tiny_http::Response::empty(403));
            return;
        }

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
                // A real HEAD answers 200 with the object's `Content-Length` and no body. `tiny_http`
                // turns an explicit `Content-Length` header into the response's `data_length`
                // (`response.rs:266-270`) and suppresses the body itself for a HEAD request
                // (`request.rs:449`), so this puts the right bytes on the wire without sending any.
                match std::fs::metadata(&real) {
                    Ok(meta) if meta.is_file() => {
                        let len = tiny_http::Header::from_bytes(
                            &b"Content-Length"[..],
                            meta.len().to_string().as_bytes(),
                        )
                        .unwrap();
                        let _ = req.respond(tiny_http::Response::empty(200).with_header(len));
                    }
                    _ => {
                        let _ = req.respond(tiny_http::Response::empty(404));
                    }
                }
            }
            "PUT" => {
                if path.ends_with('/') {
                    let _ = std::fs::create_dir_all(&real);
                    // Real S3 stores a `mkdir` marker as a genuine zero-byte OBJECT at a key ending in
                    // `/`, and returns it as a `<Contents>` whose `<Key>` equals the prefix when that
                    // prefix is listed. A filesystem cannot hold a file whose name ends in the separator,
                    // so the fixture records the marker with the `.s3marker` sentinel `list_page_xml`
                    // already understands. Without this, a directory `mkdir` had just created would list
                    // with no marker at all — and every existence probe that depends on the marker would
                    // be testing a shape real S3 never produces.
                    let _ = std::fs::write(real.join(".s3marker"), b"");
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
                if path.ends_with('/') {
                    // Deleting the marker object. `remove_dir` only succeeds when the directory is empty,
                    // which mirrors what deleting the marker key alone actually achieves on S3: the
                    // prefix stops existing exactly when nothing else is under it.
                    let _ = std::fs::remove_file(real.join(".s3marker"));
                    let _ = std::fs::remove_dir(&real);
                } else {
                    let _ = std::fs::remove_file(&real);
                }
                // 204 unconditionally, including for a key that was never there — S3's real, idempotent
                // DELETE semantics, and the reason `S3Provider::delete` cannot claim an object existed.
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
    ///
    /// **CPE-1693:** `root` is a [`cpe_server::fsutil::ScratchDir`] guard, not a bare `PathBuf` — it
    /// removes the fixture's numbered subdirectory when the caller's binding goes out of scope (normally
    /// the end of the `#[test]` fn), the `impl Drop` guard the CPE-1693 review prescribed for this exact
    /// spawner. Keep the returned guard bound (`let (base, root, requests) = spawn_s3_fixture();`), not
    /// discarded, for as long as the fixture needs to keep serving.
    fn spawn_s3_fixture_with_page_cap(
        page_cap: Option<usize>,
    ) -> (String, cpe_server::fsutil::ScratchDir, Arc<AtomicUsize>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &root_for_thread, page_cap, &requests_thread);
            }
        });
        (format!("http://{addr}"), root, requests)
    }

    /// A fresh, numbered scratch directory for one fixture, nested under a single per-test-binary-run
    /// parent. Factored out of [`spawn_s3_fixture_with_page_cap`] so a second fixture shape can reuse it
    /// **without adding a second top-level temp directory**.
    ///
    /// **CPE-1693:** returns the numbered subdirectory wrapped in a [`cpe_server::fsutil::ScratchDir`]
    /// guard (via [`cpe_server::fsutil::ScratchDir::adopt`], since this directory is already created with
    /// its own numbered-child naming rather than `scratch_dir`'s `<prefix>-<pid>-<seq>` scheme) so the
    /// caller's cleanup is automatic. The shared `PARENT` directory itself is deliberately left as-is —
    /// one empty (once every numbered child guard has dropped) `cpe-s3-fixtures-*` entry per test-binary
    /// run, not per spawn, same as before this ticket — process-exit-time cleanup of the parent is out of
    /// scope; see the ticket's Work Log.
    fn fixture_root() -> cpe_server::fsutil::ScratchDir {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // `(pid, n)` alone is NOT unique across runs — Windows reuses process ids, so a later run could
        // inherit an earlier one's files if a previous run's directory somehow survived. The sibling
        // `cpe-webdav` fixture was actually bitten by this during CPE-1706; same shape, same fix, applied
        // here before it bites too.
        static PARENT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let parent = PARENT.get_or_init(|| {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            std::env::temp_dir().join(format!("cpe-s3-fixtures-{}-{}", std::process::id(), stamp))
        });
        let root = parent.join(n.to_string());
        std::fs::create_dir_all(&root).unwrap();
        cpe_server::fsutil::ScratchDir::adopt(root)
    }

    /// The common case: no server-enforced page cap (the fixture honours whatever `max-keys` the client
    /// sent, which `S3Provider::list` always sets to 1000).
    fn spawn_s3_fixture() -> (String, cpe_server::fsutil::ScratchDir, Arc<AtomicUsize>) {
        spawn_s3_fixture_with_page_cap(None)
    }

    /// The same fixture, served by a credential that holds every **per-object** permission
    /// (`s3:GetObject`, `s3:PutObject`, `s3:DeleteObject`) but **not `s3:ListBucket`** — an entirely
    /// ordinary MinIO/Ceph policy, and the configuration CPE-1723 item 1 is about.
    ///
    /// Every `ListObjectsV2` gets S3's real denial: **403 with the `AccessDenied` XML body**. A bodiless
    /// 403 would have been the wrong shape and would have quietly tested a different code path — a `GET`
    /// carries a body by protocol, unlike the `HEAD`s the `deny-` sentinel exists for, so this one routes
    /// through [`error::map_s3_error`] exactly as a real gateway's would. GET/HEAD/PUT/DELETE on object
    /// keys are served normally by [`handle`], because that is the whole point: the caller really is
    /// entitled to those.
    fn spawn_s3_fixture_without_listbucket() -> (String, cpe_server::fsutil::ScratchDir, Arc<AtomicUsize>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let query = full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                let is_list =
                    parse_query(&query).iter().any(|(k, v)| k == "list-type" && v == "2");
                if is_list {
                    requests_thread.fetch_add(1, Ordering::Relaxed);
                    let body = "<?xml version=\"1.0\"?><Error><Code>AccessDenied</Code>\
                                <Message>Access Denied.</Message></Error>";
                    let ct = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/xml"[..],
                    )
                    .unwrap();
                    let _ = req.respond(
                        tiny_http::Response::from_string(body).with_header(ct).with_status_code(403),
                    );
                    continue;
                }
                handle(req, &root_for_thread, None, &requests_thread);
            }
        });
        (format!("http://{addr}"), root, requests)
    }

    /// A server backed by a **flat keyspace** rather than a filesystem, denying every `ListObjectsV2` the
    /// way a credential without `s3:ListBucket` is denied (CPE-1727 item 1).
    ///
    /// It exists for the one shape [`handle`]'s `std::fs` backing cannot represent: an **object and a
    /// prefix sharing a name** — the key `photos` alongside the keys `photos/a.jpg` and `photos/b.jpg`.
    /// No filesystem can hold a path that is both a file and a directory, but S3 keys are just strings and
    /// nothing stops it. Returns the live key set so a test can assert on **what the server still holds**
    /// after a delete, rather than on the `Result`.
    ///
    /// `HEAD` answers 200 with a `Content-Length` for a key in the set and 404 otherwise; `DELETE` removes
    /// the key and answers 204 unconditionally, which is S3's real idempotent behaviour.
    fn spawn_a_keyspace_server_without_listbucket(
        initial: &[&str],
    ) -> (String, Arc<Mutex<BTreeSet<String>>>) {
        spawn_a_keyspace_server(initial, false)
    }

    /// The same keyspace server with `ListObjectsV2` **allowed** and answered honestly — the mirror the
    /// PR #903 UAT built, and the only way to measure what a credential holding `s3:ListBucket` does with
    /// the object/prefix name collision (PR #903 UAT finding 1).
    fn spawn_a_keyspace_server_with_listbucket(
        initial: &[&str],
    ) -> (String, Arc<Mutex<BTreeSet<String>>>) {
        spawn_a_keyspace_server(initial, true)
    }

    /// One honest `ListObjectsV2` page over a flat key set: `delimiter=/` semantics, `max-keys` honoured,
    /// `IsTruncated` truthful, `start-after` honoured. A key equal to `prefix` comes back as `<Contents>`
    /// (the directory-marker convention), exactly as real S3 returns it.
    fn keyspace_list_xml(
        keys: &BTreeSet<String>,
        prefix: &str,
        max_keys: usize,
        start_after: Option<&str>,
    ) -> String {
        let mut rows: Vec<(String, String)> = Vec::new();
        let mut seen_prefixes: BTreeSet<String> = BTreeSet::new();
        for key in keys {
            let Some(rest) = key.strip_prefix(prefix) else { continue };
            match rest.split_once('/') {
                Some((dir, _)) => {
                    let common = format!("{prefix}{dir}/");
                    if seen_prefixes.insert(common.clone()) {
                        rows.push((
                            common.clone(),
                            format!("<CommonPrefixes><Prefix>{common}</Prefix></CommonPrefixes>"),
                        ));
                    }
                }
                None => rows.push((
                    key.clone(),
                    format!("<Contents><Key>{key}</Key><Size>4</Size></Contents>"),
                )),
            }
        }
        rows.sort();
        if let Some(after) = start_after {
            rows.retain(|(k, _)| k.as_str() > after);
        }
        let total = rows.len();
        let shown = total.min(max_keys);
        let is_truncated = shown < total;
        format!(
            "<?xml version=\"1.0\"?><ListBucketResult>\
             <IsTruncated>{is_truncated}</IsTruncated>{}</ListBucketResult>",
            rows[..shown].iter().map(|(_, x)| x.as_str()).collect::<Vec<_>>().join("")
        )
    }

    fn spawn_a_keyspace_server(
        initial: &[&str],
        allow_list: bool,
    ) -> (String, Arc<Mutex<BTreeSet<String>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let keys: Arc<Mutex<BTreeSet<String>>> =
            Arc::new(Mutex::new(initial.iter().map(|k| (*k).to_string()).collect()));
        let keys_thread = Arc::clone(&keys);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let method = req.method().to_string().to_uppercase();
                let full = req.url().to_string();
                let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                let params = parse_query(raw_query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if is_list && allow_list {
                    let param = |name: &str| {
                        params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
                    };
                    let xml = keyspace_list_xml(
                        &keys_thread.lock().unwrap(),
                        param("prefix").unwrap_or(""),
                        param("max-keys").and_then(|v| v.parse().ok()).unwrap_or(1000),
                        param("start-after"),
                    );
                    let ct = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/xml"[..],
                    )
                    .unwrap();
                    let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
                    continue;
                }
                if is_list {
                    let body = "<?xml version=\"1.0\"?><Error><Code>AccessDenied</Code>\
                                <Message>Access Denied.</Message></Error>";
                    let ct = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/xml"[..],
                    )
                    .unwrap();
                    let _ = req.respond(
                        tiny_http::Response::from_string(body).with_header(ct).with_status_code(403),
                    );
                    continue;
                }
                let key = percent_decode(
                    raw_path.strip_prefix(&format!("/{TEST_BUCKET}/")).unwrap_or(""),
                );
                match method.as_str() {
                    "HEAD" => {
                        let exists = keys_thread.lock().unwrap().contains(&key);
                        if exists {
                            let len = tiny_http::Header::from_bytes(
                                &b"Content-Length"[..],
                                &b"4"[..],
                            )
                            .unwrap();
                            let _ = req.respond(tiny_http::Response::empty(200).with_header(len));
                        } else {
                            let _ = req.respond(tiny_http::Response::empty(404));
                        }
                    }
                    "DELETE" => {
                        keys_thread.lock().unwrap().remove(&key);
                        let _ = req.respond(tiny_http::Response::empty(204));
                    }
                    // CPE-1748: `read` needs a body, not just HEAD's existence check, to prove which
                    // object a `GET` actually reached over the colliding keyspace. 4 bytes, matching the
                    // `Content-Length: 4` the HEAD arm above already answers for the same key set.
                    "GET" => {
                        let exists = keys_thread.lock().unwrap().contains(&key);
                        if exists {
                            let _ = req.respond(tiny_http::Response::from_string("data"));
                        } else {
                            let _ = req.respond(tiny_http::Response::empty(404));
                        }
                    }
                    _ => {
                        let _ = req.respond(tiny_http::Response::empty(405));
                    }
                }
            }
        });
        (format!("http://{addr}"), keys)
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

        let page1_xml = list_page_xml(&root, "bulk/", 0, 3, None);
        let page1 = parse_list_bucket_result(&page1_xml, "bulk/").unwrap();
        assert!(page1.is_truncated, "the fixture's own first page must be truncated for this proof to mean anything");
        assert_eq!(page1.entries.len(), 3, "stopping after one page — i.e. no continuation loop — only sees 3 of 7");
        assert_ne!(page1.entries.len(), names.len(), "a 1-page read must NOT already see everything");

        // The full 3-page walk (what `S3Provider::list`'s loop actually does) sees all 7.
        let page2_xml = list_page_xml(&root, "bulk/", 3, 3, None);
        let page2 = parse_list_bucket_result(&page2_xml, "bulk/").unwrap();
        let page3_xml = list_page_xml(&root, "bulk/", 6, 3, None);
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
    // CPE-1704: `is_safe_s3_leaf` is a SIBLING of `cpe_server::transfer::is_safe_name`, not a call into
    // it — S3 has no drive letters and no alternate data streams, so `:` must be accepted, while every
    // check that is actually about escaping the listed prefix (embedded/leading separator, a literal `..`
    // segment, a control byte) must still hold. Note the guard reads **raw bytes only** and deliberately
    // does NOT decode percent-escapes: `..%2f` is a legal key that cannot escape, and
    // `keys_that_only_look_like_traversal_once_percent_decoded_are_kept` below asserts exactly that. An
    // earlier version of this comment claimed the percent-encoded form was refused; it never was after
    // CPE-1704, and a comment that overstates a security guard is worse than none. Each shape below is its
    // own test, not folded into one big loop, so disabling any single rule inside `is_safe_s3_leaf` turns
    // a distinct, nameable assertion red — the Evidence Rules requirement this ticket calls out explicitly.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn is_safe_s3_leaf_accepts_a_colon_s3_has_no_ads_concept() {
        // CPE-1704 AC1: the bug this ticket exists to fix. `:` is a Windows drive-letter/NTFS
        // alternate-data-stream hazard for `cpe_server::transfer::is_safe_name` (local/SFTP/WebDAV) and is
        // correctly refused there; it is a completely ordinary byte in an S3 key.
        assert!(is_safe_s3_leaf("colon:name.txt"), "S3 has no drive letters/ADS; ':' is an ordinary key byte");
        assert!(is_safe_s3_leaf("x:y"), "the shared guard's bare-drive-letter rejection doesn't apply to S3");
        assert!(
            is_safe_s3_leaf("..evil.txt"),
            "the shared guard's starts_with('..') ADS hardening is filesystem-only, not an S3 rule"
        );
    }

    #[test]
    fn is_safe_s3_leaf_rejects_exactly_dot_dot() {
        assert!(!is_safe_s3_leaf(".."), "UAT set: a key that is exactly '..'");
    }

    /// The PR #890 reviewer neutralised all six arms of [`is_safe_s3_leaf`] one at a time. Four turned a
    /// distinct test red; these two turned **nothing** red, so they failed Evidence Rule 1 and are pinned
    /// here.
    ///
    /// They were genuinely dead when the only caller was `parse_list_bucket_result`, which `continue`s on
    /// an empty leaf before ever asking the guard. CPE-1704 changed that: `is_safe_leaf_name` is now a
    /// public trait method that `crates/vfs::connect::remote_dir_entries` calls on **every** name, so both
    /// arms are live on that path. An accepted `""` or `"."` there would build a `child_uri` pointing at
    /// the directory being listed — a row that navigates to itself.
    ///
    /// Each arm was disabled on its own (`!leaf.is_empty()` → `true`, then `leaf != "."` → `true`) and
    /// each reds this test alone. Note the arms live in one `&&` chain in `is_safe_s3_leaf`, and this test
    /// calls it directly — not through `parse_list_bucket_result` — so it stays green regardless of what
    /// the parser does with an empty leaf on its own.
    ///
    /// **Correction (CPE-1811):** this used to add that disabling `parse_list_bucket_result`'s separate
    /// `if leaf.is_empty() { continue }` "changes nothing here". CPE-1801 removed the `CommonPrefixes`
    /// loop's copy of that check (see its own comment there), so the only one left is the `Contents` loop's
    /// marker arm at `:1378` — a different thing entirely, not a redundant safety filter: it identifies the
    /// directory's own zero-byte marker object and is `raw_entries`-only, deliberately never counted into
    /// `filtered_count` (see [`ListPage::raw_entries`]). "Changes nothing here" is still true of *this*
    /// test, for the reason above — but disabling that arm is not inert in general: it reds
    /// `a_freshly_created_empty_directory_reports_nothing_filtered_so_no_phantom_hidden_entry_is_shown`
    /// (confirmed by disabling it and observing `filtered == 1` instead of `0`), because the marker leaf
    /// then falls through into `is_safe_s3_leaf("")`'s own refusal and gets counted as filtered instead of
    /// ignored. The old wording read as "no `leaf.is_empty()` check anywhere matters to test coverage",
    /// which was never true of the one that remains.
    #[test]
    fn is_safe_s3_leaf_rejects_the_two_arms_that_no_other_test_covers() {
        assert!(!is_safe_s3_leaf(""), "an empty leaf must never be addressable");
        assert!(!is_safe_s3_leaf("."), "a leaf that is exactly '.' resolves to the listed directory itself");
    }

    #[test]
    fn is_safe_s3_leaf_rejects_a_leading_slash() {
        assert!(!is_safe_s3_leaf("/etc/passwd"), "UAT set: a leading '/'");
    }

    #[test]
    fn is_safe_s3_leaf_rejects_a_backslash_key() {
        assert!(!is_safe_s3_leaf(r"a\b"), "UAT set: a backslash key");
    }

    #[test]
    fn is_safe_s3_leaf_rejects_an_embedded_nul() {
        assert!(!is_safe_s3_leaf("a\0b"), "UAT set: an embedded NUL");
    }

    #[test]
    fn is_safe_s3_leaf_rejects_an_embedded_newline() {
        assert!(!is_safe_s3_leaf("a\nb"), "UAT set: an embedded newline");
        assert!(!is_safe_s3_leaf("a\rb"), "an embedded CR is a control byte too, not just LF");
    }

    #[test]
    fn is_safe_s3_leaf_never_decodes_percent_escapes_literal_percent_text_is_always_visible() {
        // CPE-1704 round 3: a round-2 version of this guard percent-decoded the leaf once and refused it
        // if the DECODED form looked like a traversal shape (`..%2f` -> `../`). That was checked against
        // every place a key/entry name actually travels in this codebase — `provider_path_to_key_prefix`
        // and `leaf_under_prefix` are plain string operations, `crates/vfs` passes `path` straight through
        // with no decode step, nothing in `src/` or `src-tauri/` decodes an entry's `name`, and CPE-1684
        // states the identical design intent explicitly ("S3 must NOT normalise dot segments: key
        // `a/../b.txt` is a real, distinct key") — and found to protect nothing on any real path, while
        // refusing nine classes of ordinary, common real-world keys. `report%2ffinal.txt` is literal text:
        // no raw `/` byte, and nothing ever turns it into one. Refusing it repeated this ticket's own bug.
        for leaf in [
            "report%2ffinal.txt",
            "report%2Ffinal.txt",
            "https%3A%2F%2Fexample.com%2Findex.html", // a URL-keyed archive object
            "city=A%2FB",                             // a Hive/Athena/Glue partition value
            "%2e%2e",
            "%2e",
            "%00",
            "%0a",
            "%5cfoo",
            "..%2f",
            "..%2F",
            "%252e%252e%252f", // double-encoded — inert either way; there is no decode pass at all now
        ] {
            assert!(is_safe_s3_leaf(leaf), "{leaf:?} is literal text with no raw dangerous byte — must be visible");
        }
    }

    #[test]
    fn is_safe_s3_leaf_still_rejects_a_literal_raw_slash_next_to_percent_encoded_text() {
        // `%2e%2e/` has a REAL trailing '/' byte — refused for that raw byte alone, nothing to do with any
        // decoding (there is none). Kept separate from the no-decode test above to make the two mechanisms
        // clearly independent: one raw-byte scan, full stop.
        assert!(!is_safe_s3_leaf("%2e%2e/"), "a literal trailing '/' is unsafe on its own, decode or not");
    }

    #[test]
    fn is_safe_s3_leaf_does_not_reject_an_ordinary_percent_sign() {
        // A bare '%' — including one that isn't part of any valid escape — must not be refused on its own.
        assert!(is_safe_s3_leaf("50% off.txt"));
        assert!(is_safe_s3_leaf("100%.txt"));
        assert!(is_safe_s3_leaf("100% not a valid escape.txt"));
    }

    #[test]
    fn a_page_with_an_unsafe_key_reports_the_filtered_count_not_just_dropping_it() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                    <Contents><Key>photos/cat.jpg</Key><Size>4</Size></Contents>\
                    <Contents><Key>photos/..</Key><Size>1</Size></Contents>\
                    <CommonPrefixes><Prefix>photos/../../etc/</Prefix></CommonPrefixes>\
                    </ListBucketResult>";
        let page = parse_list_bucket_result(xml, "photos/").unwrap();
        assert_eq!(page.entries.len(), 1, "the one safe entry must still come through: {:?}", page.entries);
        assert_eq!(page.entries[0].name, "cat.jpg");
        assert_eq!(page.filtered_count, 2, "both the unsafe Contents entry and the unsafe CommonPrefixes entry must be counted");
    }

    /// **CPE-1723 item 8, measured rather than accepted.** That item asserted that `probe_prefix`'s comment
    /// was over-broad by exactly one case: that "a control byte" could never reach the counting path,
    /// because a raw control character makes the listing non-XML and `roxmltree` rejects the whole
    /// document. That is true of *most* control bytes and **false of three of them**.
    ///
    /// XML 1.0 (§2.2) forbids the C0 range **except** tab (`0x09`), LF (`0x0A`) and CR (`0x0D`), which are
    /// perfectly legal document characters. Rust's `char::is_control` returns `true` for all three. So a
    /// key leaf containing a tab parses cleanly, reaches [`is_safe_s3_leaf`], is refused there, and is
    /// counted into `filtered_count` — exactly the shape item 8 said was unreachable. The reachable-shape
    /// list is therefore **four** entries wide (`\`, exactly `..`, exactly `.`, and a tab or LF -- a CR arrives as LF), not three
    /// and not the original unqualified "a control byte".
    ///
    /// Both halves are asserted here so neither claim can rot: the tab is filtered *and counted*, and a
    /// NUL really does take the whole document down with a parse error rather than being counted.
    #[test]
    fn a_tab_in_a_key_is_a_control_byte_that_really_does_reach_the_filtered_count_unlike_a_nul() {
        let with_tab = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                        <Contents><Key>photos/two\tcolumns.txt</Key><Size>4</Size></Contents>\
                        <Contents><Key>photos/ordinary.txt</Key><Size>4</Size></Contents>\
                        </ListBucketResult>";
        let page = parse_list_bucket_result(with_tab, "photos/").expect(
            "a tab is a legal XML character (XML 1.0 §2.2 exempts 0x09/0x0A/0x0D), so this document \
             parses — the premise that every control byte is stopped by the parser is wrong",
        );
        assert_eq!(page.entries.len(), 1, "only the ordinary key may be surfaced: {:?}", page.entries);
        assert_eq!(
            page.filtered_count, 1,
            "a tab-bearing leaf is refused by is_safe_s3_leaf and must be COUNTED, which is precisely the \
             path CPE-1723 item 8 claimed a control byte could never reach"
        );

        let with_nul = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                        <Contents><Key>photos/nul\u{0}byte.txt</Key><Size>4</Size></Contents>\
                        </ListBucketResult>";
        assert!(
            parse_list_bucket_result(with_nul, "photos/").is_err(),
            "a NUL is NOT a legal XML character, so this half of item 8's premise holds: the document \
             fails loudly instead of being counted"
        );
    }

    #[test]
    fn a_key_containing_a_colon_reaches_the_caller_end_to_end() {
        // Built directly against the XML (not the std::fs-backed fixture used elsewhere in this file)
        // because ':' is an illegal filename character on Windows NTFS — a real on-disk fixture could never
        // represent this exact key on every CI OS, but ListObjectsV2's wire protocol has no such
        // restriction, and this is exactly the shape CPE-1704 fixes.
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                            <Contents><Key>colon:name.txt</Key><Size>4</Size></Contents></ListBucketResult>";
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let provider = S3Provider::connect(&cfg(&base));

        let entries = provider.list("/").expect("list");
        assert_eq!(entries.len(), 1, "the colon key must reach the caller end to end: {entries:?}");
        assert_eq!(entries[0].name, "colon:name.txt");
        assert!(provider.is_safe_leaf_name("colon:name.txt"), "the trait override must agree with list()");
    }

    #[test]
    fn list_with_filtered_count_reports_a_filtered_entry_instead_of_a_silent_phantom_empty_folder() {
        // A bucket can legally contain an object whose key is literally ".." — S3 has no directory-
        // traversal semantics on the wire, so the guard correctly refuses to surface it as itself. CPE-1704
        // round 2 tried making this "not silent" by injecting a synthetic ProviderEntry into the returned
        // Vec; review found that WORSE than the silent drop (spoofable by a same-named real object,
        // dishonest is_dir/size fields, a DELETE of it would report success per S3's 204-on-missing-key
        // semantics, and it landed after the MAX_LIST_ENTRIES check). Round 3: `list()` — the trait's
        // required method — stays a plain, honest `Vec<ProviderEntry>` with nothing fabricated in it (the
        // "a/../b.txt" case DOES still look like an empty folder through `list()` alone); the count is a
        // real `usize`, returned only by `list_with_filtered_count`, that cannot be spoofed by anything a
        // server sends because it is computed here, from what this function itself dropped.
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                            <Contents><Key>..</Key><Size>1</Size></Contents></ListBucketResult>";
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let provider = S3Provider::connect(&cfg(&base));

        // The plain trait method: nothing fabricated, the unsafe key is simply absent.
        let plain = provider.list("/").expect("list");
        assert!(plain.is_empty(), "list() must stay a plain, honest Vec with nothing invented in it: {plain:?}");

        // The richer method the real caller (crates/vfs) uses: the same empty Vec, plus an honest count.
        let (entries, filtered) = provider.list_with_filtered_count("/").expect("list_with_filtered_count");
        assert!(entries.is_empty());
        assert_eq!(filtered, 1, "the one unsafe key must be counted, not just dropped with no trace anywhere");
    }

    /// The test above calls a **concrete** `S3Provider`, and that is exactly why it could not catch the
    /// bug the PR #890 round-3 UAT found: `list_with_filtered_count` is an *inherent* method, Rust
    /// resolves inherent methods before trait methods, so a concrete call reaches the real one and passes
    /// even when the `impl FileSystemProvider` block does not declare it at all.
    ///
    /// The real caller does not hold a concrete type. `crates/vfs::connect::remote_dir_entries` takes
    /// `&dyn FileSystemProvider`, and a trait object can only reach the `impl` block — so without the
    /// override, dispatch fell through to the trait default, which hardcodes `0`. Every filtered key was
    /// counted as zero through the only path a user takes, and the whole "not silent" mechanism this
    /// ticket added was inert.
    ///
    /// **This is the third time in one ticket that a fix was verified at a boundary the real caller does
    /// not use** — first at the provider instead of the vfs layer, then on the concrete type instead of
    /// through dispatch. So this test is deliberately shaped like the caller: it goes through
    /// `&dyn FileSystemProvider` and nothing else. Delete the trait override and only this test reds.
    #[test]
    fn dyn_dispatch_reaches_the_real_filtered_count_not_the_trait_default() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                // Two legally-refusable keys and one ordinary file, so a wrong count is unambiguous:
                // the trait default reports 0, the real implementation reports 2.
                let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                            <Contents><Key>..</Key><Size>1</Size></Contents>\
                            <Contents><Key>bad\\name</Key><Size>1</Size></Contents>\
                            <Contents><Key>ordinary.txt</Key><Size>7</Size></Contents></ListBucketResult>";
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let provider = S3Provider::connect(&cfg(&base));

        // The shape `remote_dir_entries` uses. Binding through the trait object is the entire point of
        // this test — calling `provider.list_with_filtered_count(..)` directly would pass regardless.
        let as_trait: &dyn FileSystemProvider = &provider;

        let (entries, filtered) = as_trait
            .list_with_filtered_count("/")
            .expect("list_with_filtered_count through the trait object");

        assert_eq!(
            entries.len(),
            1,
            "only the ordinary key may survive the guard: {entries:?}"
        );
        assert_eq!(
            filtered, 2,
            "the filtered count must survive dynamic dispatch — a 0 here means the vtable resolved to \
             the trait's default and every refused key is invisible to the caller that matters"
        );

        // `is_safe_leaf_name` is the sibling override on the same impl block; pin it through the same
        // dispatch so a future edit cannot drop one while keeping the other.
        assert!(
            as_trait.is_safe_leaf_name("colon:name.txt"),
            "S3's own leaf rule must survive dynamic dispatch too, or remote_dir_entries falls back to \
             the filesystem rule and a legal key vanishes again"
        );
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
        // shared path (CPE-1682/CPE-1700) rather than a bare "HTTP 403" string built here. CPE-1727 item 3
        // wraps that diagnosis in context; it must not replace or paraphrase it, which is what these two
        // assertions still measure.
        assert!(err.contains("AccessDenied"), "{err}");
        assert!(err.contains("You do not have permission"), "{err}");
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1727 item 3: `list`'s denial was the least actionable message in the provider.
    // ---------------------------------------------------------------------------------------------

    /// The reported defect verbatim: a denied `list` said
    ///
    /// ```text
    /// s3: HTTP 403 AccessDenied: Access Denied. — the credentials are valid but the bucket policy or IAM
    /// policy denies this request
    /// ```
    ///
    /// — no path, no operation, no `s3:ListBucket`. And `list` is **the** operation that always needs that
    /// permission, and the first thing a user hits browsing a bucket. The internal probes had been given
    /// this treatment by CPE-1723; the listing the user actually asked for had not.
    #[test]
    fn a_denied_list_names_the_operation_the_path_and_the_permission_it_needs() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        std::fs::create_dir_all(root.join("photos")).unwrap();
        let provider = S3Provider::connect(&cfg(&base));

        let err = provider
            .list("/photos")
            .expect_err("every ListObjectsV2 is denied for this credential");

        assert!(
            err.starts_with("s3: list \"/photos\""),
            "the message must name the operation and the path the user typed, before anything else: {err}"
        );
        assert!(
            err.contains("s3:ListBucket"),
            "the permission that would fix it is the whole point — without it the user has nothing to \
             act on: {err}"
        );
        assert!(
            err.contains("ListObjectsV2"),
            "naming the request makes the denial searchable against a bucket policy: {err}"
        );
        assert!(
            err.contains("\"photos/\""),
            "the prefix actually requested belongs in the message too, since that is what the policy \
             statement has to allow: {err}"
        );
        assert!(
            err.contains("AccessDenied"),
            "and the server's own diagnosis must survive the wrapper verbatim: {err}"
        );
    }

    /// The other half of the same guard: a listing that failed for a reason that is **not** an entitlement
    /// problem must not advise granting a permission. Telling a user to change their bucket policy when
    /// their gateway is merely returning 500s is a confident wrong answer, and cost-free to avoid — the
    /// status is right there.
    #[test]
    fn a_list_failure_that_is_not_a_denial_still_names_the_path_but_advises_no_permission() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let body = "<?xml version=\"1.0\"?><Error><Code>InternalError</Code>\
                            <Message>We encountered an internal error.</Message></Error>";
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(
                    tiny_http::Response::from_string(body).with_header(ct).with_status_code(500),
                );
            }
        });
        let base = format!("http://{addr}");
        let provider = S3Provider::connect(&cfg(&base));

        let err = provider.list("/photos").expect_err("a 500 must surface as an error");
        assert!(
            err.starts_with("s3: list \"/photos\""),
            "the operation and the path are named for every failure, not just denials: {err}"
        );
        assert!(
            !err.contains("s3:ListBucket"),
            "a server-side error is not an entitlement problem, and advising a policy change here sends \
             the user to fix something that is not broken: {err}"
        );
        assert!(
            err.contains("InternalError"),
            "the server's own diagnosis must still be there: {err}"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1740: `FileSystemProvider::list`'s pagination loop still accepted any `2xx`, so a `203`
    // (RFC 9110 §15.3.4's legitimate transforming-proxy answer) or a `206 Partial Content` reply parsed
    // cleanly and rendered as the folder's COMPLETE contents — measured at the PR #903 round-5 UAT as
    // `list under a 203 = Ok(["a.jpg"])`. `ListObjectsV2` has exactly one success status, `200`, exactly
    // the narrowing `probe_prefix_after`'s belt already got from CPE-1727.
    // ---------------------------------------------------------------------------------------------

    /// Pins AC1: `list` must refuse a `203` and a `206` reply rather than rendering either as the
    /// folder's complete contents, and the message must name the status and say a listing requires
    /// exactly `200`.
    ///
    /// Each status is its own table row so a guard that only catches one of the two cannot pass by
    /// accident: `203` was the status actually measured at round 5, `206` is the sibling `probe_prefix`'s
    /// own guard already covers, and both share the one `status != 200` line this test exists to pin.
    #[test]
    fn list_refuses_a_203_and_a_206_reply_rather_than_rendering_it_as_the_folders_complete_contents() {
        for status in [203u16, 206u16] {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let addr = server.server_addr().to_ip().unwrap();
            std::thread::spawn(move || {
                for req in server.incoming_requests() {
                    // A REAL, well-formed listing — not truncated, not an error — so the only thing that
                    // can make this refused is the status. If the guard were dropped this body parses
                    // cleanly and `list` would answer `Ok(["a.jpg"])`, exactly the round-5 measurement.
                    let xml = "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated>\
                                <Contents><Key>photos/a.jpg</Key><Size>7</Size></Contents>\
                                </ListBucketResult>";
                    let ct =
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..])
                            .unwrap();
                    let _ = req.respond(
                        tiny_http::Response::from_string(xml).with_header(ct).with_status_code(status),
                    );
                }
            });
            let base = format!("http://{addr}");
            let provider = S3Provider::connect(&cfg(&base));

            // Effect before verdict (this sprint's standing rule): capture the raw outcome first, so a
            // guard that regresses to `Ok(["a.jpg"])` reds THIS assertion with the actual entries printed,
            // rather than panicking inside `.expect_err()` before the harm can be named.
            let outcome = provider.list("/photos");
            assert!(
                outcome.is_err(),
                "THE DEFECT: a {status} reply was rendered as the folder's complete contents instead of \
                 being refused — the round-5 measurement was exactly this shape (`Ok([\"a.jpg\"])`): \
                 {outcome:?}"
            );
            let err = outcome.unwrap_err();
            assert!(
                err.contains(&status.to_string()),
                "[{status}] the message must name the status that caused the refusal: {err}"
            );
            assert!(
                err.contains("200"),
                "[{status}] and say that a listing requires exactly 200: {err}"
            );
        }
    }

    /// Pins the message half of AC2, on `list`'s own path rather than the belt's: the not-200 case must
    /// not route a readable listing body through the S3 *error* parser. Before this, `list`'s non-2xx
    /// branch always called `error::map_s3_error`, which hunts a body for an `<Error>`'s `<Code>` element
    /// — and a 203/206 body here is a real listing, never an S3 error, so that call reported "no <Code>
    /// element was found" about a body that has none because it was never an error to begin with.
    #[test]
    fn a_non_canonical_2xx_on_list_is_not_routed_through_the_s3_error_parser() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let xml = "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated>\
                            </ListBucketResult>";
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..])
                    .unwrap();
                let _ = req.respond(
                    tiny_http::Response::from_string(xml).with_header(ct).with_status_code(203),
                );
            }
        });
        let base = format!("http://{addr}");
        let provider = S3Provider::connect(&cfg(&base));

        let err = provider.list("/photos").expect_err("a 203 must be refused, not rendered");
        assert!(
            !err.contains("no <Code> element"),
            "THE DEFECT: a body that is a real listing (never an S3 error) was routed through the S3 \
             error parser, which reported a missing <Code> element about it: {err}"
        );
        assert!(
            !err.contains("could not be read as an S3 error"),
            "and must not claim the body could not be read — it read fine, it just was not what a \
             listing must arrive as: {err}"
        );
    }

    /// Pins AC2 on the belt: the reported defect verbatim was
    ///
    /// ```text
    /// The server answered HTTP 203 — it did reply, and successfully; what failed was reading the reply.
    /// ...
    /// Underlying error: s3: "scratch/": s3: HTTP 203 and the response body could not be read as an S3
    /// error (no <Code> element was found in it); refusing to guess which cause applies
    /// ```
    ///
    /// Both halves are wrong: reading the reply did NOT fail (the reply was fully readable and refused on
    /// its *status*), and the underlying cause hunted a real listing body for an S3 `<Error>`'s `<Code>`
    /// element that was never going to be there. This fixture is `a_two_hundred_the_belt_cannot_read_...`
    /// with the status changed from 200 to 203 and a well-formed listing body instead of garbage — the
    /// message for THIS shape must say the opposite of that test's, because unlike that test's 200 this
    /// reply was never read at all before being refused.
    #[test]
    fn the_belts_203_message_does_not_claim_a_read_failure_or_hunt_a_listing_for_an_error_code() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let query = full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                let params = parse_query(&query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if is_list && params.iter().any(|(k, _)| k == "start-after") {
                    // A REAL, well-formed, empty listing — not garbage, not an S3 error — under a 203.
                    let body = "<?xml version=\"1.0\"?><ListBucketResult>\
                                <IsTruncated>false</IsTruncated></ListBucketResult>";
                    let _ = req.respond(
                        tiny_http::Response::from_string(body).with_status_code(203),
                    );
                    continue;
                }
                handle(req, &root_for_thread, None, &requests_thread);
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.mkdir("/scratch").expect("mkdir must succeed — this server denies nothing");

        // Effect before verdict, same rule as the 206 test beside this one: the DELETE going out on a
        // wrongly-trusted 203 is the actual harm, not merely a wrong string.
        let outcome = provider.delete("/scratch");
        assert!(
            root.join("scratch/.s3marker").is_file(),
            "THE HARM: a 203 on the belt was trusted as an empty listing and the marker was deleted \
             (outcome was {outcome:?})"
        );
        let err = outcome.expect_err("a 203 cannot confirm that a prefix holds nothing past the marker");

        assert!(
            err.contains("203"),
            "the refusal must name the status that caused it: {err}"
        );
        assert!(
            !err.contains("what failed was reading the reply"),
            "THE DEFECT (message half 1): nothing failed reading this reply — it was refused on its \
             status before its body was ever inspected: {err}"
        );
        assert!(
            !err.contains("no <Code> element"),
            "THE DEFECT (message half 2): the body is a real, well-formed listing, never an S3 error, so \
             hunting it for an <Error>'s <Code> element and reporting one missing is the wrong complaint: \
             {err}"
        );
        assert!(
            !err.contains("could not be read as an S3 error"),
            "and must not claim the body could not be read — it was never read at all, because the \
             status alone was enough to refuse it: {err}"
        );
        assert!(
            err.contains("200"),
            "the message must say what a listing actually requires: {err}"
        );
    }

    /// **The blocking finding of PR #911's review of CPE-1740's first attempt.** The "refused on its
    /// status before its body was ever read" arm above was selected on `status` alone — but
    /// `signed_exchange` treats every `2xx` as `ok`, so a `203` whose body is over
    /// [`MAX_RESPONSE_BODY_BYTES`] fails INSIDE `signed_exchange`'s own over-cap guard and never reaches
    /// `probe_prefix_after`'s `status != 200` short-circuit at all. The body WAS read — to the cap — and
    /// reading it is exactly why the reply was refused. The old wording therefore contradicted its own
    /// appended "Underlying error", which named the byte cap one sentence after claiming the body was
    /// never read: the mirror image of the defect CPE-1740 removed on this exact status.
    ///
    /// Same over-cap shape CPE-1706 pins for `list` (a complete, valid root element followed by megabytes
    /// of legal post-root padding, so truncation cannot be inferred from the XML merely failing to
    /// parse) — sent under a `203` instead of a `200`, on the belt instead of the plain listing.
    #[test]
    fn the_belts_203_message_does_not_claim_an_unread_body_when_it_was_read_to_the_cap_and_refused() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            let mut xml = String::from(
                "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated>\
                 </ListBucketResult>",
            );
            xml.push_str(&" ".repeat(MAX_RESPONSE_BODY_BYTES + 1024 * 1024));
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let query = full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                let params = parse_query(&query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if is_list && params.iter().any(|(k, _)| k == "start-after") {
                    let _ = req.respond(
                        tiny_http::Response::from_string(xml.clone()).with_status_code(203),
                    );
                    continue;
                }
                handle(req, &root_for_thread, None, &requests_thread);
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.mkdir("/scratch").expect("mkdir must succeed — this server denies nothing");

        // The over-cap body is megabytes, so give this the same generous, bounded deadline CPE-1706's
        // sibling test for `list` uses rather than the default per-test timeout.
        let outcome = call_with_deadline(
            "S3Provider::delete against an over-cap 203 belt reply",
            Duration::from_secs(60),
            move || provider.delete("/scratch"),
        );

        assert!(
            root.join("scratch/.s3marker").is_file(),
            "THE HARM: an over-cap 203 was trusted as an empty listing and the marker was deleted \
             (outcome was {outcome:?})"
        );
        let err = outcome
            .expect_err("an over-cap reply cannot confirm the prefix holds nothing past the marker");

        assert!(
            err.contains("203"),
            "the refusal must still name the status: {err}"
        );
        assert!(
            err.contains(&format!("{MAX_RESPONSE_BODY_BYTES}-byte cap")),
            "the underlying cause must name the REAL reason — the body was read to the cap and refused, \
             not merely a status check: {err}"
        );
        assert!(
            !err.contains("before its body was ever read") && !err.contains("before its body was ever inspected"),
            "THE DEFECT (PR #911 review): the body WAS read, to the cap, and reading it is exactly why \
             this was refused — claiming it was 'never read'/'never inspected' one sentence before naming \
             the byte cap that stopped the read is self-contradictory: {err}"
        );
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
        let outcome = provider.list("/");

        // Assert the harm BEFORE unwrapping the Result (CPE-1743): if the guard ever answers `Ok`, the
        // run must still reach this and red on what actually happened, not on "expected an error".
        assert_eq!(
            requests.load(Ordering::Relaxed),
            1,
            "must fail on the very first malformed page, not retry or silently accept what it has \
             (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "IsTruncated=true with no NextContinuationToken must be a loud error, not a silently \
             truncated-but-reported-complete listing",
        );
        assert!(err.contains("IsTruncated=true"), "{err}");
        assert!(err.contains("NextContinuationToken"), "{err}");
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

        let outcome = provider.list("/");

        // Assert the harm BEFORE unwrapping the Result (CPE-1743): the harm here IS "no bytes left the
        // process", so this must not be gated behind a successful `expect_err`.
        assert_eq!(
            requests.load(Ordering::Relaxed),
            0,
            "the guard must fire BEFORE any request reaches the fixture — a live server saw a request \
             despite the refusal (outcome was {outcome:?})"
        );

        let err = outcome.expect_err("a non-ASCII access key id must be refused, not silently mangled");
        assert!(err.contains("ureq"), "{err}");
        assert!(err.contains("Authorization"), "the error must name which header, not just that something failed: {err}");
        assert!(err.contains("byte") && err.contains("offset"), "the error must name the byte and its offset: {err}");
        // B4 (independent-review finding on an earlier draft): the value must never be echoed, because
        // this exact call site passes `&signed.authorization`, which for a real request carries the
        // request's SigV4 signature and access key id.
        assert!(!err.contains("Signature="), "the Authorization value leaked into the error: {err}");
        assert!(!err.contains("Credential="), "the Authorization value leaked into the error: {err}");
        assert!(!err.contains("AKIA"), "the access key id leaked into the error: {err}");
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1706 item 1: wall-clock is bounded. Three separate bounds, three separate proofs.
    // ---------------------------------------------------------------------------------------------

    /// A server that completes the accept, sends a **valid `200 OK` header block**, and then emits its
    /// body **one byte at a time with `gap` between bytes, forever** (CPE-1706 round 2).
    ///
    /// This is the shape that defeated the first round of this ticket, and it is worth being precise
    /// about why: a per-*read* timeout's clock restarts on every byte, so a peer that sends one byte
    /// every 29 s never trips a 30 s `timeout_read` — it is not "stalled" by that definition at any
    /// instant, only useless in aggregate. And a between-pages deadline cannot fire while this body is in
    /// flight. Only an end-to-end per-request deadline sees it. The declared `Content-Length` is huge so
    /// the client keeps waiting for more rather than treating the body as finished.
    fn spawn_a_server_that_dribbles_one_byte_at_a_time(gap: Duration) -> String {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                std::thread::spawn(move || {
                    // A complete, valid response head — so the client is past connect and past headers,
                    // and is committed to reading a body it will never finish receiving.
                    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\n\
                                Content-Length: 8388608\r\n\r\n";
                    if s.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    let _ = s.flush();
                    let body = b"<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";
                    let mut i = 0usize;
                    loop {
                        std::thread::sleep(gap);
                        if s.write_all(&[body[i % body.len()]]).is_err() || s.flush().is_err() {
                            return;
                        }
                        i += 1;
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    /// **CPE-1713 (round 3): the wiring check that replaced the round-2 60 s live-fire test.**
    ///
    /// Round 2's `a_server_that_dribbles_one_byte_at_a_time_is_cut_off_at_the_shipped_values` ran the real
    /// [`S3Provider::connect`] — no injected `Duration` anywhere — against a dribbling server and waited
    /// out the full shipped 60 s deadline to prove it. That cost +120 s of CI wall clock per OS × 3 OSes
    /// (≈6 minutes every run, forever), to prove one narrow thing the other two tests in this file don't:
    /// that [`S3Provider::connect`] actually initialises `request_deadline` FROM [`TIMEOUT_LIST_REQUEST`],
    /// rather than from some other constant or a stale default.
    ///
    /// This is a field assertion, not a live-fire: no socket, no sleep, no thread. It reads the field the
    /// dribble guard actually consults, off the exact constructor production calls.
    ///
    /// **The three properties CPE-1706 needs pinned, and which test pins which — removing any one turns a
    /// distinct test red:**
    /// 1. **Mechanism** — a `request_deadline` field actually bounds `list`'s wall clock, driven through
    ///    the real `signed_get` → `req.timeout(..)` → `req.call()` path:
    ///    [`a_dribbling_server_is_cut_off_by_the_per_request_deadline`] (below). Delete the
    ///    `req = req.timeout(deadline)` line in `signed_request` and this reds — the 500 ms deadline no
    ///    longer cuts the call short, so it runs past this test's own 10 s ceiling.
    /// 2. **Value** — [`TIMEOUT_LIST_REQUEST`] itself is a sane, finite bound (not 0, not a year):
    ///    [`the_shipped_timeout_values_are_finite_and_within_sane_bounds`]. Change the constant to
    ///    something absurd and only that test reds.
    /// 3. **Wiring** — `connect()`'s real constructor path actually assigns that constant to the field the
    ///    mechanism above reads: this test,
    ///    [`connect_installs_the_shipped_request_deadline_on_the_field_the_call_site_reads`]. Change
    ///    `connect_with_timeouts` to assign `request_deadline: TIMEOUT_READ` (a different, still-sane
    ///    `Duration`) instead of `TIMEOUT_LIST_REQUEST` and only this test reds — (1) and (2) both still
    ///    pass unchanged, because neither one ever calls the real zero-argument `connect()`.
    ///
    /// Verified by hand for this ticket (not committed, to avoid permanently breaking prod): with
    /// `request_deadline: TIMEOUT_LIST_REQUEST` changed to `request_deadline: TIMEOUT_READ` in
    /// `connect_with_timeouts`, this test reds on the field mismatch while (1) and (2) stay green; with the
    /// `req = req.timeout(deadline)` line removed from `signed_request`, (1) reds (it does not return
    /// inside its 10 s ceiling) while this test and (2) stay green. Together the two breaks are exactly
    /// "pass a different duration" and "drop the `.timeout()` call" — the two ways CPE-1713 asks the
    /// replacement to prove it catches a disconnected constant.
    #[test]
    fn connect_installs_the_shipped_request_deadline_on_the_field_the_call_site_reads() {
        let provider = S3Provider::connect(&cfg("http://127.0.0.1:1"));
        assert_eq!(
            provider.request_deadline, TIMEOUT_LIST_REQUEST,
            "S3Provider::connect must install TIMEOUT_LIST_REQUEST on the field signed_get threads into \
             req.timeout(..) — a mismatch here means the shipped constant is disconnected from the real \
             constructor, exactly the class of bug CPE-1706 round 1 shipped undetected"
        );
    }

    /// Drives the mechanism through the same `signed_get` → `req.timeout(..)` → `req.call()` path
    /// production uses, with a short deadline injected via [`S3Provider::with_request_deadline`] so the
    /// guard can be broken and observed in a second rather than a minute. See the wiring test above for how
    /// this and [`the_shipped_timeout_values_are_finite_and_within_sane_bounds`] combine to cover the same
    /// ground the round-2 60 s live-fire test did, at a fraction of the cost.
    #[test]
    fn a_dribbling_server_is_cut_off_by_the_per_request_deadline() {
        let base = spawn_a_server_that_dribbles_one_byte_at_a_time(Duration::from_millis(50));
        let started = Instant::now();
        let err = call_with_deadline(
            "S3Provider::list against a dribbling server under a 500 ms request deadline",
            Duration::from_secs(30),
            move || {
                S3Provider::connect(&cfg(&base))
                    .with_request_deadline(Duration::from_millis(500))
                    .list("/")
            },
        )
        .expect_err("a dribbling server must be cut off by the per-request deadline");
        let elapsed = started.elapsed();
        assert!(err.starts_with("s3: http://127.0.0.1"), "{err}");
        assert!(
            elapsed < Duration::from_secs(10),
            "the 500 ms request deadline must be what ended it, but it took {elapsed:?}"
        );
    }

    /// The per-read socket bound, driven through the production request path: `connect_with_timeouts` and
    /// `connect` differ only in where the two `Duration`s come from — both build the agent via
    /// [`build_agent`], and the call then goes through the same `list` → `signed_get` → `req.call()` that
    /// production uses. A short bound is injected because waiting out the shipped 30 s would cost 30 s of
    /// wall clock in every CI job on three OSes; the shipped values are pinned separately by
    /// [`the_shipped_timeout_values_are_finite_and_within_sane_bounds`].
    ///
    /// **The request deadline is shortened alongside the read timeout**, because setting a per-request
    /// deadline *replaces* `timeout_read` for that request (`ureq` `stream.rs:433-436`) rather than
    /// layering over it. Leaving it at the shipped 60 s here would mean this test measured the deadline
    /// and not the read timeout at all — which is exactly the "verified at a boundary production does not
    /// go through" trap, one knob over.
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
            move || {
                S3Provider::connect_with_timeouts(&cfg(&base), short, short)
                    .with_request_deadline(short)
                    .list("/")
            },
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
        for (name, value) in [
            ("TIMEOUT_READ", TIMEOUT_READ),
            ("TIMEOUT_WRITE", TIMEOUT_WRITE),
            ("TIMEOUT_CONNECT", TIMEOUT_CONNECT),
            ("TIMEOUT_LIST_REQUEST", TIMEOUT_LIST_REQUEST),
        ] {
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

    /// Every knob [`build_agent`] sets, asserted on the **real production agent** (CPE-1706 round 2).
    ///
    /// `timeout_write` and `timeout_connect` were shipped in round 1 with no guard at all: deleting
    /// either line left all 133 tests passing. `timeout_write` is close to untestable end-to-end here —
    /// a `GET`'s request bytes are a few hundred and always fit in the socket buffer, so the write path
    /// never blocks — and `timeout_connect`'s whole stated rationale was *"pin it so a future `ureq`
    /// default change cannot silently unbound connect"*, which a deleted line defeats invisibly. An arm
    /// that cannot fail its own test cannot do the job it was added for.
    ///
    /// `ureq::Agent` derives `Debug` and prints its `AgentConfig`, which is how `ureq`'s own
    /// `agent_config_debug` test checks the same fields (`agent.rs:722-736`) — so this inspects the built
    /// agent rather than re-asserting the constants, and reds if any knob stops being wired.
    ///
    /// It also pins the **absence** of the agent-level overall `timeout`, which is a deliberate decision
    /// (it would replace the per-read bound and cap large-object transfers) and until now was recorded
    /// only in prose.
    #[test]
    fn build_agent_wires_every_timeout_knob_and_deliberately_leaves_the_agent_level_one_unset() {
        // Distinctive values nothing else would produce. ureq DEFAULTS timeout_connect to 30 s, so
        // asserting `Some(30s)` would pass with the line deleted — measured, that is exactly what the
        // first version of this test did.
        let agent = format!("{:?}", build_agent(Duration::from_secs(11), Duration::from_secs(12), Duration::from_secs(13)));
        assert!(agent.contains("timeout_read: Some(11s)"), "timeout_read is not wired: {agent}");
        assert!(agent.contains("timeout_write: Some(12s)"), "timeout_write is not wired: {agent}");
        assert!(
            agent.contains("timeout_connect: Some(13s)"),
            "timeout_connect is not wired — note ureq's own default is 30s, so a `Some(30s)` here would              mean the line was DELETED and the default was showing through: {agent}"
        );
        assert!(
            agent.contains("timeout: None"),
            "the AGENT-level overall timeout must stay unset — it takes precedence over timeout_read \
             (ureq agent.rs:476-477) and would cap a large-object GET by wall clock regardless of \
             progress. The per-REQUEST deadline is the right knob and is applied per call site: {agent}"
        );
        assert!(agent.contains("redirects: 0"), "the CPE-1461 no-redirect policy is not wired: {agent}");
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

    /// The `CommonPrefixes` mirror of
    /// [`a_key_nested_below_contents_own_level_is_not_taken_for_the_entrys_key`]. Round 1 tightened
    /// `<Prefix>` to `cp.children()` alongside `<Key>` but only tested the `<Key>` half, so reverting the
    /// `<Prefix>` line on its own reddened nothing (CPE-1706 round 2 review).
    #[test]
    fn a_prefix_nested_below_common_prefixes_own_level_is_not_taken_for_the_entrys_prefix() {
        // `<Meta>` precedes the real `<Prefix>` in document order, so a `descendants()` search rooted at
        // `<CommonPrefixes>` finds the decoy first.
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                     <CommonPrefixes><Meta><Prefix>decoy/</Prefix></Meta>\
                       <Prefix>real/</Prefix></CommonPrefixes>\
                   </ListBucketResult>";
        let page = parse_list_bucket_result(xml, "").unwrap();
        assert_eq!(page.entries.len(), 1, "{:?}", page.entries);
        assert_eq!(page.entries[0].name, "real", "a nested <Prefix> was taken for the entry's own");
        assert!(page.entries[0].is_dir);
    }

    /// The two *container* halves of item 2, which round 1 changed "for consistency" and never tested —
    /// reverting either to `doc.descendants()` reddened nothing (CPE-1706 round 2 review).
    ///
    /// A `<Contents>` or `<CommonPrefixes>` that is not a direct child of `<ListBucketResult>` is not a
    /// page entry at all: it is some other element's content, and a whole-document search would lift it
    /// out of its context and present it as a real file or directory. The two are asserted separately so
    /// each container line reds on its own.
    #[test]
    fn a_contents_element_nested_below_the_root_is_not_a_page_entry() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                     <SomeOtherElement><Contents><Key>ghost.txt</Key><Size>1</Size></Contents></SomeOtherElement>\
                     <Contents><Key>real.txt</Key><Size>2</Size></Contents>\
                   </ListBucketResult>";
        let page = parse_list_bucket_result(xml, "").unwrap();
        let names: Vec<&str> = page.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["real.txt"],
            "a <Contents> buried inside another element was lifted out and presented as a real file"
        );
    }

    #[test]
    fn a_common_prefixes_element_nested_below_the_root_is_not_a_page_entry() {
        let xml = "<ListBucketResult><IsTruncated>false</IsTruncated>\
                     <SomeOtherElement><CommonPrefixes><Prefix>ghost/</Prefix></CommonPrefixes></SomeOtherElement>\
                     <CommonPrefixes><Prefix>real/</Prefix></CommonPrefixes>\
                   </ListBucketResult>";
        let page = parse_list_bucket_result(xml, "").unwrap();
        let names: Vec<&str> = page.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["real"],
            "a <CommonPrefixes> buried inside another element was lifted out and presented as a real dir"
        );
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
        // "the way any other unsafe name is dropped" — which, since CPE-1704, means counted rather than
        // silently vanished. Putting the bound inside `is_safe_s3_leaf` is what buys this; a separate
        // `continue` in the parser would have dropped these two without ever telling anyone.
        assert_eq!(
            page.filtered_count, 2,
            "both over-cap entries must be COUNTED as filtered, not silently dropped"
        );
    }

    /// The length bound is the one arm of [`is_safe_s3_leaf`] CPE-1706 added, and this pins it at that
    /// function — the same place `S3Provider`'s `is_safe_leaf_name` override points, so `crates/vfs`'s
    /// `remote_dir_entries` applies it on the one path a user's real listing takes.
    ///
    /// **The boundary is checked on both sides on purpose.** An off-by-one here would silently drop a
    /// legal 1024-byte key — the exact class of bug CPE-1704 existed to fix (a legal key vanishing from a
    /// listing with no error, no warning, nothing), reintroduced by the ticket that was meant to harden
    /// the same function. `<= MAX_KEY_LEAF_BYTES` and `< MAX_KEY_LEAF_BYTES` differ only at the one input
    /// real S3 can actually produce at the limit, so a test that only checked "way over is refused" would
    /// pass under either.
    #[test]
    fn is_safe_s3_leaf_refuses_exactly_one_byte_past_the_key_limit_and_no_sooner() {
        assert!(
            is_safe_s3_leaf(&"a".repeat(MAX_KEY_LEAF_BYTES - 1)),
            "one byte under real S3's key limit is obviously legal"
        );
        assert!(
            is_safe_s3_leaf(&"a".repeat(MAX_KEY_LEAF_BYTES)),
            "a key at EXACTLY real S3's own 1024-byte limit is legal and must survive — refusing it is \
             the CPE-1704 bug (a legal key vanishing silently) reintroduced by an off-by-one"
        );
        assert!(
            !is_safe_s3_leaf(&"a".repeat(MAX_KEY_LEAF_BYTES + 1)),
            "one byte past the limit is not a key any conforming server can produce"
        );
    }

    /// The length arm is reachable through `&dyn FileSystemProvider`, not just through the free function.
    /// `crates/vfs::connect::remote_dir_entries` asks the provider through the vtable, so an arm that only
    /// worked on the concrete type would be invisible on the one path a user's real listing takes — the
    /// same "correct at a boundary production does not go through" trap CPE-1704 round 3 was about.
    #[test]
    fn the_key_length_bound_is_reachable_through_dynamic_dispatch_not_only_the_free_function() {
        let (base, _root, _requests) = spawn_s3_fixture();
        let provider = S3Provider::connect(&cfg(&base));
        let as_trait: &dyn FileSystemProvider = &provider;
        assert!(
            as_trait.is_safe_leaf_name(&"a".repeat(MAX_KEY_LEAF_BYTES)),
            "a key at the limit must survive dynamic dispatch too"
        );
        assert!(
            !as_trait.is_safe_leaf_name(&"a".repeat(MAX_KEY_LEAF_BYTES + 1)),
            "the length bound must survive dynamic dispatch — a `true` here means remote_dir_entries \
             never applies it and an 8 MiB 'filename' reaches the UI"
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

        // Deliberately not `.expect_err(..)`: with the cap removed this call succeeds with ~10 000
        // 900-byte-named entries, and `expect_err` would `Debug`-print all of them — a 9 MB panic message
        // that buries its own point. The count is the whole story.
        let err = match call_with_deadline(
            "S3Provider::list against a server returning a body over MAX_RESPONSE_BODY_BYTES",
            Duration::from_secs(60),
            move || S3Provider::connect(&cfg(&base)).list("/"),
        ) {
            Err(e) => e,
            Ok(entries) => panic!(
                "an over-cap body was parsed into a {}-entry listing and returned as complete — the body \
                 cap is what must stop this, and a partial listing sold as complete is the exact failure \
                 this module is written against",
                entries.len()
            ),
        };
        assert!(
            err.contains(&format!("{MAX_RESPONSE_BODY_BYTES}-byte cap")),
            "the error must name the cap that fired, so the cause is diagnosable: {err}"
        );
    }

    /// Sends a valid `200 OK` with a huge declared length and then streams as fast as the client will
    /// take it, **forever**. Distinct from the dribbler: this one is not slow, it is endless, so no
    /// time-based bound stops it — only the `.take()` inside [`read_body_capped`] does.
    fn spawn_a_server_that_streams_forever() -> String {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                std::thread::spawn(move || {
                    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\n\
                                Content-Length: 1099511627776\r\n\r\n";
                    if s.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    let chunk = [b' '; 8192];
                    while s.write_all(&chunk).is_ok() {}
                });
            }
        });
        format!("http://{addr}")
    }

    /// Makes the `.take()` in [`read_body_capped`] observable, which it otherwise is not.
    ///
    /// CPE-1706 round 2's review found the error-path body cap reddened nothing when deleted — a pure
    /// memory guard changes no output, so it looks untestable. Merging both read sites into one
    /// `read_body_capped` fixed the duplication, but the length comparison, not the `.take()`, is what
    /// the over-cap tests actually exercise: with `.take(u64::MAX)` those tests still pass, because a
    /// 9 MiB body is still longer than the cap. (Verified by probe, not assumed.)
    ///
    /// Against a server that never stops sending, the difference is stark: with the `.take()` the read
    /// ends after 8 MiB + 1 and reports the cap; without it, `read_to_end` grows the buffer until the
    /// process dies. The deadline harness turns that into a red instead of an OOM or a hung CI job.
    #[test]
    fn a_server_that_streams_without_end_is_stopped_by_the_body_cap_not_read_until_memory_runs_out() {
        let base = spawn_a_server_that_streams_forever();
        let err = call_with_deadline(
            "S3Provider::list against a server that never stops sending",
            Duration::from_secs(60),
            move || S3Provider::connect(&cfg(&base)).list("/"),
        )
        .expect_err("an endless body must be cut off at the cap");
        assert!(
            err.contains(&format!("{MAX_RESPONSE_BODY_BYTES}-byte cap")),
            "the error must name the cap that stopped the read: {err}"
        );
    }

    /// **The counter-example an independent UAT found to CPE-1706 round 1's item-6 claim.** That round
    /// asserted an over-cap body "always" surfaced as the parse error `the root node was opened but never
    /// closed`, and relied on truncation producing malformed XML. It does not: if the cut lands *after* a
    /// complete root element — here the real listing is tiny and the rest of the 8 MiB is legal post-root
    /// whitespace — the truncated prefix is perfectly well-formed and parses into a short, plausible,
    /// **wrong** listing. Round 1 measured `Ok in 241 ms with 1 entries: ["decoy.txt"]`.
    ///
    /// The fix is to stop inferring truncation from document shape and compare lengths instead, so this
    /// pins the shape that defeated the old reasoning.
    #[test]
    fn an_over_cap_body_that_is_still_well_formed_xml_is_refused_instead_of_sold_as_a_short_listing() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            // A complete, valid one-entry listing, then megabytes of legal post-root whitespace. Cut
            // anywhere in the padding and what remains still parses — as a 1-entry listing.
            let mut xml = String::from(
                "<ListBucketResult><IsTruncated>false</IsTruncated>\
                 <Contents><Key>decoy.txt</Key><Size>1</Size></Contents></ListBucketResult>",
            );
            xml.push_str(&" ".repeat(MAX_RESPONSE_BODY_BYTES + 1024 * 1024));
            for req in server.incoming_requests() {
                let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(xml.clone()).with_header(ct));
            }
        });
        let base = format!("http://{addr}");

        let result = call_with_deadline(
            "S3Provider::list against an over-cap body whose truncated prefix is still well-formed",
            Duration::from_secs(60),
            move || S3Provider::connect(&cfg(&base)).list("/"),
        );
        let err = match result {
            Err(e) => e,
            Ok(entries) => panic!(
                "an over-cap body was sold as a complete {}-entry listing ({:?}) — the truncated prefix \
                 parsed cleanly, which is exactly why truncation must be detected by LENGTH and not by \
                 whether the XML happens to still be well-formed",
                entries.len(),
                entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>()
            ),
        };
        assert!(
            err.contains(&format!("{MAX_RESPONSE_BODY_BYTES}-byte cap")),
            "the error must name the cap, not a parser accident: {err}"
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

    // =============================================================================================
    // CPE-1684: the object operations — stat / read / write / delete / mkdir, and the rename refusal.
    // =============================================================================================

    /// A provider wired to a fresh fixture, plus that fixture's root directory and request counter.
    fn s3_fixture_provider() -> (S3Provider, cpe_server::fsutil::ScratchDir, Arc<AtomicUsize>) {
        let (base, root, requests) = spawn_s3_fixture();
        (S3Provider::connect(&cfg(&base)), root, requests)
    }

    /// Compare two byte buffers without letting a failure `Debug`-print hundreds of kilobytes: report the
    /// first differing offset instead, which is the only part of the answer anyone can read.
    fn assert_bytes_eq(got: &[u8], want: &[u8], what: &str) {
        assert_eq!(got.len(), want.len(), "{what}: length differs");
        if let Some(i) = got.iter().zip(want).position(|(a, b)| a != b) {
            panic!("{what}: first difference at byte {i}: got {:#04x}, want {:#04x}", got[i], want[i]);
        }
    }

    // ---------------------------------------------------------------------------------------------
    // AC1: each op round-trips against the fixture.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn write_then_stat_round_trips_the_object_and_reports_the_size_the_server_actually_holds() {
        let (mut provider, root, _requests) = s3_fixture_provider();
        let body = b"hello from CPE-1684".to_vec();
        provider.write("/notes/hello.txt", &body).expect("write must succeed");

        // Assert on what the user GETS, not on the `Result`: two bugs shipped this week where the return
        // value said `Ok` while the file was empty or absent.
        let stored = std::fs::read(root.join("notes/hello.txt"))
            .expect("write returned Ok but the object is not on the server");
        assert_bytes_eq(&stored, &body, "the object the PUT actually stored");

        let entry = provider.stat("/notes/hello.txt").expect("stat must find the object just written");
        assert_eq!(entry.name, "hello.txt", "stat must name the key's last segment");
        assert!(!entry.is_dir, "an object is not a directory");
        assert_eq!(
            entry.size,
            body.len() as u64,
            "stat must report the size the server sent in Content-Length, not a placeholder"
        );
    }

    #[test]
    fn read_round_trips_a_multi_chunk_object_byte_for_byte() {
        let (mut provider, _root, _requests) = s3_fixture_provider();
        // Three full READ_CHUNK_BYTES chunks plus a partial one, with a position-dependent pattern, so a
        // loop that dropped, duplicated, reordered or short-read a chunk cannot pass by accident.
        let body: Vec<u8> = (0..(READ_CHUNK_BYTES * 3 + 1234)).map(|i| (i % 251) as u8).collect();
        provider.write("/big.bin", &body).expect("write must succeed");

        let got = provider.read("/big.bin").expect("read must return the object");
        assert_bytes_eq(&got, &body, "the object read back over several chunks");
    }

    #[test]
    fn delete_removes_the_object_and_the_listing_stops_showing_it() {
        let (mut provider, root, _requests) = s3_fixture_provider();
        provider.write("/docs/a.txt", b"a").unwrap();
        provider.write("/docs/b.txt", b"b").unwrap();

        provider.delete("/docs/a.txt").expect("deleting one object must succeed");

        assert!(
            !root.join("docs/a.txt").exists(),
            "delete returned Ok while the object is still on the server — S3 answers 204 for anything, so \
             the Result alone proves nothing"
        );
        let names: Vec<String> =
            provider.list("/docs").unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["b.txt".to_string()], "the listing must reflect the delete");
    }

    // ---------------------------------------------------------------------------------------------
    // AC5: the mkdir marker is the exact key CPE-1683's parser filters out.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn mkdir_writes_the_marker_at_the_key_list_filters_so_the_parent_shows_a_dir_and_no_stray_file() {
        let (mut provider, root, _requests) = s3_fixture_provider();
        std::fs::create_dir_all(root.join("photos")).unwrap();

        provider.mkdir("/photos/2024").expect("mkdir must succeed");

        // The shape itself, pinned against the function CPE-1683 made `pub` for exactly this call. Deriving
        // it a second way here — off by one slash — is precisely how a phantom folder gets created.
        assert_eq!(provider_path_to_key_prefix("/photos/2024"), "photos/2024/");
        assert!(
            root.join("photos/2024/.s3marker").is_file(),
            "the marker object was not written at the agreed key `photos/2024/`"
        );

        let parent: Vec<(String, bool)> =
            provider.list("/photos").unwrap().into_iter().map(|e| (e.name, e.is_dir)).collect();
        assert_eq!(
            parent,
            vec![("2024".to_string(), true)],
            "the parent must show the new directory, and nothing else"
        );

        let inside = provider.list("/photos/2024").unwrap();
        assert!(
            inside.is_empty(),
            "the marker came back as a phantom zero-byte file inside its own directory: {:?}",
            inside.iter().map(|e| (e.name.as_str(), e.is_dir, e.size)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn stat_reports_a_freshly_created_empty_directory_as_a_directory_not_as_missing() {
        let (mut provider, _root, _requests) = s3_fixture_provider();
        provider.mkdir("/photos/2024").expect("mkdir must succeed");

        let entry = provider
            .stat("/photos/2024")
            .expect("a directory this very module just created must not stat as missing");
        assert!(entry.is_dir, "a prefix with a marker under it is a directory");
        assert_eq!(entry.name, "2024");
    }

    #[test]
    fn stat_reports_a_marker_less_prefix_with_content_under_it_as_a_directory() {
        // The common real-bucket shape: nobody wrote a marker, the "directory" exists only because keys
        // sit under the prefix. A HEAD on `photos` 404s, and answering "not found" for a path `list` will
        // happily show would be a flat contradiction between two ops on the same provider.
        let (mut provider, _root, _requests) = s3_fixture_provider();
        provider.write("/photos/a.jpg", b"jpeg").unwrap();

        let entry = provider.stat("/photos").expect("a prefix with keys under it is a directory");
        assert!(entry.is_dir);
        assert_eq!(entry.name, "photos");
    }

    // ---------------------------------------------------------------------------------------------
    // AC4: a missing key and a denied key are distinguishable, through a BODILESS HEAD response.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn stat_on_a_missing_key_is_a_clear_not_found_and_not_the_parsers_refusing_to_guess() {
        let (provider, _root, _requests) = s3_fixture_provider();
        let err = provider.stat("/nope.txt").expect_err("a missing key must not stat");

        assert!(err.contains("not found"), "a missing key must be reported as not found: {err}");
        assert!(err.contains("404"), "the error must name the status it saw: {err}");
        assert!(
            !err.contains("refusing to guess"),
            "the bodiless-HEAD rule exists precisely so this does NOT fall through to map_s3_error's \
             \"the response body could not be read … refusing to guess which cause applies\", which would \
             otherwise be the majority user experience for the single most common failure: {err}"
        );
        assert!(
            !err.to_lowercase().contains("denied"),
            "a missing key must never be reported as a denial: {err}"
        );
    }

    #[test]
    fn stat_on_a_denied_key_reports_the_denial_and_is_distinguishable_from_a_missing_one() {
        let (provider, _root, _requests) = s3_fixture_provider();
        let denied = provider.stat("/deny-secret.txt").expect_err("a 403 must not stat");
        let missing = provider.stat("/nope.txt").expect_err("a 404 must not stat");

        assert!(denied.contains("access denied"), "a 403 must be reported as a denial: {denied}");
        assert!(denied.contains("403"), "the error must name the status it saw: {denied}");
        assert!(
            !denied.contains("not found"),
            "a denial must never be softened into not-found — that is the exact confusion the criterion \
             forbids: {denied}"
        );
        assert_ne!(denied, missing, "the two cases must not produce the same message");
        // The honest caveat, not a hedge: AWS documents answering 403 rather than 404 for a key that does
        // not exist when the caller lacks s3:ListBucket, so a flat "access denied" would be a confident
        // wrong answer a third of the time.
        assert!(
            denied.contains("s3:ListBucket"),
            "the message must say that a 403 can also mean 'missing, and you may not be told so': {denied}"
        );
    }

    /// A raw-socket server, not the `tiny_http` fixture, and that is the point: `tiny_http` **always**
    /// emits a `Content-Length` (or chunked encoding) and silently drops a `Content-Length` header it
    /// cannot parse (`response.rs:266-270`), so it physically cannot produce the response this test needs.
    /// The first version of this test used the fixture and passed while measuring nothing — it received
    /// `Content-Length: 0` and `stat` dutifully reported a zero-byte object.
    fn spawn_a_server_that_answers_head_200_with_no_content_length() -> String {
        use std::io::{BufRead as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                // Drain the request head BEFORE answering. Writing a response and closing while the client
                // is still sending resets the connection, and the client then reports a transport error
                // instead of the response — measured, not guessed: the first version of this helper did
                // exactly that and produced `os error 10054, An existing connection was forcibly closed`.
                let Ok(peek) = s.try_clone() else { continue };
                let mut reader = std::io::BufReader::new(peek);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) if line == "\r\n" || line == "\n" => break,
                        Ok(_) => {}
                    }
                }
                // `Connection: close` plus a clean write-shutdown gives the client a definite end of
                // message without ever stating a length — the shape a broken gateway produces.
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                let _ = s.flush();
                let _ = s.shutdown(std::net::Shutdown::Write);
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn stat_refuses_a_200_head_with_no_usable_content_length_rather_than_inventing_a_zero_size() {
        let base = spawn_a_server_that_answers_head_200_with_no_content_length();
        let provider = S3Provider::connect(&cfg(&base));

        let err = call_with_deadline(
            "S3Provider::stat against a 200 HEAD carrying no Content-Length",
            Duration::from_secs(60),
            move || provider.stat("/x.bin"),
        )
        .expect_err("a 200 HEAD with no Content-Length has not told us the size");
        assert!(err.contains("Content-Length"), "the error must name what was missing: {err}");
        assert!(
            err.contains("refusing to report 0"),
            "size 0 would be an invented measurement, not a read one: {err}"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1749: `read` and `stat` both used to accept any `2xx`, so an unsolicited `206` — this client
    // sends no `Range` header on either verb — parsed cleanly. On `read` that meant a truncated body
    // returned as `Ok`, measured as `READ 206 >>> Ok([72, 65, 76, 70])` against a 4-byte `HALF` body,
    // which `cpe_server::transfer`'s download sink would write to disk as the finished file. On `stat`
    // it meant `Content-Length` — under a `206` the RANGE length, not the object's — reported as the
    // object's size. `list`'s sibling fix is CPE-1740.
    // ---------------------------------------------------------------------------------------------

    /// A server that answers every request with an unsolicited `206` and `Content-Length: body.len()`,
    /// and records whether any request carried a `Range` header. Used by both the `stat` and `read`
    /// CPE-1749 tests below — a HEAD gets a bodiless 206 (tiny_http suppresses the body itself, matching
    /// `handle`'s own HEAD arm), a GET gets `body` itself — so a guard that only catches one verb cannot
    /// pass by only fixing the other.
    fn spawn_a_server_that_answers_with_an_unsolicited_206(
        body: &'static [u8],
    ) -> (String, Arc<std::sync::Mutex<Vec<bool>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let saw_range = Arc::new(std::sync::Mutex::new(Vec::new()));
        let saw_range_thread = Arc::clone(&saw_range);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let has_range = req.headers().iter().any(|h| h.field.equiv("Range"));
                saw_range_thread.lock().unwrap().push(has_range);
                let len = tiny_http::Header::from_bytes(
                    &b"Content-Length"[..],
                    body.len().to_string().as_bytes(),
                )
                .unwrap();
                if req.method() == &tiny_http::Method::Head {
                    let _ = req.respond(tiny_http::Response::empty(206).with_header(len));
                } else {
                    let _ = req.respond(
                        tiny_http::Response::from_data(body).with_status_code(206).with_header(len),
                    );
                }
            }
        });
        (format!("http://{addr}"), saw_range)
    }

    /// Pins the read half of AC1 and AC3: the measured reproduction from the ticket, reproduced against
    /// this crate. Effect asserted BEFORE the `Result` is unwrapped (this sprint's standing rule; also
    /// AC3) — this defect fails by returning `Ok`, so `.expect_err()` would panic unreachably on a
    /// regression instead of naming the truncated bytes that came back.
    #[test]
    fn read_refuses_an_unsolicited_206_rather_than_returning_the_truncated_body_as_the_whole_object() {
        let (base, saw_range) = spawn_a_server_that_answers_with_an_unsolicited_206(b"HALF");
        let provider = S3Provider::connect(&cfg(&base));

        let outcome = call_with_deadline(
            "S3Provider::read against an unsolicited 206",
            Duration::from_secs(60),
            move || provider.read("/x.bin"),
        );
        match outcome {
            Ok(bytes) => panic!(
                "THE DEFECT: an unsolicited 206 carrying the 4-byte body b\"HALF\" was accepted and \
                 returned as the object's complete contents: {bytes:?} ({} bytes) — \
                 `cpe_server::transfer`'s download sink writes exactly this to disk as the finished file, \
                 so this is a truncated download reported as a success",
                bytes.len()
            ),
            Err(err) => {
                assert!(err.contains("206"), "the message must name the status that caused the refusal: {err}");
                assert!(err.contains("200"), "and say that a complete body requires exactly 200: {err}");
            }
        }
        assert_eq!(
            *saw_range.lock().unwrap(),
            vec![false],
            "the fixture must serve this 206 to a request carrying NO Range header — otherwise this test \
             cannot tell an unsolicited 206 apart from a legitimately-ranged reply, and would pass for the \
             wrong reason"
        );
    }

    /// Pins the stat half of AC1: an unsolicited 206's `Content-Length` is a RANGE length, and reporting
    /// it as the object's size would be exactly the invented-measurement failure
    /// `stat_refuses_a_200_head_with_no_usable_content_length_...` already guards for a missing header —
    /// this is the same guard against a present-but-wrong one.
    #[test]
    fn stat_refuses_an_unsolicited_206_rather_than_reporting_the_range_length_as_the_objects_size() {
        let (base, saw_range) = spawn_a_server_that_answers_with_an_unsolicited_206(b"HALF");
        let provider = S3Provider::connect(&cfg(&base));

        let outcome = call_with_deadline(
            "S3Provider::stat against an unsolicited 206",
            Duration::from_secs(60),
            move || provider.stat("/x.bin"),
        );
        match outcome {
            Ok(entry) => panic!(
                "THE DEFECT: an unsolicited 206 whose Content-Length is a 4-byte RANGE length was \
                 accepted and reported as the object's size: {entry:?} — the object's real size is \
                 unknown, and this claims to have measured it"
            ),
            Err(err) => {
                assert!(err.contains("206"), "the message must name the status that caused the refusal: {err}");
                assert!(err.contains("200"), "and say that stat requires exactly 200: {err}");
            }
        }
        assert_eq!(
            *saw_range.lock().unwrap(),
            vec![false],
            "the fixture must serve this 206 to a request carrying NO Range header — otherwise this test \
             cannot tell an unsolicited 206 apart from a legitimately-ranged reply, and would pass for the \
             wrong reason"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // AC2: `read` is bounded, and the bound fires DURING the transfer.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn a_read_past_the_cap_is_refused_and_never_returns_the_truncated_prefix_as_the_file() {
        let (base, root, _requests) = spawn_s3_fixture();
        std::fs::write(root.join("big.bin"), vec![b'x'; 300 * 1024]).unwrap();
        let provider = S3Provider::connect(&cfg(&base)).with_read_cap(128 * 1024);

        let result = call_with_deadline(
            "S3Provider::read of an object past the read cap",
            Duration::from_secs(60),
            move || provider.read("/big.bin"),
        );
        let err = match result {
            Err(e) => e,
            Ok(bytes) => panic!(
                "an over-cap object came back as {} bytes of `Ok` — `cpe_server::transfer`'s download sink \
                 writes whatever comes back to disk as the finished file, so a truncated Vec here is data \
                 loss wearing a success",
                bytes.len()
            ),
        };
        assert!(err.contains("read cap"), "the error must name the bound that fired: {err}");
        assert!(err.contains("131072"), "the error must name the cap's value: {err}");
    }

    /// The one that proves the cap fires **during** the transfer rather than after it.
    ///
    /// Against a finite over-cap object the check above is satisfied by any bound at all, including one
    /// applied after a `read_to_end`. Against a server that never stops sending, only a check inside the
    /// [`READ_CHUNK_BYTES`] loop can end the call: without it the buffer grows until the process dies, so
    /// the deadline harness turns the regression into a red instead of an OOM or a six-hour CI job.
    #[test]
    fn a_read_from_a_server_that_never_stops_sending_is_cut_off_by_the_cap_not_read_until_memory_runs_out() {
        let base = spawn_a_server_that_streams_forever();
        let provider = S3Provider::connect(&cfg(&base)).with_read_cap(64 * 1024);

        let err = call_with_deadline(
            "S3Provider::read against a server that never stops sending",
            Duration::from_secs(60),
            move || provider.read("/endless.bin"),
        )
        .expect_err("an endless body must be cut off at the cap");
        assert!(err.contains("read cap"), "the error must name the cap that stopped the read: {err}");
    }

    #[test]
    fn the_read_chunk_size_matches_the_convention_cpe_ftp_settled() {
        assert_eq!(
            READ_CHUNK_BYTES,
            64 * 1024,
            "the fixed 64 KiB chunk is the convention `cpe-ftp` settled and this ticket was told to match"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // AC3: `rename` refuses honestly — no PUT-copy, no DELETE, and the capability says so up front.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn rename_is_refused_by_name_and_issues_no_request_at_all() {
        let (base, root, requests) = spawn_s3_fixture();
        let mut provider = S3Provider::connect(&cfg(&base));
        std::fs::write(root.join("a.txt"), b"a").unwrap();

        let outcome = provider.rename("/a.txt", "/b.txt");

        // Assert on the harm BEFORE unwrapping the Result: if the guard ever answers `Ok`, the run must
        // still reach these and red on the damage, not stop at `expect_err`.
        assert_eq!(
            requests.load(Ordering::Relaxed),
            0,
            "rename reached the network — refusing means refusing: no CopyObject PUT and no DELETE may be \
             issued, because a delete that fails after a successful copy silently leaves two objects \
             (outcome was {outcome:?})"
        );
        assert!(root.join("a.txt").is_file(), "the source object must be untouched (outcome was {outcome:?})");
        assert!(!root.join("b.txt").exists(), "no destination object may have been created (outcome was {outcome:?})");

        let err = outcome.expect_err("S3 has no rename; a copy+delete emulation must not be attempted");
        assert!(err.contains("no rename"), "the error must name S3's lack of a rename: {err}");
        assert!(
            err.contains("not atomic"),
            "the error must say why the emulation is refused, not just that it is: {err}"
        );

        assert!(
            !provider.capabilities().supports_rename,
            "a caller must be able to see the refusal coming instead of discovering it by trying"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // `delete`: exactly one key. A directory with content is refused, not silently no-op'd.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn delete_of_a_directory_with_content_is_refused_and_removes_nothing() {
        let (mut provider, root, _requests) = s3_fixture_provider();
        provider.write("/photos/2024/a.jpg", b"a").unwrap();
        provider.write("/photos/2024/b.jpg", b"b").unwrap();

        let outcome = provider.delete("/photos/2024");

        // Assert on what the user still has BEFORE unwrapping the Result: if the guard ever answers
        // `Ok`, the run must still reach these and red on the damage, not stop at `expect_err`.
        assert!(
            root.join("photos/2024/a.jpg").is_file(),
            "a.jpg was deleted by a refused delete (outcome was {outcome:?})"
        );
        assert!(
            root.join("photos/2024/b.jpg").is_file(),
            "b.jpg was deleted by a refused delete (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "S3 answers 204 to a DELETE of a key that never existed, so a single-key delete of a \
             directory prefix would report success while the whole subtree stayed put",
        );
        assert!(
            err.contains("recursive"),
            "the error must name the missing capability, not just fail: {err}"
        );
    }

    #[test]
    fn delete_of_an_empty_directory_removes_its_marker_key_because_that_really_is_one_key() {
        let (mut provider, root, _requests) = s3_fixture_provider();
        provider.mkdir("/scratch").expect("mkdir must succeed");
        assert!(root.join("scratch/.s3marker").is_file(), "precondition: the marker exists");

        provider.delete("/scratch").expect("an empty directory is one key and can be deleted honestly");
        assert!(!root.join("scratch").exists(), "the marker key survived a successful-looking delete");
    }

    /// **CPE-1723 item 3 — the display half of the marker filter.** `parse_list_bucket_result`'s
    /// `leaf.is_empty()` check is what classifies a directory's own zero-byte marker as *ignored* rather
    /// than *filtered*. Without it the marker falls through to `is_safe_s3_leaf("")`, which is `false`, so
    /// it lands in `filtered_count` instead — and every folder `mkdir` has just created starts reporting
    /// `filtered == 1`.
    ///
    /// Nothing asserted that until now. The count is read by `crates/vfs::connect::remote_dir_entries`, and
    /// once CPE-1708 surfaces it the user would be shown a spurious **"1 entry hidden"** on every empty
    /// folder they create — a warning about a file that does not exist, on the one listing where they know
    /// for certain nothing is there.
    ///
    /// Deliberately through `&dyn FileSystemProvider`: that is the shape `remote_dir_entries` holds, and
    /// CPE-1704 spent three rounds on fixes verified on the concrete type instead.
    #[test]
    fn a_freshly_created_empty_directory_reports_nothing_filtered_so_no_phantom_hidden_entry_is_shown() {
        let (base, root, _requests) = spawn_s3_fixture();
        let mut concrete = S3Provider::connect(&cfg(&base));
        {
            let provider: &mut dyn FileSystemProvider = &mut concrete;
            provider.mkdir("/fresh").expect("mkdir must succeed");
        }
        assert!(root.join("fresh/.s3marker").is_file(), "precondition: the marker object exists");

        let as_trait: &dyn FileSystemProvider = &concrete;
        let (entries, filtered) = as_trait
            .list_with_filtered_count("/fresh")
            .expect("listing a freshly created directory");

        assert!(entries.is_empty(), "a freshly created directory has no children: {entries:?}");
        assert_eq!(
            filtered, 0,
            "the directory's own marker is ignored, not filtered — counting it here is what would show \
             the user a spurious '1 entry hidden' on every empty folder they create"
        );
    }

    /// The marker of a directory is a strict prefix of every key beneath it, so S3's lexicographic order
    /// always returns it **first**. This pins the reason [`S3Provider::probe_prefix`] cannot ask for one
    /// key: with `max-keys=1` a directory holding a thousand objects looks exactly like an empty one, and
    /// `delete` would remove its marker and report success. The fixture paginates for real, so capping its
    /// pages at one key reproduces exactly that under-filled response.
    #[test]
    fn a_directory_whose_first_returned_key_is_only_its_marker_is_still_refused_by_delete() {
        let (base, root, _requests) = spawn_s3_fixture_with_page_cap(Some(1));
        let mut provider = S3Provider::connect(&cfg(&base));
        std::fs::create_dir_all(root.join("photos/2024")).unwrap();
        std::fs::write(root.join("photos/2024/.s3marker"), b"").unwrap();
        std::fs::write(root.join("photos/2024/a.jpg"), b"a").unwrap();

        let outcome = provider.delete("/photos/2024");

        assert!(
            root.join("photos/2024/a.jpg").is_file(),
            "the content was deleted anyway (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "a server that returned only the marker on the first page must not be read as an empty \
             directory — IsTruncated said there was more",
        );
        assert!(err.contains("recursive"), "{err}");
    }

    /// **The sharpest bug the round-2 UAT found, and it was mine.** `probe_prefix` reported "real
    /// entries" as `page.entries.len()` — a count taken **after** [`is_safe_s3_leaf`] filtering. That guard
    /// refuses a leaf containing `\`, exactly `..`/`.`, or a tab or LF (the control bytes XML 1.0
    /// permits — see [`S3Provider::probe_prefix`] and CPE-1723 item 8), every one of which is a perfectly
    /// legal S3 key. Such an object landed in `raw_entries` but not in `entries`, so `delete`
    /// read a directory holding one as "marker only", deleted the marker, got its `204`, and told the user
    /// the folder was gone. On a **conforming** server — right ordering, right `IsTruncated`, `max-keys`
    /// honoured. Not a hostile one.
    ///
    /// The marker-present variant is the worse one: the marker really is removed, so the folder vanishes
    /// from every listing while the object survives underneath it, now unreachable through the UI.
    ///
    /// What makes it worth this comment is that [`is_safe_s3_leaf`]'s own doc already said *"A leaf this
    /// refuses is a real S3 key"*. The model was written down correctly and the probe consulted the wrong
    /// number anyway.
    ///
    /// `filtered_count` is therefore part of "is there content here", not a diagnostic sideline.
    #[test]
    fn delete_refuses_a_directory_whose_only_content_is_an_object_list_filters_out() {
        let (mut provider, root, _requests) = s3_fixture_provider();
        std::fs::create_dir_all(root.join("photos")).unwrap();
        // The directory holds its own marker AND one real object whose leaf `is_safe_s3_leaf` refuses.
        std::fs::write(root.join("photos/.s3marker"), b"").unwrap();
        std::fs::write(root.join("photos/.s3unsafe"), b"").unwrap();

        // Precondition, and the whole trap in two lines: the listing shows NOTHING but reports one entry
        // filtered — so `entries.len()` is 0 while the prefix genuinely holds an object.
        let (entries, filtered) = provider.list_with_filtered_count("/photos").unwrap();
        assert_eq!(entries.len(), 0, "precondition: the filtered leaf must not be surfaced");
        assert_eq!(filtered, 1, "precondition: it must be counted as filtered, not vanish");

        let outcome = provider.delete("/photos");

        // Assert on what the user still has BEFORE unwrapping: the marker must NOT have been deleted,
        // because deleting it is what makes the folder disappear while the object stays.
        assert!(
            root.join("photos/.s3marker").is_file(),
            "the marker was deleted by a refused delete — the folder would vanish from listings while \
             the object underneath it survived (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "the prefix holds a real object that `list` merely refuses to display — deleting the marker \
             and reporting success would tell the user a folder was removed while its contents survive, \
             now unreachable through the UI",
        );
        assert!(err.contains("recursive"), "the error must name the missing capability: {err}");
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1723 item 1: a credential without `s3:ListBucket` must not be told about a prefix it never
    // typed. The probe is named as what failed; the delete is refused rather than guessed at.
    // ---------------------------------------------------------------------------------------------

    /// The exact reported symptom: `delete` with `s3:DeleteObject` but no `s3:ListBucket` produced an
    /// access-denied whose **subject was `"photos/gone.jpg/"`** — a trailing-slash prefix the user never
    /// wrote, about an internal check they were never told existed.
    ///
    /// The `starts_with` assertion is the load-bearing one: it names the broken shape exactly, so it cannot
    /// be satisfied by an error that merely happens to mention the words.
    ///
    /// **CPE-1727 retargeted this at a key that does not exist.** It used to delete `/photos/a.jpg` with
    /// the object really present, and asserted the refusal — which is now the wrong expectation: item 1
    /// restores that delete via a HEAD, and
    /// `delete_of_a_real_object_succeeds_without_listbucket_because_a_head_proves_object_ness` asserts it
    /// succeeds. The message this test is about is unchanged and still reachable: it is what a user gets
    /// when the HEAD cannot prove object-ness either, which is the whole remaining refusal path.
    #[test]
    fn delete_without_listbucket_names_the_probe_instead_of_a_prefix_the_user_never_typed() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("photos/a.jpg"), b"jpeg").unwrap();
        let mut provider = S3Provider::connect(&cfg(&base));

        let outcome = provider.delete("/photos/gone.jpg");

        // Assert on what the user still has BEFORE unwrapping the Result.
        assert!(
            root.join("photos/a.jpg").is_file(),
            "a refused delete removed the object anyway (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "the directory check cannot run and no HEAD proves an object, so the delete must \
             be refused, not guessed",
        );

        assert!(
            !err.starts_with("s3: \"photos/gone.jpg/\""),
            "this is the reported bug verbatim: the error's subject is a trailing-slash prefix the user \
             never typed, produced by an internal probe they were never told about: {err}"
        );
        assert!(
            err.contains("ListObjectsV2"),
            "the error must name the probe as the thing that failed: {err}"
        );
        assert!(
            err.contains("s3:ListBucket"),
            "the error must name the permission that would fix it — that is the whole point of naming the \
             probe: {err}"
        );
        assert!(
            err.contains("nothing has been deleted"),
            "a user reading a failed delete needs to know whether anything happened: {err}"
        );
    }

    /// **CPE-1727 item 1, the operation this family of tickets is about.** A credential holding
    /// `s3:GetObject` + `s3:DeleteObject` and **not** `s3:ListBucket` must be able to delete an ordinary
    /// object. CPE-1723 left it unable to delete anything at all, and asserted that as correct.
    ///
    /// The restoring mechanism is a `HEAD` on the key itself — a question `s3:GetObject` permits, and one
    /// a pure prefix can never answer 200 to, because a virtual directory has no object at its own key.
    /// Deleting the fallback in [`FileSystemProvider::delete`] reds this test and only this test among the
    /// no-`s3:ListBucket` set; taking the *proof* out of it (deleting on any HEAD answer) reds
    /// `a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses` instead,
    /// which is the pair that shows both halves are load-bearing.
    #[test]
    fn delete_of_a_real_object_succeeds_without_listbucket_because_a_head_proves_object_ness() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("photos/single.jpg"), b"jpeg").unwrap();
        std::fs::write(root.join("photos/keep.jpg"), b"keep").unwrap();
        let mut provider = S3Provider::connect(&cfg(&base));

        provider
            .delete("/photos/single.jpg")
            .expect("a HEAD proves this key names an object, which is all the delete needs to be safe");

        // Assert on the server's filesystem, not on the `Result`.
        assert!(
            !root.join("photos/single.jpg").exists(),
            "delete returned Ok while the object is still on the server"
        );
        assert!(
            root.join("photos/keep.jpg").is_file(),
            "the delete removed a key it was never asked about"
        );
    }

    /// **The case the ticket said to measure before shipping**, and the one place where *"HEAD says
    /// object"* and *"the prefix has content"* are true at the same time: an object named `photos` and a
    /// set of objects under `photos/`. Nothing in S3 forbids it — keys are strings — and the
    /// filesystem-backed fixture cannot represent it (a path cannot be a file and a directory at once), so
    /// this stands up a keyspace-backed server instead and asserts on **the keys the server still holds**.
    ///
    /// **Measured behaviour, recorded rather than argued about:** the object `photos` is deleted and
    /// `photos/a.jpg` and `photos/b.jpg` are untouched. The DELETE goes to the exact key the HEAD proved,
    /// never the `photos/` marker form, so the collision cannot orphan the subtree.
    ///
    /// **What that costs, stated plainly:** a user who meant the *folder* gets a success and a folder that
    /// is still there. A client that may not list cannot tell those two intentions apart — and the
    /// alternative is CPE-1723's blanket refusal, which is what this ticket exists to undo.
    #[test]
    fn an_object_and_a_prefix_sharing_a_name_deletes_only_the_object_the_head_proved() {
        let (base, keys) = spawn_a_keyspace_server_without_listbucket(&[
            "photos",
            "photos/a.jpg",
            "photos/b.jpg",
        ]);
        let mut provider = S3Provider::connect(&cfg(&base));

        provider
            .delete("/photos")
            .expect("the HEAD proved an object at exactly this key, so exactly that key is removed");

        let left: Vec<String> = keys.lock().unwrap().iter().cloned().collect();
        assert_eq!(
            left,
            vec!["photos/a.jpg".to_string(), "photos/b.jpg".to_string()],
            "the delete must remove the object `photos` and nothing under the prefix `photos/`"
        );
    }

    /// **The mirror of the test above, built by the PR #903 UAT, and the more uncomfortable half.** Same
    /// keyspace, same call — a credential that **can** list. The probe succeeds, sees `photos/a.jpg` and
    /// `photos/b.jpg` under the prefix, and refuses. So the object `photos` is deletable by the *weaker*
    /// credential and **permanently undeletable** by the stronger one: granting `s3:ListBucket` makes an
    /// operation impossible.
    ///
    /// **A characterisation test, not a guard.** On `origin/main` both credentials were refused, so
    /// CPE-1727 creates this divergence; it is recorded here, in [`FileSystemProvider::delete`]'s doc, and
    /// in `src/docs/31-network.md` rather than designed away, because the fix is a caller that can say
    /// whether it meant the object or the folder (CPE-1735) and not a different guess inside `delete`.
    #[test]
    fn the_credential_that_can_list_is_the_one_that_cannot_delete_the_colliding_object() {
        let (base, keys) = spawn_a_keyspace_server_with_listbucket(&[
            "photos",
            "photos/a.jpg",
            "photos/b.jpg",
        ]);
        let mut provider = S3Provider::connect(&cfg(&base));

        let outcome = provider.delete("/photos");

        // Assert the harm BEFORE unwrapping the Result (CPE-1743): if the guard ever answers `Ok`, the
        // run must still reach this and red on the key set that actually survived, not on
        // "expected an error".
        let left: Vec<String> = keys.lock().unwrap().iter().cloned().collect();
        assert_eq!(
            left,
            vec!["photos".to_string(), "photos/a.jpg".to_string(), "photos/b.jpg".to_string()],
            "nothing was removed — and the object `photos` cannot be removed by this credential at all, \
             while the credential WITHOUT s3:ListBucket removes it in the test above (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "the probe sees content under photos/ and refuses — which is right for the folder and wrong \
             for the object of the same name",
        );
        assert!(err.contains("recursive"), "the refusal is the directory refusal: {err}");
    }

    /// **What the collision actually looks like to a user, measured by the PR #903 UAT round 3** — and the
    /// correction to an argument this PR made in round 2. `list("/")` over the colliding keyspace returns
    /// `photos` **twice**: one `is_dir: false` row for the object and one `is_dir: true` row for the
    /// prefix. That is what real S3 returns (`<Contents>` and `<CommonPrefixes>` are independent), and it
    /// means a user clicking the file row has *already* said which of the two they meant.
    ///
    /// Round 2 argued the privileged path must keep refusing because deleting the object would "report the
    /// row the user clicked as deleted while the folder stands". That is only true of the folder row. The
    /// real obstacle is that the bit is lost between the row and the call — see
    /// [`FileSystemProvider::delete`]'s doc and CPE-1735.
    ///
    /// It also records a hazard for any UI keyed on `name`: two entries in one listing share a name and
    /// differ only in `is_dir`. Filed as CPE-1737.
    #[test]
    fn a_name_collision_lists_as_two_rows_so_the_user_has_already_said_which_one_they_meant() {
        let (base, _keys) = spawn_a_keyspace_server_with_listbucket(&[
            "photos",
            "photos/a.jpg",
            "photos/b.jpg",
        ]);
        let provider = S3Provider::connect(&cfg(&base));

        let entries = provider.list("/").expect("listing the bucket root");
        let rows: Vec<(String, bool)> =
            entries.iter().map(|e| (e.name.clone(), e.is_dir)).collect();
        assert_eq!(
            rows,
            vec![("photos".to_string(), false), ("photos".to_string(), true)],
            "the object and the prefix are two independent rows with the same name — so the row carries \
             the distinction that `delete(path)` then loses"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1748 — the bug this ticket exists to close: `stat`/`read`/`delete` used to compute the SAME
    // object key for `/photos` and `/photos/`, so a directory silently collapsed onto its same-named
    // file the moment either op was wired up. Exercised over the exact colliding keyspace CPE-1737
    // uses, per this ticket's own acceptance criteria.
    // ---------------------------------------------------------------------------------------------

    /// **The bug, for `stat`.** Before this fix, `S3Provider::stat("/photos/")` and
    /// `S3Provider::stat("/photos")` computed the identical object key (`provider_path_to_object_key`
    /// strips the trailing `/` on the way in), so BOTH answered `is_dir: false` — the directory row
    /// silently collapsed onto the file. Breaking the fix (deleting the
    /// `path_addresses_a_directory(path, &key)` guard clause in `stat`, so it falls straight through to
    /// the HEAD-on-object logic below) reds this at the FIRST assertion, with `is_dir: false` where a
    /// directory was asked for — naming which object was actually returned, not a mismatched string.
    #[test]
    fn stat_on_the_colliding_keyspace_resolves_the_trailing_slash_to_the_prefix_and_the_bare_path_to_the_object()
    {
        let (base, _keys) = spawn_a_keyspace_server_with_listbucket(&[
            "photos",
            "photos/a.jpg",
            "photos/b.jpg",
        ]);
        let provider = S3Provider::connect(&cfg(&base));

        let dir_entry = provider.stat("/photos/").expect("the prefix has content under it");
        assert!(
            dir_entry.is_dir,
            "stat(\"/photos/\") must resolve to the DIRECTORY — got a FILE result (is_dir: false), \
             which is the exact CPE-1737 collision reopened at the stat layer"
        );

        let file_entry = provider.stat("/photos").expect("the bare path resolves to the object");
        assert!(
            !file_entry.is_dir,
            "stat(\"/photos\") must resolve to the OBJECT — got a DIRECTORY result (is_dir: true), \
             which would mean the bare path can no longer reach the file at all"
        );
        assert_eq!(file_entry.size, 4, "the object's own size, proving this is the object and not a \
             directory placeholder");
    }

    /// **The bug, for `read`.** Before this fix, `read("/photos/")` and `read("/photos")` issued a `GET`
    /// on the identical key, so reading the DIRECTORY row silently returned the FILE's bytes. Now the
    /// directory-addressed path is refused outright — reading a directory has no sane byte answer — while
    /// the bare path still reads the object normally. Breaking the fix (removing the
    /// `path_addresses_a_directory` check at the top of `read`) reds this: the "must be refused" call
    /// starts returning `Ok(b"data")`, i.e. the FILE's bytes for what was asked as a directory.
    #[test]
    fn read_on_the_colliding_keyspace_refuses_the_trailing_slash_and_reads_the_bare_path_as_the_object() {
        let (base, _keys) = spawn_a_keyspace_server_with_listbucket(&[
            "photos",
            "photos/a.jpg",
            "photos/b.jpg",
        ]);
        let provider = S3Provider::connect(&cfg(&base));

        let err = provider
            .read("/photos/")
            .expect_err("reading a path that explicitly addresses a directory must be refused, not \
                          silently answer with the same-named object's bytes");
        assert!(err.contains("directory"), "the refusal must say WHY, got: {err}");

        let bytes = provider.read("/photos").expect("the bare path reads the object");
        assert_eq!(bytes, b"data", "the bare path must still read the object's real bytes");
    }

    /// **The bug, for `delete` — the narrow slice of it CPE-1748 fixes (see its Notes for what stays
    /// CPE-1735's).** Without `s3:ListBucket` the directory-content probe fails outright, and `delete`
    /// used to fall back to "a 2xx HEAD on the key proves object-ness" with NO regard for whether the
    /// caller's path said "delete the directory" — so `delete("/photos/")` (an explicit directory
    /// delete) could silently delete the unrelated FILE `photos` the moment the probe couldn't answer.
    /// Now an explicit directory path never takes that fallback: a failed probe stays a refusal.
    /// Breaking the fix (deleting the `!path_addresses_a_directory(path, &key) &&` clause added to the
    /// HEAD-fallback condition) reds this: the "must refuse" delete instead reports `Ok`, and the `photos`
    /// key — which the caller never selected — is gone from the server's key set.
    #[test]
    fn delete_of_an_explicit_directory_path_never_falls_back_to_deleting_the_same_named_object_on_a_failed_probe()
    {
        let (base, keys) = spawn_a_keyspace_server_without_listbucket(&[
            "photos",
            "photos/a.jpg",
            "photos/b.jpg",
        ]);
        let mut provider = S3Provider::connect(&cfg(&base));

        let outcome = provider.delete("/photos/");

        // Assert the harm BEFORE unwrapping (CPE-1743 convention): if the guard ever answers `Ok`, this
        // must still red on WHICH key vanished, not on "expected an error".
        let left: Vec<String> = keys.lock().unwrap().iter().cloned().collect();
        assert_eq!(
            left,
            vec!["photos".to_string(), "photos/a.jpg".to_string(), "photos/b.jpg".to_string()],
            "the caller asked to delete the DIRECTORY \"/photos/\" — nothing must be removed, and \
             specifically the unrelated object `photos` must survive (outcome was {outcome:?})"
        );
        outcome.expect_err(
            "without s3:ListBucket the directory-content probe cannot answer, and an explicitly \
             directory-addressed delete must stay refused rather than falling back to deleting the \
             same-named object",
        );
    }

    /// **PR #903 UAT finding 3, characterised.** Deleting a key that does not exist answers `Ok` for a
    /// credential that can list (the probe comes back empty, the DELETE gets S3's idempotent 204, and
    /// `delete`'s contract is only *"that key is absent now"*) and `Err` for one that cannot (the HEAD
    /// answers 404, so nothing proves object-ness and the delete is refused).
    ///
    /// The HEAD path is the stricter of the two about the identical situation. That is defensible — the
    /// un-listable credential cannot tell "absent" from "a directory I may not enumerate" — but a caller
    /// that treats `delete` as idempotent needs to know it depends on the credential's policy.
    #[test]
    fn deleting_a_key_that_does_not_exist_is_ok_with_listbucket_and_refused_without_it() {
        let (with_base, _with_root, _r1) = spawn_s3_fixture();
        let mut can_list = S3Provider::connect(&cfg(&with_base));
        can_list.delete("/notes/never.txt").expect(
            "with s3:ListBucket the probe comes back empty and S3's DELETE is idempotent, so this is the \
             long-standing Ok",
        );

        let (without_base, _without_root, _r2) = spawn_s3_fixture_without_listbucket();
        let mut cannot_list = S3Provider::connect(&cfg(&without_base));
        let err = cannot_list
            .delete("/notes/never.txt")
            .expect_err("without it, nothing proves the key names an object, so the delete is refused");
        assert!(
            err.contains("nothing has been deleted"),
            "the refusal must still tell the user nothing happened: {err}"
        );
    }

    /// The trap the fix had to avoid. `delete` refuses a directory with content because S3 has no atomic
    /// multi-key delete; the probe is the **only** thing that tells a directory from an object. So a
    /// *denied* probe must not become a licence to do what a *successful* probe forbids — that would make
    /// the guard weaker the less the server is willing to say, and `DELETE` answers 204 for a prefix just
    /// as readily as for an object, so "deleted" would be a flat lie about a whole subtree.
    ///
    /// **CPE-1727 item 1 kept this exactly as it was**, and it is now the guard on the HEAD fallback's
    /// precondition: `photos/2024` is a pure prefix, so the HEAD answers 404 and proves nothing. Accepting
    /// any answer other than 2xx — or skipping the HEAD and deleting on the denial — reds this test with
    /// two surviving objects.
    #[test]
    fn a_denied_probe_does_not_re_enable_the_directory_delete_that_a_successful_probe_refuses() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        std::fs::create_dir_all(root.join("photos/2024")).unwrap();
        std::fs::write(root.join("photos/2024/a.jpg"), b"a").unwrap();
        std::fs::write(root.join("photos/2024/b.jpg"), b"b").unwrap();
        let mut provider = S3Provider::connect(&cfg(&base));

        let outcome = provider.delete("/photos/2024");

        assert!(
            root.join("photos/2024/a.jpg").is_file(),
            "a.jpg was deleted by a refused delete (outcome was {outcome:?})"
        );
        assert!(
            root.join("photos/2024/b.jpg").is_file(),
            "b.jpg was deleted by a refused delete (outcome was {outcome:?})"
        );

        outcome.expect_err(
            "falling back to an un-probed single-key DELETE here would return 204 and report a whole \
             subtree removed while every object in it stayed put",
        );
    }

    /// `stat`'s half of the same fix. On AWS proper this is mostly masked — without `s3:ListBucket` a HEAD
    /// answers 403 rather than 404, so `stat` returns at the HEAD and never reaches its probe — but MinIO
    /// and Ceph answer 404, which is exactly how the probe gets reached and the mystery prefix produced.
    /// The fixture answers 404 for a key with no file behind it, so it models the reachable case.
    #[test]
    fn stat_without_listbucket_names_the_probe_rather_than_the_prefix_it_invented() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        // A virtual directory: no object at the key itself, so HEAD 404s and the probe is what decides.
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("photos/a.jpg"), b"a").unwrap();
        let provider = S3Provider::connect(&cfg(&base));

        let err = provider.stat("/photos").expect_err(
            "the directory check was denied, so whether this path exists is genuinely unknown — reporting \
             'not found' would be a confident wrong answer",
        );
        assert!(
            !err.starts_with("s3: \"photos/\""),
            "the reported bug's shape: an error whose subject is a probe's prefix, not the caller's \
             path: {err}"
        );
        assert!(err.contains("ListObjectsV2"), "the error must name the probe: {err}");
        assert!(err.contains("s3:ListBucket"), "the error must name the permission that fixes it: {err}");
    }

    /// **The third probe call site — untested until the CPE-1723 review removed its wrapper and found
    /// the whole suite still green.**
    ///
    /// The PR claimed the wrapper was "wired at all three probe call sites". Two were pinned; this one
    /// was not, so Evidence Rule 1 was unmet for a third of the guard in a PR whose own body presents
    /// guard-neutralisation as its evidence. No behaviour was ever wrong — the wrapper is present and
    /// correct — but it was unprotected against a future refactor, and the coverage claim was one third
    /// wider than the evidence.
    ///
    /// This site matters more than its share suggests: `stat("/")` is plausibly the **first** thing a
    /// credential holding only `s3:GetObject` hits on connecting, so it is the first message such a user
    /// ever sees.
    #[test]
    fn stat_of_the_bucket_root_without_listbucket_also_names_the_probe() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        std::fs::write(root.join("a.jpg"), b"a").unwrap();
        let provider = S3Provider::connect(&cfg(&base));

        let err = provider
            .stat("/")
            .expect_err("the bucket-root probe was denied, so whether the bucket is reachable is unknown");
        assert!(err.contains("ListObjectsV2"), "the error must name the probe: {err}");
        assert!(err.contains("s3:ListBucket"), "the error must name the permission that fixes it: {err}");
        assert!(
            !err.starts_with("s3: \"\""),
            "and it must not report a bare empty prefix as its subject, which is what the unwrapped \
             error renders for the bucket root: {err}"
        );
    }

    /// The other half of the acceptance criterion: **the per-object operations this credential really is
    /// entitled to keep working.** None of `read`, `write` or a `stat` that finds a real object runs the
    /// probe at all, so a missing `s3:ListBucket` must be invisible to them.
    ///
    /// **CPE-1727 item 1 added `delete` to that list.** This test used to end by asserting the delete
    /// failed — the one place the old behaviour was pinned as correct. It is not correct: the credential
    /// holds `s3:DeleteObject`, the key names a real object, and a `HEAD` can prove that without
    /// `s3:ListBucket`. The assertion now measures the object leaving the server.
    #[test]
    fn the_object_operations_a_credential_without_listbucket_is_entitled_to_still_work() {
        let (base, root, _requests) = spawn_s3_fixture_without_listbucket();
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.write("/notes/todo.txt", b"buy milk").expect("write needs no s3:ListBucket");
        assert!(root.join("notes/todo.txt").is_file(), "write reported success without writing the object");

        assert_bytes_eq(
            &provider.read("/notes/todo.txt").expect("read needs no s3:ListBucket"),
            b"buy milk",
            "read without s3:ListBucket",
        );

        let entry = provider.stat("/notes/todo.txt").expect("stat of a real object needs no probe");
        assert_eq!(entry.size, 8, "the HEAD answered, so no prefix probe was ever needed");
        assert!(!entry.is_dir);

        provider.delete("/notes/todo.txt").expect(
            "delete of a real object is entitled too: the probe cannot run, but a HEAD proves the key \
             names an object, which is all the delete needs",
        );
        assert!(
            !root.join("notes/todo.txt").exists(),
            "delete returned Ok while the object is still on the server"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1723 item 6 / CPE-1727 item 2: the `start-after` third belt, and the hole that survives it.
    // ---------------------------------------------------------------------------------------------

    /// The fixture's `start-after` support, pinned **before** anything is concluded from a test that uses
    /// it (CPE-1727 item 2). The shipped fixture read only `max-keys` and `continuation-token`, so it
    /// answered a `start-after` request identically to one without — against which the belt below would
    /// have been unfalsifiable.
    ///
    /// Deliberately calls [`S3Provider::probe_prefix_after`] directly: this is a statement about the
    /// **fixture**, not about `delete`, and routing it through `delete` would let the belt's own logic
    /// decide the outcome.
    #[test]
    fn the_fixture_honours_start_after_so_the_belt_is_not_measured_against_a_server_that_ignores_it() {
        let (base, root, _requests) = spawn_s3_fixture();
        let provider = S3Provider::connect(&cfg(&base));
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("photos/.s3marker"), b"").unwrap();
        std::fs::write(root.join("photos/a.jpg"), b"a").unwrap();

        let (raw, real, _) = provider.probe_prefix_after("photos/", None).unwrap();
        assert_eq!(raw, 2, "precondition: the marker and one object are both under the prefix");
        assert_eq!(real, 1, "precondition: exactly one of them is real content");

        let (_, past_marker, _) = provider.probe_prefix_after("photos/", Some("photos/")).unwrap();
        assert_eq!(
            past_marker, 1,
            "start-after the marker must still return the object beyond it — this is the exact question \
             the delete belt asks, and a fixture that ignored the parameter would answer it the same way \
             either way"
        );

        let (_, past_object, _) =
            provider.probe_prefix_after("photos/", Some("photos/a.jpg")).unwrap();
        assert_eq!(
            past_object, 0,
            "start-after the last key must return nothing — a fixture that ignores start-after returns \
             the object again here, which is what makes this assertion the discriminating one"
        );
    }

    /// A server that **under-fills** the page (one key when two were asked for), **denies being
    /// truncated**, and **honours `start-after`** — the exact shape the third belt exists for
    /// (CPE-1727 item 2).
    ///
    /// Both of `probe_prefix`'s belts are defeated by construction: the second key never arrives, and
    /// `IsTruncated` is silent. The belt's re-list is a *different question* — under-filling a page is
    /// legal S3 latitude, so the first lie is free, but answering "nothing past the marker" while
    /// `photos/a.jpg` exists is a flat protocol violation. This server is not willing to tell that lie,
    /// which is what the belt is built to force.
    ///
    /// Returns the DELETE request lines the server received, so the test asserts on **what was sent to be
    /// destroyed** rather than on the `Result`.
    fn spawn_an_underfilling_server_that_honours_start_after() -> (String, Arc<Mutex<Vec<String>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let deletes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let deletes_thread = Arc::clone(&deletes);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let method = req.method().to_string().to_uppercase();
                let full = req.url().to_string();
                let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                let params = parse_query(raw_query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if !is_list {
                    if method == "DELETE" {
                        deletes_thread.lock().unwrap().push(raw_path.to_string());
                    }
                    let _ = req.respond(tiny_http::Response::empty(204));
                    continue;
                }
                let start_after = params
                    .iter()
                    .find(|(k, _)| k == "start-after")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                // The keyspace: the directory's own marker, and one real object beneath it.
                let rows = [
                    ("photos/", "<Contents><Key>photos/</Key><Size>0</Size></Contents>"),
                    ("photos/a.jpg", "<Contents><Key>photos/a.jpg</Key><Size>3</Size></Contents>"),
                ];
                let visible: Vec<&str> = if start_after.is_empty() {
                    // The first lie: one key however many were asked for.
                    vec![rows[0].1]
                } else {
                    rows.iter().filter(|(k, _)| *k > start_after.as_str()).map(|(_, x)| *x).collect()
                };
                // The second lie, on every page: never truncated.
                let body = format!(
                    "<?xml version=\"1.0\"?><ListBucketResult>\
                     <IsTruncated>false</IsTruncated>{}</ListBucketResult>",
                    visible.join("")
                );
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(ct));
            }
        });
        (format!("http://{addr}"), deletes)
    }

    /// **CPE-1727 item 2.** The belt catches the doubly-lying gateway that CPE-1723 closed as
    /// "no proportionate fix", and it catches it **before any DELETE leaves this process**.
    ///
    /// Removing the belt from [`FileSystemProvider::delete`] reds this test with
    /// `DELETEs sent: ["/test-bucket/photos/"]` — the marker gone, the object orphaned, and a success
    /// reported.
    #[test]
    fn an_underfilling_server_that_denies_truncation_is_caught_by_the_start_after_belt() {
        let (base, deletes) = spawn_an_underfilling_server_that_honours_start_after();
        let mut provider = S3Provider::connect(&cfg(&base));

        let result = provider.delete("/photos");

        // Asserted BEFORE the `Result`, deliberately: what matters is that nothing was sent to be
        // destroyed. Without the belt this is `["/test-bucket/photos/"]` — the marker gone and
        // photos/a.jpg left unreachable — and that is the line that must red.
        assert_eq!(
            deletes.lock().unwrap().clone(),
            Vec::<String>::new(),
            "no DELETE may be sent at all: the one this would have sent removes the marker and leaves \
             photos/a.jpg unreachable"
        );
        let err = result.expect_err(
            "the first page under-filled and denied truncation, but the re-list past the marker found \
             photos/a.jpg — so this prefix has content",
        );
        assert!(err.contains("recursive"), "the error must name the missing capability: {err}");
    }

    /// **The belt's restrictive failure mode, measured after the PR #903 review found it recorded
    /// nowhere.** Every scoping sentence in the first round of this PR named the *permissive* limit
    /// ("does not catch a server that ignores `start-after`"). This is the other one: the belt makes an
    /// **optional** `ListObjectsV2` parameter a hard dependency of `delete`, so a server that is fully
    /// permissive and entirely honest but simply does not implement `start-after` now **refuses an
    /// empty-directory delete that succeeded before the belt existed**.
    ///
    /// The fixture is deliberately the ordinary filesystem-backed one, wrapped only to reject the new
    /// parameter with `400 InvalidArgument` — so every permission is held, nothing lies, and the refusal
    /// is entirely the belt's doing. The marker surviving on disk is the measured loss.
    ///
    /// It doubles as the guard on the PR #903 review's blocking finding 1: this is the one path that
    /// produces the belt's failure message, and the message must not tell a credential that demonstrably
    /// holds `s3:ListBucket` to go and grant `s3:ListBucket`.
    ///
    /// **Refusing is kept deliberately.** Treating a failed confirmation as consent is how CPE-1723's
    /// original bug reads. But it is a trade, and CPE-1735 carries the question of what to do when a real
    /// gateway turns out not to support the parameter.
    #[test]
    fn a_server_that_rejects_start_after_now_refuses_an_empty_directory_delete_it_used_to_allow() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let query = full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                let params = parse_query(&query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if is_list && params.iter().any(|(k, _)| k == "start-after") {
                    // An honest, permissive server that simply does not implement the parameter.
                    let body = "<?xml version=\"1.0\"?><Error><Code>InvalidArgument</Code>\
                                <Message>Unsupported parameter: start-after</Message></Error>";
                    let ct = tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/xml"[..],
                    )
                    .unwrap();
                    let _ = req.respond(
                        tiny_http::Response::from_string(body).with_header(ct).with_status_code(400),
                    );
                    continue;
                }
                handle(req, &root_for_thread, None, &requests_thread);
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.mkdir("/scratch").expect("mkdir must succeed — this server denies nothing");
        assert!(root.join("scratch/.s3marker").is_file(), "precondition: the marker exists");

        let outcome = provider.delete("/scratch");

        // The measured loss: an entitled operation removed by a new guard. Asserted BEFORE unwrapping
        // the Result, so a guard that ever answers `Ok` here still reds on the marker's fate rather than
        // on "expected an error, got ()" (CPE-1743).
        assert!(
            root.join("scratch/.s3marker").is_file(),
            "the empty folder is still there — that is the cost this test exists to record \
             (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "RECORDED, NOT DESIRED: the belt cannot confirm the marker-only verdict, so it refuses — this \
             exact delete succeeds on a server that implements start-after",
        );
        assert!(
            err.contains("nothing has been deleted"),
            "the user must be told nothing happened: {err}"
        );
        assert!(
            err.contains("start-after"),
            "and told what the second request added, since that is the only actionable part: {err}"
        );
        // PR #903 review, blocking finding 1. The belt runs ONLY after the identical listing succeeded
        // with this same credential, so a permission diagnosis here is not merely unhelpful, it is false.
        assert!(
            !err.contains("granting s3:ListBucket is the fix"),
            "the credential demonstrably HAS s3:ListBucket — the same listing succeeded moments earlier — \
             so advising the user to grant it sends them to fix the one cause already ruled out: {err}"
        );
        assert!(
            err.contains("not one of the statuses S3 denies with"),
            "and it should rule the category out outright, rather than merely omitting the false \
             advice: {err}"
        );
        // Round-4 review. The wording here is deliberately scoped to what the status licenses — the
        // server did not *refuse* — rather than the stronger "this is not a permissions problem", which
        // rests on `401 | 403` being a complete list of the ways a server says no. It is not: AWS's own
        // error table maps `ExpiredToken` to 400, which would land in this arm, and the stronger sentence
        // would then be a guess dressed as a finding. Two literals standing in for a property is the
        // shape this whole ticket keeps producing.
        assert!(
            !err.contains("so it did not refuse"),
            "the 401|403 list cannot support the stronger claim — a gateway reporting an expired token \
             as 400 lands in this arm, so the sentence must stay inside what the status proves: {err}"
        );
    }

    /// **The blocking finding of the PR #903 UAT's fourth round: the `None` arm said the server never
    /// answered, when it had.**
    ///
    /// `probe_prefix_after` obtains its status at `signed_get` and then failed *after* that point —
    /// `from_utf8` on the body, or `parse_list_bucket_result` — while mapping both to `None`. So a server
    /// answering **200** with a body the crate could not read produced *"No HTTP status was read at all,
    /// so the request failed before the server could answer"*, with an appended cause describing the body
    /// it had just read. The message contradicted its own evidence, which is the third distinct instance
    /// of that shape in this one function.
    ///
    /// The reachable route is the parser one, and it is not exotic: a gateway emitting an unescaped `&`
    /// in a key — precisely the escaping CPE-1736 documents — yields a 200 that is answered and then
    /// rejected locally. This fixture uses the same shape with an invalid-UTF-8 body, which is the
    /// cheaper half to stage and exercises the identical arm.
    ///
    /// `None` now means *no status was ever obtained* — transport, or a failure before any reply. A 2xx
    /// that could not be read is its own diagnosis: the server replied, and replied successfully, and
    /// what failed was reading it. That says nothing about permissions **and nothing about
    /// `start-after`** either, which the old wording quietly implied by falling through to the arm that
    /// blames the new parameter.
    /// Every unreadable-reply shape, **each one on its own row**.
    ///
    /// The round-4 fix carried the status on two routes and the round-4 *test* covered one of them. The
    /// UAT reverted the other — `parse_list_bucket_result`'s — and the suite stayed green at 188/0. The
    /// parse route is the **more** reachable of the two (any malformed XML, any valid-UTF-8 document
    /// that is not a listing), so the half without a guard was the half more likely to come back. Hence
    /// a table: a new way for a reply to be unreadable gets a row, not a judgement call about whether
    /// the existing row already covers it.
    #[test]
    fn a_two_hundred_the_belt_cannot_read_does_not_claim_the_server_never_answered() {
        // (what the server sends as the belt's reply, what makes it unreadable)
        let bodies: &[(&[u8], &str)] = &[
            (&[0xff, 0xfe, 0xfd], "not valid UTF-8 — the `from_utf8` route"),
            (
                b"<?xml version=\"1.0\"?><ListBucketResult><Contents></ListBucketResult>",
                "valid UTF-8, malformed XML — the `parse_list_bucket_result` route the UAT found unguarded",
            ),
            (
                b"<?xml version=\"1.0\"?><html><body>proxy error</body></html>",
                "well-formed XML that is not a listing at all",
            ),
        ];

        for (body, why) in bodies {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let addr = server.server_addr().to_ip().unwrap();
            let root = fixture_root();
            let requests = Arc::new(AtomicUsize::new(0));
            let requests_thread = Arc::clone(&requests);
            let root_for_thread = root.to_path_buf();
            let body_owned = body.to_vec();
            std::thread::spawn(move || {
                for req in server.incoming_requests() {
                    let full = req.url().to_string();
                    let query =
                        full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                    let params = parse_query(&query);
                    let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                    if is_list && params.iter().any(|(k, _)| k == "start-after") {
                        // 200 OK. The server has answered, successfully; the failure is entirely on
                        // this side of the wire.
                        let _ =
                            req.respond(tiny_http::Response::from_data(body_owned.clone()));
                        continue;
                    }
                    handle(req, &root_for_thread, None, &requests_thread);
                }
            });
            let base = format!("http://{addr}");
            let mut provider = S3Provider::connect(&cfg(&base));

            provider.mkdir("/scratch").expect("mkdir must succeed — this server denies nothing");
            assert!(root.join("scratch/.s3marker").is_file(), "[{why}] precondition: marker exists");

            // **Effect before verdict.** The `Result` is captured, not unwrapped, so the assertion
            // carrying the harm is reachable. The round-5 UAT caught the reverse here: with the guard
            // removed the run stopped at `expect_err` and never reached the marker check, so the
            // assertion naming the damage could not fire — and I had reported "delete Ok, marker gone"
            // when only the first half was ever asserted. Same "assert on the bytes, not on the
            // `Result`" rule this ticket has been applying to everything else, broken in the tests
            // written to enforce it.
            let outcome = provider.delete("/scratch");

            assert!(
                root.join("scratch/.s3marker").is_file(),
                "[{why}] THE HARM: an unreadable confirmation was treated as consent and the marker \
                 was deleted (outcome was {outcome:?})"
            );
            let err = outcome
                .expect_err("the confirmation could not be read, so the verdict is unconfirmed");
            assert!(
                !err.contains("failed before any reply"),
                "[{why}] THE DEFECT: the server answered 200 and this message said no reply ever \
                 arrived — while quoting a failure to read the reply it had received. `None` must mean \
                 no status was obtained, not that one was discarded: {err}"
            );
            assert!(
                err.contains("it did reply, and successfully"),
                "[{why}] a 2xx that could not be read is its own diagnosis and must say so: {err}"
            );
            assert!(
                !err.contains("a server that does not implement it"),
                "[{why}] and must not blame start-after — a successful reply is no evidence about \
                 parameter support: {err}"
            );
        }
    }

    /// A **`206 Partial Content`** listing must not answer a question about absence.
    ///
    /// `probe_prefix_after` accepted any `2xx`, and `parse_list_bucket_result` reads a well-formed
    /// fragment as a *complete* listing — so on the belt a 206 carrying an empty listing meant "nothing
    /// past the marker" and the DELETE went out. The round-4 UAT measured `*** DELETE WENT THROUGH
    /// (Ok) ***`.
    ///
    /// This is the same hazard `signed_exchange`'s over-cap guard already refuses in words — *"refusing
    /// rather than parsing a truncated body, which can look like a complete but much shorter listing"* —
    /// arriving by status code rather than by byte count. The assertion is on the **DELETEs sent**, not
    /// on the `Result`, because the whole failure mode was a cheerful `Ok`.
    #[test]
    fn a_partial_content_listing_is_refused_rather_than_read_as_an_empty_one() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let query = full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                let params = parse_query(&query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if is_list && params.iter().any(|(k, _)| k == "start-after") {
                    let body = "<?xml version=\"1.0\"?><ListBucketResult>\
                                <IsTruncated>false</IsTruncated></ListBucketResult>";
                    let _ = req.respond(
                        tiny_http::Response::from_string(body).with_status_code(206),
                    );
                    continue;
                }
                handle(req, &root_for_thread, None, &requests_thread);
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.mkdir("/scratch").expect("mkdir must succeed — this server denies nothing");

        // Effect before verdict — see the sibling test above for why.
        let outcome = provider.delete("/scratch");

        assert!(
            root.join("scratch/.s3marker").is_file(),
            "THE DEFECT: a 206 says the reply is incomplete by definition, and an incomplete listing \
             was read as an empty one — so the marker was deleted on the strength of a fragment \
             (outcome was {outcome:?})"
        );
        let err = outcome.expect_err("a partial listing cannot confirm that a prefix is empty");
        assert!(
            err.contains("206"),
            "and the refusal must name the status that caused it: {err}"
        );
    }

    /// **The blocking finding of the PR #903 UAT's third round.** The belt's message used to assert
    /// *"this is not a permissions problem"* unconditionally, reasoning that the first listing had just
    /// succeeded with the same credential. The two listings are back to back but **not atomic**, so that
    /// is past evidence stated as a present fact — and this server is the counter-example: it allows the
    /// first listing and denies the second with `403 ExpiredToken`, the shape an STS session token
    /// produces when its expiry falls between two requests.
    ///
    /// The round-2 message contradicted itself inside one paragraph — "not a permissions problem", four
    /// lines above the server's own "the provided token has expired". This pins the fix: on a denial the
    /// message names **what actually changed** (the credential's authority, between the two requests) and
    /// says what to do about it, instead of ruling the category out.
    ///
    /// A `403` is deliberately paired here with `ExpiredToken` rather than `AccessDenied` because the
    /// expiry cliff is the *reachable* case — it needs no policy edit and no administrator, only time.
    #[test]
    fn a_denial_on_the_belt_names_the_authority_that_changed_instead_of_ruling_permissions_out() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let deletes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let deletes_thread = Arc::clone(&deletes);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let method = req.method().to_string().to_uppercase();
                let full = req.url().to_string();
                let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                let params = parse_query(raw_query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if !is_list {
                    if method == "DELETE" {
                        deletes_thread.lock().unwrap().push(raw_path.to_string());
                    }
                    let _ = req.respond(tiny_http::Response::empty(204));
                    continue;
                }
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                if params.iter().any(|(k, _)| k == "start-after") {
                    // The session token expired in the gap between the two listings.
                    let body = "<?xml version=\"1.0\"?><Error><Code>ExpiredToken</Code>\
                                <Message>The provided token has expired.</Message></Error>";
                    let _ = req.respond(
                        tiny_http::Response::from_string(body).with_header(ct).with_status_code(403),
                    );
                    continue;
                }
                // The first listing, while the credential was still good: marker only, honest.
                let body = "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated>\
                            <Contents><Key>photos/</Key><Size>0</Size></Contents></ListBucketResult>";
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        let result = provider.delete("/photos");
        assert_eq!(
            deletes.lock().unwrap().clone(),
            Vec::<String>::new(),
            "an unconfirmable marker-only verdict must send no DELETE, denial or not"
        );
        let err = result.expect_err("the confirmation was denied, so the verdict is unconfirmed");

        assert!(
            !err.contains("not a permissions problem"),
            "THE DEFECT: the server said the token expired, and the message denied the category the \
             server had just named. 'It is not X' is a claim and needs the same evidence as 'it is X': \
             {err}"
        );
        assert!(
            err.contains("differing only by the start-after parameter"),
            "on a denial the message must name what is actually established — that something changed \
             between two requests that are not atomic: {err}"
        );
        assert!(
            err.contains("session token"),
            "and name the reachable cause, since an STS expiry needs no administrator, only time: {err}"
        );
        // Round-4 UAT. The arm used to *prescribe* "the credential's authority changed; re-authenticate",
        // which is a positive claim the numeric status cannot support: a gateway that signs an unexpected
        // query parameter differently answers `403 SignatureDoesNotMatch` on an unchanged credential and
        // unchanged policy, and both prescribed actions are then wrong. The status says the server
        // refused; only the server's error code says why — and `map_s3_error` has already distinguished
        // them, so the finer evidence was being computed and discarded.
        assert!(
            err.contains("error code below names what changed"),
            "a 403 licenses 'the server refused this', not a specific cause — the message must defer to \
             the server's own error code rather than prescribing one: {err}"
        );
        assert!(
            err.contains("signature") && err.contains("clock"),
            "and must cover the non-authority denial, or it prescribes re-authentication for a request \
             that was refused on how it was formed: {err}"
        );
        assert!(
            err.contains("ExpiredToken"),
            "the server's own diagnosis must survive verbatim: {err}"
        );
        assert!(
            err.contains("nothing has been deleted"),
            "and the user still needs to know nothing happened: {err}"
        );
    }

    /// The `beyond_more` half of the belt's condition, which the PR #903 review correctly noted no test
    /// reached: a belt page carrying **no rows at all** but `IsTruncated=true` still says there is
    /// something past the marker, and must refuse.
    ///
    /// Unreachable through any fixture that answers honestly, because to get to the belt at all the first
    /// page must have shown the marker alone — so this server contradicts itself deliberately, which is
    /// exactly the class of server the belt exists for.
    #[test]
    fn a_belt_page_with_no_rows_but_is_truncated_still_refuses_the_delete() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let deletes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let deletes_thread = Arc::clone(&deletes);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let method = req.method().to_string().to_uppercase();
                let full = req.url().to_string();
                let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                let params = parse_query(raw_query);
                let is_list = params.iter().any(|(k, v)| k == "list-type" && v == "2");
                if !is_list {
                    if method == "DELETE" {
                        deletes_thread.lock().unwrap().push(raw_path.to_string());
                    }
                    let _ = req.respond(tiny_http::Response::empty(204));
                    continue;
                }
                let body = if params.iter().any(|(k, _)| k == "start-after") {
                    // No rows, but "there is more" — the only signal left, and it is enough.
                    "<?xml version=\"1.0\"?><ListBucketResult>\
                     <IsTruncated>true</IsTruncated></ListBucketResult>"
                } else {
                    "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated>\
                     <Contents><Key>photos/</Key><Size>0</Size></Contents></ListBucketResult>"
                };
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        let result = provider.delete("/photos");
        assert_eq!(
            deletes.lock().unwrap().clone(),
            Vec::<String>::new(),
            "a truncated belt page says there is something past the marker, so no DELETE may be sent"
        );
        let err = result.expect_err("IsTruncated=true on the belt page means content beyond the marker");
        assert!(err.contains("recursive"), "the error must name the missing capability: {err}");
    }

    /// **PR #903 UAT finding 2, characterised.** The same doubly-lying server one row lighter: it
    /// under-fills to **zero** rows, denies truncation, and `photos/a.jpg` really exists with no marker.
    /// That reaches the `raw_entries == 0` verdict — an ordinary object — where the belt's gate does not
    /// fire, so the delete goes through.
    ///
    /// **Milder than the marker case by construction**, which is why the gate stays where it is: the
    /// DELETE goes to `photos`, not `photos/`, and S3 answers 204 for a key that was never there, so no
    /// key that exists is destroyed. The harm is a false success. The cost of closing it would be a second
    /// `ListObjectsV2` on **every ordinary single-file delete** — see the gate comment in
    /// [`FileSystemProvider::delete`]. Predates CPE-1727 (`doomed_key`'s `else { key }` branch is on
    /// `origin/main`); recorded here so the trade is written down rather than rediscovered.
    #[test]
    fn a_zero_row_under_filler_reaches_the_object_verdict_where_the_belt_does_not_run() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let deletes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let deletes_thread = Arc::clone(&deletes);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let method = req.method().to_string().to_uppercase();
                let full = req.url().to_string();
                let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                let is_list =
                    parse_query(raw_query).iter().any(|(k, v)| k == "list-type" && v == "2");
                if !is_list {
                    if method == "DELETE" {
                        deletes_thread.lock().unwrap().push(raw_path.to_string());
                    }
                    let _ = req.respond(tiny_http::Response::empty(204));
                    continue;
                }
                // Zero rows, and not truncated — while photos/a.jpg exists. Both lies, one row lighter
                // than the marker-only liar, and that one row is what moves it out of the belt's reach.
                let body = "<?xml version=\"1.0\"?><ListBucketResult>\
                            <IsTruncated>false</IsTruncated></ListBucketResult>";
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.delete("/photos").expect(
            "if this now fails, the belt's gate has been widened past the marker-only verdict — good, but \
             the gate comment in `delete` still claims it is not, and must be updated to match",
        );
        assert_eq!(
            deletes.lock().unwrap().clone(),
            vec!["/test-bucket/photos".to_string()],
            "the DELETE goes to `photos`, NOT `photos/` — which is why this hole cannot destroy a key that \
             exists, and why the gate is a defensible trade rather than an oversight"
        );
    }

    /// The same gateway, one lie further: it **ignores `start-after`** and re-serves its marker-only page.
    /// **A characterisation test, not a guard** — it measures the hole that survives all three belts.
    ///
    /// **CPE-1727 item 4 made it filesystem-backed.** It used to assert `provider.delete(..).is_ok()`
    /// against a server with no storage behind it — the one assertion in that PR made on the `Result`
    /// rather than on an effect, and it *could not* have shown an effect, because there was nothing for a
    /// DELETE to change. Now the non-listing verbs run against a real directory through [`handle`], so the
    /// residual harm is measured rather than described: **the marker is deleted and `photos/a.jpg`
    /// survives**. That is item 4's second finding too — the doomed key is `photos/`, so the harm is a lost
    /// marker plus a false success (the folder vanishes from listings while its object remains), not the
    /// orphaning of a subtree of many objects.
    ///
    /// If someone later defends against this shape, **this test reds** and `probe_prefix`'s "what this
    /// cannot defend against" section is the thing to update.
    #[test]
    fn an_underfilling_server_that_also_denies_truncation_defeats_both_belts_and_that_is_recorded_not_fixed() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let root = fixture_root();
        std::fs::create_dir_all(root.join("photos")).unwrap();
        std::fs::write(root.join("photos/.s3marker"), b"").unwrap();
        std::fs::write(root.join("photos/a.jpg"), b"a").unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_thread = Arc::clone(&requests);
        let root_for_thread = root.to_path_buf();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let query = full.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
                let is_list = parse_query(&query).iter().any(|(k, v)| k == "list-type" && v == "2");
                if !is_list {
                    // Every other verb is served for real against `root`, so a DELETE actually removes
                    // something and the test can measure what.
                    handle(req, &root_for_thread, None, &requests_thread);
                    continue;
                }
                // One key — the directory's own marker — however many were asked for, a flat denial that
                // there is any more, and the same answer again when the belt asks with `start-after`.
                // Three lies, of which the third is the one that gets past CPE-1727's belt.
                let body = "<?xml version=\"1.0\"?><ListBucketResult>\
                            <IsTruncated>false</IsTruncated>\
                            <Contents><Key>photos/</Key><Size>0</Size></Contents></ListBucketResult>";
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(ct));
            }
        });
        let base = format!("http://{addr}");
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.delete("/photos").expect(
            "if this now fails, a defence against the gateway that ignores start-after has been added — \
             good, but probe_prefix's 'what this cannot defend against' section still claims it is \
             undefended and must be updated to match",
        );

        // The measured harm, on the server's own filesystem.
        assert!(
            !root.join("photos/.s3marker").exists(),
            "the marker survived — if it did, the delete had no effect at all and this test is no longer \
             measuring the recorded hole"
        );
        assert!(
            root.join("photos/a.jpg").is_file(),
            "the residual harm is a lost marker plus a false success, NOT an orphaned subtree: the doomed \
             key is `photos/`, so the object under the prefix is never touched"
        );
    }

    /// A server that **honours `max-keys` faithfully** and then **lies about `IsTruncated`**, always
    /// reporting `false` even when it has just cut the page short.
    ///
    /// That combination, not under-filling, is what [`S3Provider::probe_prefix`]'s `max-keys=2` defends
    /// against — a gateway that returns fewer keys than asked for returns fewer when asked for one too, so
    /// asking for more buys nothing against it. Here the second key really is in the page whenever two are
    /// requested, so the second key is the only thing that can contradict the false `IsTruncated`.
    ///
    /// The two rows are a directory's own marker followed by one real object, in S3's lexicographic order
    /// — the marker always sorts first because it is a strict prefix of everything beneath it.
    fn spawn_a_server_that_honours_max_keys_but_denies_being_truncated() -> String {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let full = req.url().to_string();
                let (_, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                let params = parse_query(raw_query);
                let max_keys: usize = params
                    .iter()
                    .find(|(k, _)| k == "max-keys")
                    .and_then(|(_, v)| v.parse().ok())
                    .unwrap_or(1000);
                let rows = [
                    "<Contents><Key>photos/</Key><Size>0</Size></Contents>",
                    "<Contents><Key>photos/a.jpg</Key><Size>3</Size></Contents>",
                ];
                let body = format!(
                    "<?xml version=\"1.0\"?><ListBucketResult>\
                     <IsTruncated>false</IsTruncated>{}</ListBucketResult>",
                    rows[..max_keys.min(rows.len())].join("")
                );
                let ct =
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..]).unwrap();
                let _ = req.respond(tiny_http::Response::from_string(body).with_header(ct));
            }
        });
        format!("http://{addr}")
    }

    /// Pins the `max-keys=2` half of [`S3Provider::probe_prefix`], which the round-2 UAT correctly noted
    /// was unverified: changing it to `1` left the whole suite green, so a future edit would ship silently.
    ///
    /// It needs a server that **misreports its own pagination** to be observable at all, because on a
    /// conforming one the `IsTruncated` belt catches the same case first — that is precisely the division
    /// of labour the two halves have, and why `probe_prefix`'s doc calls `IsTruncated` load-bearing and
    /// this a second belt. Against a server that honours `max-keys` and denies being truncated,
    /// `max-keys=1` returns the marker alone with no truncation flag, and `delete` would remove the marker
    /// and report success with `photos/a.jpg` still there. Asking for two keys is what puts the second one
    /// in the page where the false flag cannot hide it.
    #[test]
    fn a_server_that_denies_being_truncated_is_still_seen_as_a_non_empty_directory() {
        let base = spawn_a_server_that_honours_max_keys_but_denies_being_truncated();
        let mut provider = S3Provider::connect(&cfg(&base));

        let err = provider.delete("/photos").expect_err(
            "the second key asked for is what reveals this directory has content — the server under-filled \
             the page and did not set IsTruncated, so nothing else can",
        );
        assert!(err.contains("recursive"), "the error must name the missing capability: {err}");
    }

    // ---------------------------------------------------------------------------------------------
    // The bucket root, and the CPE-1689 rule that a dot segment is a real key.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn the_bucket_root_stats_as_a_directory_but_is_not_an_object_to_read_write_or_delete() {
        let (base, _root, _requests) = spawn_s3_fixture();
        let mut provider = S3Provider::connect(&cfg(&base));

        let entry = provider.stat("/").expect("the bucket root must stat");
        assert!(entry.is_dir);
        assert_eq!(entry.name, "/");

        for err in [
            provider.read("/").expect_err("the bucket is not an object to read"),
            provider.write("/", b"x").expect_err("the bucket is not an object to write"),
            provider.delete("/").expect_err("the bucket is not an object to delete"),
        ] {
            assert!(
                err.contains("bucket itself"),
                "the refusal must say the path addresses the bucket, not something vaguer: {err}"
            );
        }
        let err = provider.mkdir("/").expect_err("the bucket root needs no marker");
        assert!(err.contains("bucket root"), "{err}");
    }

    /// CPE-1689 established that dot segments are a real, distinct key and are preserved on purpose. The
    /// `is_safe_s3_leaf` guard refuses a `..` **leaf** because it cannot be surfaced as a navigable child
    /// name — a different question from whether the key is addressable, and applying it to a whole path
    /// here would refuse a key the crate deliberately signs unnormalised.
    #[test]
    fn an_object_key_keeps_its_dot_segments_and_double_slashes_instead_of_being_normalised() {
        assert_eq!(provider_path_to_object_key("/a/../b.txt").unwrap(), "a/../b.txt");
        assert_eq!(provider_path_to_object_key("/a//b.txt").unwrap(), "a//b.txt");
        assert_eq!(provider_path_to_object_key("/report%2ffinal.txt").unwrap(), "report%2ffinal.txt");
        assert_eq!(provider_path_to_object_key("/plain.txt").unwrap(), "plain.txt");
        assert!(provider_path_to_object_key("/").is_err(), "the bucket root is not an object");
        assert!(provider_path_to_object_key("").is_err(), "the empty path is not an object");
    }

    /// **The good half of the measurement the ticket demanded first** ("Test this first: the HTTP client
    /// may rewrite the path you signed"). `crates/s3`'s "one construction, so the URL and the signature
    /// cannot disagree" guarantee ends at the crate boundary, and this is the first ticket to cross it.
    ///
    /// Two of the three shapes the ticket named survive intact: an **empty path segment** (`//`, which S3
    /// treats as a real key byte and CPE-1689 established must not collapse) and a **percent-encoded
    /// slash** (`%2F` in the key, which this crate escapes to `%252F` on the wire). The recorder reports
    /// the raw request target off the request line — what actually left the process — compared against the
    /// exact string that was signed. Nothing here is inferred from `ureq`'s source.
    #[test]
    fn a_key_with_a_double_slash_and_a_percent_encoded_slash_reaches_the_wire_intact() {
        let (base, seen) = spawn_a_request_line_recorder();
        let key = "b//c%2Fd.txt";
        let signed_path = cfg(&base).object_target(key).unwrap().encoded_path;
        assert_eq!(
            signed_path, "/test-bucket/b//c%252Fd.txt",
            "precondition: the signer must escape the literal % and keep the empty segment"
        );

        let provider = S3Provider::connect(&cfg(&base));
        let _ = provider.read(&format!("/{key}"));

        let lines = seen.lock().unwrap().clone();
        assert_eq!(lines.len(), 1, "exactly one request should have been sent: {lines:?}");
        assert_eq!(
            lines[0], signed_path,
            "the client rewrote the path between signing and sending. SIGNED {:?}, SENT {:?} — a real \
             server would answer SignatureDoesNotMatch with nothing to say why",
            signed_path, lines[0]
        );
    }

    /// **The bad half, and the ticket's hypothesis confirmed.** The PR #868 reviewer flagged this and
    /// explicitly labelled it unverified, being offline. It reproduces exactly.
    ///
    /// Measured with the guard removed, this same rig reported:
    ///
    /// ```text
    /// SIGNED "/test-bucket/a/../b//c%252Fd.txt", SENT "/test-bucket/b//c%252Fd.txt"
    /// ```
    ///
    /// `ureq` 2.12.1 parses every URL through the `url` crate, which implements WHATWG URL parsing, which
    /// resolves dot segments as part of parsing — so the rewrite happens before `ureq` has any say in it.
    /// The request would be signed for `a/../b//c%2Fd.txt` and sent for `b//c%2Fd.txt`, and a real server
    /// would answer `SignatureDoesNotMatch` with nothing in the message to say why.
    ///
    /// So the key is refused, by name, **before anything is sent** — the request counter proves it — rather
    /// than normalised (which would silently address a different object, the failure CPE-1689 exists to
    /// prevent) or sent anyway (which would produce the opaque 403 this slice keeps working to eliminate).
    #[test]
    fn a_key_with_a_dot_segment_is_refused_because_ureq_resolves_it_away_before_sending() {
        let (base, seen) = spawn_a_request_line_recorder();
        let provider = S3Provider::connect(&cfg(&base));

        let outcome = provider.read("/a/../b.txt");

        // Assert the harm BEFORE unwrapping the Result (CPE-1743): if the guard ever answers `Ok`, the
        // run must still reach this and red on the request that actually reached the server, not
        // on "expected an error".
        assert_eq!(
            seen.lock().unwrap().len(),
            0,
            "the guard must fire BEFORE the request leaves the process — a request reached the server \
             despite the refusal (outcome was {outcome:?})"
        );

        let err = outcome
            .expect_err("a key ureq would rewrite must be refused, not sent under a mismatched signature");
        assert!(err.contains("dot segment"), "the error must name what it found: {err}");
        assert!(err.contains("SignatureDoesNotMatch"), "the error must name the failure it prevents: {err}");
        assert!(err.contains("CPE-1721"), "the error must point at the follow-up that would fix it: {err}");

        // Every verb goes through the same one guard, so none of them can miss it. Capture all four
        // outcomes BEFORE asserting anything, so the shared request-count assertion runs before any
        // one verb's `is_err()` check can mask what the others did.
        let mut provider = provider;
        let stat_outcome = provider.stat("/a/./b.txt");
        let write_outcome = provider.write("/a/../b.txt", b"x");
        let delete_outcome = provider.delete("/a/../b.txt");
        let mkdir_outcome = provider.mkdir("/a/../b");

        assert_eq!(
            seen.lock().unwrap().len(),
            0,
            "one of the other verbs sent a request whose path ureq would have rewritten (stat was \
             {stat_outcome:?}, write was {write_outcome:?}, delete was {delete_outcome:?}, mkdir was \
             {mkdir_outcome:?})"
        );

        assert!(stat_outcome.is_err(), "stat must refuse a single-dot segment too");
        assert!(write_outcome.is_err(), "write must refuse it too");
        assert!(delete_outcome.is_err(), "delete must refuse it too");
        assert!(mkdir_outcome.is_err(), "mkdir must refuse it too");
    }

    #[test]
    fn the_url_dot_segment_rule_matches_the_whatwg_definition_including_the_percent_encoded_forms() {
        for dotty in [".", "..", "%2e", "%2E", "%2e%2e", ".%2e", "%2e.", "%2E%2E"] {
            assert!(is_url_dot_segment(dotty), "{dotty:?} is a dot segment the URL parser resolves away");
        }
        for ordinary in ["", "a", "...", ".txt", "a.b", "..a", "a..", "%252e", "%2f"] {
            assert!(!is_url_dot_segment(ordinary), "{ordinary:?} is an ordinary segment and must be sent");
        }
    }

    /// Records the raw request target from each request line and answers `200` with an empty body — a
    /// measurement rig, not a fixture: it never looks at the filesystem, so what it reports is exactly what
    /// the client put on the wire.
    fn spawn_a_request_line_recorder() -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_thread = Arc::clone(&seen);
        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                seen_thread.lock().unwrap().push(req.url().to_string());
                let _ = req.respond(tiny_http::Response::empty(200));
            }
        });
        (format!("http://{addr}"), seen)
    }

    // ---------------------------------------------------------------------------------------------
    // The single-PUT ceiling, and dispatch through the channel production actually uses.
    // ---------------------------------------------------------------------------------------------

    #[test]
    fn the_single_put_ceiling_refuses_one_byte_past_five_gibibytes_and_no_sooner() {
        assert_eq!(MAX_SINGLE_PUT_BYTES, 5 * 1024 * 1024 * 1024);
        assert!(!too_big_for_single_put(0));
        assert!(!too_big_for_single_put(MAX_SINGLE_PUT_BYTES - 1));
        assert!(
            !too_big_for_single_put(MAX_SINGLE_PUT_BYTES),
            "exactly the ceiling is still a legal single PUT — refusing it would reject a legal upload on \
             our side, the one failure direction this refusal must not have"
        );
        assert!(too_big_for_single_put(MAX_SINGLE_PUT_BYTES + 1));
    }

    /// **Rule: verify through the channel the real caller uses.** CPE-1704 burned four rounds on fixes that
    /// were correct at a boundary production never goes through — including one where an *inherent* method
    /// shadowed a trait method, so every test passed on the concrete type while `crates/vfs`, holding
    /// `&dyn FileSystemProvider`, silently got the trait's default. None of the six ops added here has an
    /// inherent twin, but "none today" is not a property a test can rely on, so every one of them is
    /// exercised once through a trait object.
    ///
    /// # CPE-1723 item 4: the rename assertion here used to measure nothing
    ///
    /// It was `provider.rename("/dyn/x", "/dyn/y").is_err()`, on a source key **that does not exist**. A
    /// fully working copy-then-delete emulation — the exact thing the refusal exists to prevent — reads
    /// `/dyn/x`, gets a 404, and returns `Err`. So the assertion passed just as happily on the emulation as
    /// on the refusal, which is the definition of a test that cannot fail for the right reason. The
    /// reviewer substituted one and the test stayed green.
    ///
    /// It now renames a source that **really is there**, so a working emulation returns `Ok` and reds it,
    /// and it asserts on what the user ends up holding — source still present, destination never created,
    /// and not one request on the wire — rather than on the `Result`. That last one is the assertion that
    /// also catches the *dangerous* emulation: one whose copy lands and which then returns an
    /// honest-looking `Err`, leaving two objects where the user believes there is one. `expect_err` is
    /// satisfied by that variant; "zero requests were sent" is not.
    #[test]
    fn every_object_op_works_through_a_trait_object_the_way_production_holds_the_provider() {
        let (base, root, requests) = spawn_s3_fixture();
        let mut concrete = S3Provider::connect(&cfg(&base));
        let provider: &mut dyn FileSystemProvider = &mut concrete;

        provider.write("/dyn/a.txt", b"hello").expect("write through dyn");
        assert_bytes_eq(&provider.read("/dyn/a.txt").expect("read through dyn"), b"hello", "read via dyn");
        assert_eq!(provider.stat("/dyn/a.txt").expect("stat through dyn").size, 5);

        provider.mkdir("/dyn/sub").expect("mkdir through dyn");
        assert!(provider.stat("/dyn/sub").expect("stat a dir through dyn").is_dir);

        provider.delete("/dyn/a.txt").expect("delete through dyn");
        assert!(!root.join("dyn/a.txt").exists(), "delete through dyn left the object in place");

        provider.write("/dyn/src.txt", b"payload").expect("the rename source must really exist");
        let before = requests.load(Ordering::Relaxed);
        let outcome = provider.rename("/dyn/src.txt", "/dyn/dst.txt");

        // Assert on the harm BEFORE unwrapping the Result (CPE-1743): if the guard ever answers `Ok`
        // through the trait object, the run must still reach these and red on the damage, not stop at
        // `expect_err`.
        assert_eq!(
            requests.load(Ordering::Relaxed),
            before,
            "rename reached the network through dyn: an emulation whose copy lands and which then reports \
             an honest-looking error still leaves the user two objects believing they have one, and only \
             'no request was sent' can tell that apart from a real refusal (outcome was {outcome:?})"
        );
        assert!(
            root.join("dyn/src.txt").is_file(),
            "the source object was moved by a refused rename (outcome was {outcome:?})"
        );
        assert!(
            !root.join("dyn/dst.txt").exists(),
            "a destination object was created by a refused rename (outcome was {outcome:?})"
        );

        let err = outcome.expect_err(
            "rename must refuse through dyn too — and with a source that exists, a working copy-then-delete \
             emulation would return Ok here instead",
        );
        assert!(err.contains("no rename"), "the refusal must say what it is refusing: {err}");

        assert!(!provider.capabilities().supports_rename);
        assert!(!provider.capabilities().has_real_dirs);
    }

    // ---------------------------------------------------------------------------------------------
    // CPE-1722 / CPE-1721: an S3 key is an opaque byte string, not a filesystem path.
    // ---------------------------------------------------------------------------------------------

    /// A **key-exact** object store: a `HashMap` from the real S3 key to its bytes, with no filesystem
    /// anywhere in it.
    ///
    /// # Why the existing [`handle`] fixture could not be used for CPE-1722, stated plainly
    ///
    /// [`handle`] maps keys onto `std::fs` (`root.join(path.trim_start_matches('/'))`), and **the
    /// filesystem is the exact thing that performs the normalisation under test**. Every key shape this
    /// ticket is about collapses before it reaches disk: `Path::new("a//b")` and `Path::new("a/./b")` both
    /// have the components `a`, `b`; `a/../b` resolves to `b` and escapes the directory; `trim_start_matches`
    /// eats the leading slash that distinguishes `/a.txt` from `a.txt`; a key of only slashes lands on the
    /// root itself; and no filesystem holds a name ending in the separator. A round-trip test written against
    /// that fixture would pass **because the fixture never produced the interesting case** — the harness would
    /// be answering about `a/b` while the test believed it was asking about `a//b`.
    ///
    /// This is the same class of limit CPE-1736 already filed against [`handle`] (it never percent-decodes an
    /// object path, and interpolates key text into XML unescaped), and it is why this store decodes the
    /// request path into a genuine key and stores it under exactly those bytes. It is deliberately small: the
    /// five verbs this crate sends, no sentinels, no pagination.
    /// The bucket contents of a [`spawn_key_exact_store`]: real S3 key -> the object's bytes.
    type KeyExactBucket = Arc<Mutex<std::collections::BTreeMap<String, Vec<u8>>>>;

    fn spawn_key_exact_store() -> (String, KeyExactBucket) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let store: Arc<Mutex<std::collections::BTreeMap<String, Vec<u8>>>> = Default::default();
        let store_thread = Arc::clone(&store);
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let method = req.method().to_string().to_uppercase();
                let full = req.url().to_string();
                let (raw_path, raw_query) = full.split_once('?').unwrap_or((full.as_str(), ""));
                // The request path is `/{bucket}` + the percent-encoded key. Decoding it is what makes
                // this store key-exact and is precisely what `handle` does not do.
                let after_bucket =
                    raw_path.strip_prefix(&format!("/{TEST_BUCKET}")).unwrap_or(raw_path);
                let key = percent_decode(after_bucket.strip_prefix('/').unwrap_or(after_bucket));
                let params = parse_query(raw_query);
                let param = |n: &str| params.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());

                if method == "GET" && param("list-type") == Some("2") {
                    let prefix = param("prefix").unwrap_or("");
                    let mut contents: Vec<(String, usize)> = Vec::new();
                    let mut common: BTreeSet<String> = BTreeSet::new();
                    for (k, v) in store_thread.lock().unwrap().iter() {
                        let Some(rest) = k.strip_prefix(prefix) else { continue };
                        match rest.find('/') {
                            // `delimiter=/` rolls everything below this level up into a CommonPrefix.
                            Some(i) => {
                                common.insert(format!("{prefix}{}", &rest[..=i]));
                            }
                            None => contents.push((k.clone(), v.len())),
                        }
                    }
                    let mut xml = String::from(
                        "<?xml version=\"1.0\"?><ListBucketResult \
                         xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><IsTruncated>false\
                         </IsTruncated>",
                    );
                    for (k, n) in contents {
                        xml.push_str(&format!("<Contents><Key>{k}</Key><Size>{n}</Size></Contents>"));
                    }
                    for p in common {
                        xml.push_str(&format!("<CommonPrefixes><Prefix>{p}</Prefix></CommonPrefixes>"));
                    }
                    xml.push_str("</ListBucketResult>");
                    let ct =
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/xml"[..])
                            .unwrap();
                    let _ = req.respond(tiny_http::Response::from_string(xml).with_header(ct));
                    continue;
                }

                match method.as_str() {
                    "PUT" => {
                        let mut body = Vec::new();
                        let _ = req.as_reader().read_to_end(&mut body);
                        store_thread.lock().unwrap().insert(key, body);
                        let _ = req.respond(tiny_http::Response::empty(200));
                    }
                    "GET" => match store_thread.lock().unwrap().get(&key) {
                        Some(v) => {
                            let _ = req.respond(tiny_http::Response::from_data(v.clone()));
                        }
                        None => {
                            let _ = req.respond(tiny_http::Response::empty(404));
                        }
                    },
                    "HEAD" => match store_thread.lock().unwrap().get(&key) {
                        Some(v) => {
                            let len = tiny_http::Header::from_bytes(
                                &b"Content-Length"[..],
                                v.len().to_string().as_bytes(),
                            )
                            .unwrap();
                            let _ =
                                req.respond(tiny_http::Response::empty(200).with_header(len));
                        }
                        None => {
                            let _ = req.respond(tiny_http::Response::empty(404));
                        }
                    },
                    "DELETE" => {
                        store_thread.lock().unwrap().remove(&key);
                        let _ = req.respond(tiny_http::Response::empty(204));
                    }
                    _ => {
                        let _ = req.respond(tiny_http::Response::empty(405));
                    }
                }
            }
        });
        (format!("http://{addr}"), store)
    }

    /// **CPE-1722, to the wire** — the acceptance criterion asks for the chosen behaviour driven through
    /// the request-line recorder, "asserting the sent path — not merely that a helper returns a string".
    ///
    /// Every row is a provider path whose key the old `trim_matches('/')` collapsed onto a *different*
    /// object. Restore that trim and this reds on the first row with the exact wire path it produced.
    #[test]
    fn the_slashes_at_both_ends_of_a_provider_path_reach_the_wire_as_key_bytes() {
        // (provider path, the request target the key must produce)
        let cases: &[(&str, &str)] = &[
            // The ticket's headline case: `//report.pdf` must NOT address `report.pdf`.
            ("//report.pdf", "/test-bucket//report.pdf"),
            ("///deep.txt", "/test-bucket///deep.txt"),
            // Interior slashes were already preserved (CPE-1689); pinned here so the new rule cannot
            // regress them while fixing the ends.
            ("/a//b.txt", "/test-bucket/a//b.txt"),
            // A key that genuinely ends in a slash is spelled with the slash doubled.
            ("/trail.txt//", "/test-bucket/trail.txt/"),
            // A key that is nothing but slashes.
            ("///", "/test-bucket//"),
            ("////", "/test-bucket///"),
            // The ordinary shapes must be untouched by all of the above.
            ("/plain.txt", "/test-bucket/plain.txt"),
            ("/photos/2024/x.jpg", "/test-bucket/photos/2024/x.jpg"),
        ];

        for (path, want) in cases {
            let (base, seen) = spawn_a_request_line_recorder();
            let provider = S3Provider::connect(&cfg(&base));
            let outcome = provider.read(path);

            let lines = seen.lock().unwrap().clone();
            assert_eq!(
                lines.len(),
                1,
                "read({path:?}) should have sent exactly one request (outcome was {outcome:?})"
            );
            assert_eq!(
                lines[0], *want,
                "read({path:?}) addressed the wrong object. The key is an opaque byte string: a slash \
                 at either end is a key byte, and collapsing it silently retargets the request"
            );
        }
    }

    /// The same rule for the verb that made this a data-loss bug rather than a lookup miss: `write`.
    ///
    /// Asserts on **what the server ends up holding**, not on the `Result` — the whole failure was a
    /// `write` that returned `Ok` having overwritten a different object.
    #[test]
    fn writing_to_a_doubled_leading_slash_creates_a_second_object_instead_of_overwriting_the_first() {
        let (base, store) = spawn_key_exact_store();
        let mut provider = S3Provider::connect(&cfg(&base));

        provider.write("/report.pdf", b"the original").expect("write the ordinary key");
        provider.write("//report.pdf", b"the impostor").expect("write the leading-slash key");

        let held = store.lock().unwrap().clone();
        assert_eq!(
            held.keys().cloned().collect::<Vec<_>>(),
            vec!["/report.pdf".to_string(), "report.pdf".to_string()],
            "these are two different S3 keys and the server must be holding both — before CPE-1722 the \
             second write overwrote the first and the bucket held one object"
        );
        assert_bytes_eq(
            held.get("report.pdf").unwrap(),
            b"the original",
            "the ordinary key's object was overwritten by a write to a different key",
        );
        assert_bytes_eq(held.get("/report.pdf").unwrap(), b"the impostor", "the leading-slash key");

        // And the reads come back to the right caller, not crossed over.
        assert_bytes_eq(&provider.read("/report.pdf").unwrap(), b"the original", "read the ordinary key");
        assert_bytes_eq(&provider.read("//report.pdf").unwrap(), b"the impostor", "read the other one");
    }

    /// **The Foreman's round-trip**: every legal-in-S3 key shape this crate can reach must survive
    /// create → list → stat → read → delete, against a store that keeps keys byte-exact.
    ///
    /// The two dot-segment shapes (`a/./b`, `a/../b`) are deliberately absent and are covered by
    /// [`the_dot_segment_refusal_now_covers_list_too_so_no_folder_lists_children_it_cannot_open`]; they are
    /// unreachable through `ureq` 2 for the reason recorded on [`guard_path_survives_the_client`].
    #[test]
    fn every_reachable_legal_key_shape_round_trips_create_list_stat_read_delete() {
        // (provider path, the S3 key it must address, the prefix path that lists it, the leaf name that
        // listing must show — `None` for a key with no displayable leaf, which must be *filtered and
        // counted* rather than shown under an invented name)
        let cases: &[(&str, &str, &str, Option<&str>)] = &[
            ("/a//b.txt", "a//b.txt", "/a//", Some("b.txt")),
            ("//lead.txt", "/lead.txt", "//", Some("lead.txt")),
            // A key ending in a slash is the directory-marker convention, so `delimiter=/` rolls it up
            // as a CommonPrefix — it lists as the *directory* `trail.txt`, which is what it is.
            ("/trail.txt//", "trail.txt/", "/", Some("trail.txt")),
            // A key that is nothing but slashes: addressable, but its leaf is the empty string, so
            // `is_safe_s3_leaf` (CPE-1704) correctly declines to surface it as a navigable child name.
            ("///", "/", "/", None),
            ("/plain.txt", "plain.txt", "/", Some("plain.txt")),
        ];

        for (path, want_key, list_at, want_leaf) in cases {
            let (base, store) = spawn_key_exact_store();
            let mut provider = S3Provider::connect(&cfg(&base));
            let body = format!("body for {path}").into_bytes();

            provider.write(path, &body).unwrap_or_else(|e| panic!("write({path:?}): {e}"));
            assert_eq!(
                store.lock().unwrap().keys().cloned().collect::<Vec<_>>(),
                vec![want_key.to_string()],
                "write({path:?}) stored the wrong key"
            );

            let entry = provider.stat(path).unwrap_or_else(|e| panic!("stat({path:?}): {e}"));
            assert_eq!(
                entry.size,
                body.len() as u64,
                "stat({path:?}) reported a size for something other than the object just written"
            );
            // The whole point of this ticket is that the two projections agree, so the name `stat`
            // reports must be the name the listing shows for the same object. A key ending in `/` is a
            // shape only CPE-1722 makes reachable, and `rsplit('/')` on one yields `""`.
            assert_eq!(
                entry.name,
                want_leaf.unwrap_or("").to_string(),
                "stat({path:?}) named the object differently from how list({list_at:?}) shows it"
            );

            let got = provider.read(path).unwrap_or_else(|e| panic!("read({path:?}): {e}"));
            assert_bytes_eq(&got, &body, &format!("read({path:?})"));

            // `list` reaches the same key through the QUERY STRING rather than the path, so it is a
            // genuinely independent check that the two agree about where this key lives.
            let (entries, filtered) =
                provider.list_with_filtered_count(list_at).unwrap_or_else(|e| panic!("list: {e}"));
            let names: Vec<&String> = entries.iter().map(|e| &e.name).collect();
            match want_leaf {
                Some(leaf) => assert!(
                    entries.iter().any(|e| e.name == *leaf),
                    "list({list_at:?}) did not surface {leaf:?} for key {want_key:?}; it returned {names:?}"
                ),
                // A key of only slashes has an empty leaf, so it cannot be surfaced as a navigable child
                // name. What it must NOT do is appear under an invented one.
                //
                // CPE-1801: this used to be a NAMED LIMITATION pinned at `filtered == 0` —
                // `parse_list_bucket_result` dropped an empty `<CommonPrefixes>` leaf with a bare
                // `continue` *before* the `filtered_count` arm, so the key was invisible AND uncounted,
                // quietly breaking CPE-1704's `entries.len() + filtered_count` delete-path total. Fixed
                // by routing the empty leaf through the same `is_safe_s3_leaf` refusal as any other
                // unsafe leaf (empty is one of that guard's own arms) instead of a separate early
                // `continue` — so it is now counted, deliberately, at `1`. Still invisible: a slashes-only
                // key genuinely has no displayable name, and surfacing one under an invented name would
                // make `list` disagree with `stat` (which also reports `""` for this key) — exactly the
                // two-projections-disagree bug this whole test exists to catch. Updated here rather than
                // deleted so the guard keeps doing its job: touch this arithmetic again without reading
                // this comment, and it reds.
                None => {
                    assert!(
                        names.is_empty(),
                        "key {want_key:?} has no displayable leaf, so list({list_at:?}) must not show \
                         it under an invented name; it returned {names:?}"
                    );
                    assert_eq!(
                        filtered, 1,
                        "CPE-1801: a slashes-only key's empty CommonPrefixes leaf must be counted into \
                         filtered_count, not dropped uncounted, so entries.len() + filtered_count stays \
                         a true total"
                    );
                }
            }

            provider.delete(path).unwrap_or_else(|e| panic!("delete({path:?}): {e}"));
            assert!(
                store.lock().unwrap().is_empty(),
                "delete({path:?}) left the bucket holding {:?} — it removed a different key, or none",
                store.lock().unwrap().keys().collect::<Vec<_>>()
            );
        }
    }

    /// The grammar, as one table, at the boundary itself. [`provider_path_to_key_prefix`] is asserted
    /// alongside [`provider_path_to_object_key`] on every row because CPE-1722's first acceptance
    /// criterion is that the two **agree** — `list`/`mkdir` and `stat`/`read`/`write`/`delete` must not
    /// end up with two path grammars inside one provider.
    #[test]
    fn one_path_grammar_decides_both_the_object_key_and_the_prefix_key() {
        // (provider path, object key or None for the bucket root, prefix key)
        let cases: &[(&str, Option<&str>, &str)] = &[
            ("/a.txt", Some("a.txt"), "a.txt/"),
            ("/a.txt/", Some("a.txt"), "a.txt/"),
            ("//a.txt", Some("/a.txt"), "/a.txt/"),
            ("///a.txt", Some("//a.txt"), "//a.txt/"),
            ("/a//b.txt", Some("a//b.txt"), "a//b.txt/"),
            ("/a.txt//", Some("a.txt/"), "a.txt//"),
            ("///", Some("/"), "//"),
            ("////", Some("//"), "///"),
            ("/photos/2024", Some("photos/2024"), "photos/2024/"),
            ("photos/2024/", Some("photos/2024"), "photos/2024/"),
            // Dot segments are ordinary key bytes at this layer; only the transport refuses them.
            ("/a/../b.txt", Some("a/../b.txt"), "a/../b.txt/"),
            // The bucket root has no object key. `"//"` is the one path where the two projections
            // legitimately differ: no object key (that key would be zero-length), but a real prefix —
            // the virtual directory that holds a key like `/lead.txt`.
            ("", None, ""),
            ("/", None, ""),
            ("//", None, "/"),
        ];

        for (path, want_key, want_prefix) in cases {
            match want_key {
                Some(k) => assert_eq!(
                    provider_path_to_object_key(path).as_deref(),
                    Ok(*k),
                    "object key for {path:?}"
                ),
                None => assert!(
                    provider_path_to_object_key(path).is_err(),
                    "{path:?} is the bucket root, which is not an object"
                ),
            }
            assert_eq!(provider_path_to_key_prefix(path), *want_prefix, "prefix key for {path:?}");
        }
    }

    /// CPE-1722's first acceptance criterion is that the grammar is decided **once**, with `list`/`mkdir`
    /// and `stat`/`read`/`write`/`delete` agreeing. A table can only assert that on the rows someone
    /// thought to write down, so the agreement is asserted here as a **property** over a generated
    /// cross-product of slash shapes: wherever a path has an object key, its prefix key must be exactly
    /// that key plus one slash.
    ///
    /// Give either helper back its own independent `trim_matches('/')` and this reds immediately.
    #[test]
    fn the_object_key_and_the_prefix_key_agree_on_every_path_that_has_both() {
        let slashes = ["", "/", "//", "///"];
        let bodies = ["", "a.txt", "a/b.txt", "a//b.txt", "a/../b.txt", "photos"];
        let mut checked = 0usize;
        for lead in slashes {
            for body in bodies {
                for trail in slashes {
                    let path = format!("{lead}{body}{trail}");
                    if let Ok(key) = provider_path_to_object_key(&path) {
                        assert_eq!(
                            provider_path_to_key_prefix(&path),
                            format!("{key}/"),
                            "the two helpers disagree about {path:?} — two path grammars inside one \
                             provider is exactly what CPE-1722 forbids"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 50, "the property must actually have been exercised, not vacuously true");
    }

    /// **CPE-1721's folded-in extra criterion**: "a folder must not list children that the same provider
    /// will refuse to open."
    ///
    /// `list` puts the prefix in the query string, which `ureq`/`url` does not normalise, so this used to
    /// return a perfectly browsable folder — while `stat`/`read`/`write` put the key in the path, which
    /// *is* normalised, so every child of that folder was refused. Now the same guard decides both.
    #[test]
    fn the_dot_segment_refusal_now_covers_list_too_so_no_folder_lists_children_it_cannot_open() {
        let (base, seen) = spawn_a_request_line_recorder();
        let provider = S3Provider::connect(&cfg(&base));

        let outcome = provider.list_with_filtered_count("/a/../b");

        // The harm first (CPE-1743): if the guard stops firing, red on the request that reached the
        // server rather than on "expected an error".
        assert_eq!(
            seen.lock().unwrap().len(),
            0,
            "list must refuse before sending: it would otherwise hand back a browsable folder whose \
             every child this same provider then declines to open (outcome was {outcome:?})"
        );
        let err = outcome.expect_err("a prefix whose children are unopenable must be refused");
        assert!(err.contains("could not open"), "the error must say what the asymmetry is: {err}");
        assert!(err.contains("dot segment"), "the error must name what it found: {err}");
        assert!(err.contains("CPE-1721"), "the error must point at the fix: {err}");

        // The object verbs still refuse the same shape. Driven against the key-exact store, NOT the
        // request-line recorder: the recorder answers nothing parseable, so `is_err()` against it holds
        // with or without the guard and would have been coverage this test did not provide. Asserting
        // the message distinguishes a real refusal from a transport failure.
        let (base2, store) = spawn_key_exact_store();
        let ok = S3Provider::connect(&cfg(&base2));
        let child = ok.read("/a/../b/c.txt").expect_err("the child was openable after all");
        assert!(child.contains("dot segment"), "a refusal, not a transport failure: {child}");
        assert!(store.lock().unwrap().is_empty(), "a refused read must not have reached the store");
        // And an ordinary prefix is unaffected.
        ok.list_with_filtered_count("/a/b").expect("an ordinary prefix must still list");
    }

    /// The negative control for the fixture claim above, so "the filesystem-backed fixture cannot serve
    /// these keys" is a measurement in the test suite rather than a sentence in a doc comment.
    ///
    /// # This test is deliberately written against the REAL fixture
    ///
    /// An earlier version asserted properties of `std::path::Path` with a locally re-implemented copy of
    /// [`handle`]'s join expression. It contained no repo symbol at all, so **no change to `crates/s3`
    /// could have turned it red** — including the very change it claims to be watching for. Its own
    /// docstring promised the opposite ("if a future change makes this stop collapsing, this reds"),
    /// which made it worse than absent: it certified the claim justifying [`spawn_key_exact_store`]'s
    /// existence while being incapable of noticing that claim going stale.
    ///
    /// So it now drives [`spawn_s3_fixture`] itself. Two writes to two genuinely distinct S3 keys land
    /// on **one** file, and the first object's bytes are gone — which is exactly the silent overwrite
    /// CPE-1722 fixes in the provider, reproduced here in the *fixture* to prove the harness cannot tell
    /// the two keys apart. The day someone gives that fixture key-exactness, this reds and the key-exact
    /// store can be retired in favour of the simpler one.
    #[test]
    fn the_filesystem_backed_fixture_provably_cannot_represent_these_key_shapes() {
        let (base, root, _requests) = spawn_s3_fixture();
        let mut provider = S3Provider::connect(&cfg(&base));

        // Two different S3 keys — `report.pdf` and `/report.pdf`. The provider now addresses them
        // correctly (that is CPE-1722, pinned by the key-exact tests above); this is about what the
        // `std::fs`-backed fixture can HOLD once they arrive.
        provider.write("/report.pdf", b"the original").expect("write the ordinary key");
        provider.write("//report.pdf", b"the impostor").expect("write the leading-slash key");

        let files: Vec<String> = std::fs::read_dir(AsRef::<Path>::as_ref(&root))
            .expect("the fixture root must be readable")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            files,
            vec!["report.pdf".to_string()],
            "the fixture was handed two distinct S3 keys and should be shown to hold only one file — if \
             it now holds both, it has become key-exact and `spawn_key_exact_store` is redundant"
        );
        assert_bytes_eq(
            &std::fs::read(root.join("report.pdf")).expect("the one file must exist"),
            b"the impostor",
            "the second key overwrote the first ON DISK: the filesystem is the thing that collapses \
             these keys, which is why a round-trip test built on this fixture would pass without ever \
             producing the interesting case",
        );
    }
}
