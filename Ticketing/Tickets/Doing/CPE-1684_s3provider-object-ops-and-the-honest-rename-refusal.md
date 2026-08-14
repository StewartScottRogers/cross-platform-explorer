---
id: CPE-1684
title: "S3Provider object ops — stat/read/write/delete/mkdir, and rename refused honestly rather than faked"
type: Feature
status: Doing
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1503
estimate: M
created: 2026-08-12
closed:
---

## What

The remaining `FileSystemProvider` ops on `S3Provider`: `stat`, `read`, `write`, `delete`, `mkdir`, and
`rename`. CPE-1683 covers `list`; this is everything else the trait requires.

## The rename decision, and why it is the interesting part of this ticket

S3 has no rename. The tempting implementation is copy-then-delete, and it is a trap: it is not atomic, it
is O(size) rather than O(1), it silently rewrites storage class and metadata, and — the part that matters —
if the delete fails after the copy succeeds the user now has two copies and believes they have one, while
if the copy half-fails on a large object they may have neither.

So `rename` returns a clear error saying S3 cannot rename, and `capabilities().supports_rename` is
`false` so callers can see that before they try. `ProviderCapabilities` exists precisely so a backend can
say what it cannot do; the doc comment on it names a future S3 provider as the first expected user of
`supports_rename = false`. This ticket is that user.

Refusing is the honest answer. A copy-then-delete that presents itself as a rename is a confident wrong
answer, which is the failure mode this crew keeps writing tickets about.

## Scope

- `stat` → HEAD on the key; a missing key is a clear not-found, distinct from a permission failure
  (CPE-1682 already draws that line — do not redraw it here).
- `read` → GET. Bounded: never buffer an unbounded remote object into memory in one call. `cpe-ftp`
  settled the convention with fixed 64 KiB chunks rather than one `read_to_end`; match it.
- `write` → PUT of the whole body. Multipart upload is explicitly **out of scope** — the trait already
  hands us a complete `&[u8]`, so the 5 GB single-PUT ceiling is not the binding constraint. Note the
  ceiling in a comment; do not build for it.
- `delete` → DELETE on the key. Deleting a virtual directory means deleting the keys under that prefix —
  decide and document whether that is supported at all in v1 or refused like `rename`; a partial recursive
  delete that reports success would be the same class of bug as the fake rename.
- `mkdir` → the conventional zero-byte object whose key ends in `/`. CPE-1683 must not then show it as a
  file; if that ordering slips, the two tickets need to agree on the marker's exact shape.

## MEASURED 2026-08-13: the warning below is TRUE. Read this first, then the hypothesis it replaces

The section immediately below was a hypothesis, explicitly labelled unverified by the reviewer who raised
it. **It reproduces exactly.** Measured against the real send path with a raw request-line recorder,
comparing what left the process against `RequestTarget::encoded_path` (the exact string that was signed):

```text
SIGNED "/test-bucket/a/../b//c%252Fd.txt", SENT "/test-bucket/b//c%252Fd.txt"
```

`ureq` 2.12.1 parses every URL through the `url` crate (WHATWG URL parsing), which resolves dot segments
as part of parsing — before `ureq` has any say in it.

**Only dot segments are affected, and that half is measured too, not assumed**: in the same request the
empty path segment (`//`) and the percent-encoded `%` (`%252F`) both survived byte for byte. So two of the
three shapes this section asked about are fine, and the third is not.

**What was done about it:** `guard_path_survives_the_client` refuses a key carrying a `.`/`..` segment,
naming the cause, **before any request is sent** — rather than normalising it (which would silently
address a different object, the exact failure CPE-1689 exists to prevent) or sending it anyway (which
produces `SignatureDoesNotMatch` with nothing to say why). The follow-up that would actually make such a
key reachable is **CPE-1721**, filed from this measurement.

## Test this first: the HTTP client may rewrite the path you signed (added 2026-08-12)

Flagged by the PR #868 reviewer, and **explicitly labelled by them as unverified** — they were offline and
could not check `ureq`'s behaviour, so treat this as a thing to test, not a finding to act on.

S3 must **not** normalise dot segments: key `a/../b.txt` is a real, distinct key, and `crates/s3` correctly
signs the canonical path `/a/../b.txt`. But `ureq` 2 — the client this epic plans to use — is believed to
resolve dot segments while parsing the URL. If it does, it would put `/b.txt` on the wire while the
signature covers `/a/../b.txt`, and the server answers `SignatureDoesNotMatch` with nothing in the message
to say why.

`crates/s3`'s "one construction, so the URL and the signature cannot disagree" guarantee **ends at the crate
boundary**. This is the first ticket that crosses it.

So, before building the object operations: send a request whose key contains `..`, `//`, and a percent-
encoded `%2F`, and check what actually goes on the wire against what was signed. If the client rewrites the
path, that decides the client — or requires bypassing its URL parsing — and it is much cheaper to know now
than to debug as an unexplained 403 later. **State what you measured**, per the Evidence Rules in
`Ticketing/wiki.md`; this note is a hypothesis and should be replaced by a measurement.

Related: **CPE-1689**, which established that leading slashes and dot segments are preserved on purpose.

## Verify (headless)

The same in-process `tiny_http` fixture CPE-1683 stands up, extended to serve HEAD/GET/PUT/DELETE against a
temp directory — the technique `crates/webdav/src/lib.rs` already uses to map WebDAV methods onto
`std::fs`.

## Acceptance criteria

- [ ] Each of stat/read/write/delete/mkdir round-trips against the fixture, with a test per op.
- [ ] `read` of a large object never holds the whole body in memory at once — the chunking is asserted, not
      assumed, and removing it turns a test red.
- [ ] `rename` returns an error naming S3's lack of atomic rename, and `capabilities().supports_rename` is
      `false`. A test asserts no PUT-copy and no DELETE were issued — proving it refused rather than faked.
- [ ] `stat` on a missing key is not-found; `stat` on a denied key reports the denial. The two are
      distinguishable, through CPE-1682's error path.
- [ ] The `mkdir` marker written here is the exact key shape CPE-1683 filters out, verified by a test that
      does `mkdir` then `list` and sees a directory and no stray file.
- [ ] `cargo test` green; `cargo clippy --all-targets -D warnings` clean.

## A bodiless HEAD 404 will be your MOST COMMON case — plan for it

Added by the Foreman from the CPE-1682 UAT (PR #879), 2026-08-12, because it lands squarely on this
ticket's `stat` criterion.

CPE-1682's `map_s3_error(status, body)` is honest but body-driven: with no `<Code>` element to read, it
returns *"HTTP 404 and the response body could not be read as an S3 error … refusing to guess which cause
applies"*. That is the correct behaviour for a parser.

But **HTTP HEAD responses never carry a body.** Every existence/metadata check this ticket adds is
HEAD-shaped, so a `stat` on a missing key produces a 404 with nothing to parse — meaning the honest
"could not be read" message would become the *majority* user experience for the single most common
failure in the whole provider, precisely where the AC demands "a missing key is not-found; a denied key
reports the denial, and the two are distinguishable".

So `stat` must not lean on `map_s3_error` alone. Map a **bodiless** response by status **and HTTP
method**: a bodiless 404 from a HEAD is a genuine not-found; a bodiless 403 from a HEAD is a denial.
Route to `map_s3_error` when there IS a body. Say in the code which rule you applied and why, and pin
both bodiless cases with tests — otherwise the "distinguishable" criterion passes in unit tests that
supply a body and fails against every real server.

## Header bytes outside {SP, HTAB} ∪ [0x21,0x7E] — what ureq actually does (measured; read the correction below)

**Corrected 2026-08-13 by CPE-1683 — see "Correction to the ureq-header-drop warning above" in Notes,
below, before reading this section as a settled fact.** The heading below originally asserted a silent
drop; measured against the actual send path `S3Provider::list` uses, that specific claim did not
reproduce. The traced `unit.rs` mechanism immediately below is still real and still worth knowing — it
just is not what happens on the path this crate's `GET` requests take. Read this section for the
mechanism, then the Notes correction for what was actually observed on your own request path before
deciding anything from it.

Added by the Foreman from the PR #883 (CPE-1695) review, 2026-08-13. This is a landmine sitting exactly
where this ticket works, found by tracing the real send path rather than reading the obvious functions.

`ureq`'s write loop (`ureq-2.12.1/src/unit.rs:467-474`) does **not** pass a raw value to
`write_header`. It calls `header.value()` first and skips the header entirely when that returns `None`:

```rust
for header in &unit.headers {
    if let Some(v) = header.value() {
        // (the real source branches here on is_header_sensitive -> write_sensitive_header;
        //  both arms sit inside this same `if let Some(v)` gate, so both are skipped alike)
        prelude.write_header(header.name(), v)?;
    }
}
```

Note the gate applies to **sensitive** headers too — `Authorization` included. A signed `Authorization`
header carrying a non-conforming byte is dropped by the identical mechanism.

`Header::value()` (`header.rs:99-109`) filters through `is_field_vchar_or_obs_fold`
(`header.rs:231-237`), which permits only `{SP, HTAB} ∪ [0x21, 0x7E]`. The filter applies to the whole
`Option<&str>`, so **one** non-conforming byte makes `value()` return `None` and the entire header
disappears from the outgoing request — silently, with no error.

Bytes that trigger it include VT (`0x0B`), FF (`0x0C`), and **anything >= 0x80** — which means **NBSP**
(`0xC2 0xA0`), the character CPE-1695 deliberately decided to *preserve* through canonicalisation because
S3 does not trim it either.

**Why this is your problem, not CPE-1695's.** A dropped signed header desynchronises what-was-signed from
what-was-sent — the same opaque `SignatureDoesNotMatch` the S3 slice has been working to eliminate,
arriving by a different route. CPE-1695 fixed the canonicalisation side correctly; it cannot fix the
transport side because the transport does not exist yet. **You are the transport.**

Decide, and write the decision down:

- refuse the request loudly at our layer when a signed header value contains a byte the client will not
  send (so the failure names the byte instead of arriving as a signature mismatch), or
- encode the value, or
- use a client without that filter.

Do **not** assume the bytes reach the wire. A test that signs a header and asserts the signature is
correct will pass while the header never leaves the process.

## SETTLED (from source, PR #888 review): ureq is LOUD on every send path — the only exposure is middleware

This warning has now been through three revisions. Read this section and ignore the framing above it; the
traced `unit.rs` mechanism is real code but **unreachable** for headers you set on a `Request`.

**Outbound is validated unconditionally, before any socket work.** `Request::do_call`
(`ureq-2.12.1/src/request.rs:114-117`) opens with:

```rust
for h in &self.headers { h.validate()?; }
```

`Header::validate` (`header.rs:139-149`) calls `valid_value()` = `value.iter().all(is_field_vchar_or_obs_fold)`
— **the identical byte predicate** as `Header::value()`'s filter. A non-conforming byte therefore produces
`ErrorKind::BadHeader` → `Error::Transport("Bad Header: invalid header '<line>'")` **before** anything is
written. Loud, not silent.

**This covers you completely, by construction rather than by luck.** All six send methods — `call`, `send`,
`send_bytes`, `send_string`, `send_json`, `send_form` (`request.rs:78, 192, 215, 243, 265, 294`) — funnel
through `do_call`. Your PUT / HEAD / DELETE will be loud too. **You do not need to re-run the measurement.**

**The `unit.rs:466-474` silent skip sits downstream of that gate**, so its `None` branch cannot fire for
anything validation already rejected. The original trace was accurate about the code and wrong about its
reachability — the reviewer read the write loop without checking the gate above it.

### The one genuine exception, and it is the thing to actually watch

`do_call` validates at line 115 and runs the **middleware chain at ~line 163 — after**. A header injected by
middleware calling `request.set()` is **never re-validated**, so for that header the silent skip is live.
`crates/s3` registers no middleware today, so it is not exposed. **If you add any, that is where this bites.**

### Incoming response headers are the genuinely silent path

Not `unit.rs`: `response.rs:587` does `if let Ok(header) = line.into_header()` and skips a malformed line
outright, and `Header::value()` returning `None` makes `resp.header("x")` read as **absent**. If you branch
on a response header being present, a malformed one is indistinguishable from a missing one.

Scope: `http_interop` / `http_crate` are feature-gated (`lib.rs:446-452`) and off under this crate's
`default-features = false, features = ["tls","gzip"]`, so they were not audited.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereqs: **CPE-1681** and **CPE-1682**.
Independent of CPE-1683 apart from the shared marker-key convention — agree that shape in whichever lands
first and make the other's test depend on it.

### The marker-key shape, settled by CPE-1683 (2026-08-13)

CPE-1683 landed first and settled the shape (PR for CPE-1683): a `/`-rooted provider path like
`/photos/2024` becomes the key `photos/2024/` (no leading slash, one trailing slash, no other content),
via the now-`pub` `cpe_s3::provider::provider_path_to_key_prefix`. Reuse that function directly for the
`mkdir` marker's key rather than re-deriving the shape — `crates/s3/src/provider.rs`'s
`parse_list_bucket_result` already filters a `<Contents>` entry whose `<Key>` equals the requested prefix
exactly, so writing the marker at exactly that key is what makes CPE-1683's own AC4 test
(`a_zero_byte_prefix_marker_object_does_not_appear_as_a_file_entry`) meaningful for a real marker CPE-1684
writes, not just a synthetic one the fixture injects for testing.

### Correction to the `ureq`-header-drop warning above (measured by CPE-1683, 2026-08-13)

The warning above was traced from `ureq` 2.12.1's source and explicitly labelled "a thing to test, not a
finding to act on" — CPE-1683 tested it, against the exact code path `S3Provider::list` uses
(`Agent::get(url).set(name, value).call()`), via the Evidence Rules' guard-negative-control (delete the
guard, re-run, restore). **It does not reproduce there**: `ureq` itself already refuses a header value
carrying a disallowed byte, loudly, via `ureq::Error::Transport("Bad Header: invalid header '...'")`,
before any byte reaches the network — proven by a fixture request counter that stayed at `0`. The
silent-drop mechanism traced in `unit.rs`'s write loop is real in `ureq`'s source, but evidently sits
behind a different internal path than the outbound request-builder API (most likely the one that parses
headers arriving *off the wire*, not the one writing them going out). **This does not mean the warning is
moot for CPE-1684** — `stat`/`read`/`write`/`delete`/`mkdir` may build requests differently (a different
`ureq` entry point, a streamed body, a different header-setting call) and could reach the silent-drop path
CPE-1683's `GET`-only `list` did not. Re-run the same measurement (delete whatever guard you write, observe
what `ureq` actually does, restore) against your own code's real send path before assuming either outcome.
See `crates/s3/src/provider.rs`'s module doc, "The `ureq`-header-drop decision", for the full write-up and
`guard_header_sendable` for a reusable pattern (kept even though the silent-drop case didn't reproduce, for
a clearer, byte-naming refusal a beat sooner than `ureq`'s own message).

## Work Log

### 2026-08-13 — implemented, branch `cpe-1684-s3-object-ops`

Everything lands in `crates/s3/src/provider.rs` (plus `src/docs/31-network.md`). No other crate changed:
`crates/s3` is not yet a dependency of `crates/vfs` (CPE-1685 is still blocked), so this slice is
self-contained.

**What was built**

- `stat` — `HEAD` on the object key. A 2xx reads `Content-Length`; a **200 with no usable
  `Content-Length` is refused**, because `size: 0` would be an invented measurement rather than a read
  one. A 404 falls back to a prefix probe so a virtual directory is reported as a directory instead of
  as missing; a 403 is reported as a denial and is **never** softened into not-found. The bucket root
  stats as a directory, but only after a real one-key listing proves the endpoint, addressing and
  credentials — not by fabricating a success.
- `read` — `GET`, fixed 64 KiB chunks (`cpe-ftp`'s convention, matched byte for byte), capped at 2 GiB
  (`cpe-webdav`'s value, matched so the two HTTP backends refuse at the same point). **No end-to-end
  deadline**, per the ticket and CPE-1706: this is the bulk-transfer call site `signed_get`'s
  `Option<Duration>` was made for.
- `write` — `PUT` of the whole body, no deadline (bulk), with a courtesy refusal past 5 GiB.
- `mkdir` — a zero-byte `PUT` at exactly `provider_path_to_key_prefix(path)`, reusing CPE-1683's function
  rather than re-deriving the shape.
- `delete` — exactly one key; see the decision below.
- `rename` — refused, with `supports_rename = false`. No request of any kind is issued.

**Three findings worth more than the code**

1. **The "test this first" hypothesis is true** — see the MEASURED section at the top of this ticket.
   `ureq` rewrites `a/../b.txt` to `b.txt` between signing and sending. Guarded, and CPE-1721 filed
   (originally filed as CPE-1718; renumbered — that ID was already claimed on an unmerged branch by the
   CPE-1710 worker's `join_files` follow-up).
2. **AC2 as worded is not achievable.** *"`read` of a large object never holds the whole body in memory
   at once"* cannot be true of any implementation of this trait method: `FileSystemProvider::read`
   returns `Vec<u8>`, so by the time it returns, it must. What was delivered instead, and what the tests
   actually assert: no unbounded single allocation (fixed stack buffer), and the cap fires **during** the
   transfer rather than after it — proven against a server that never stops sending, where a
   `read_to_end` would grow until the process died. `cpe-ftp` and `cpe-webdav` both already name a
   streaming `read` at the trait level as the real fix and put it out of scope; this does the same, in
   writing, on `FileSystemProvider::read`'s doc comment rather than silently.
3. **`list_with_filtered_count`'s instruction to these ops, read literally, contradicts CPE-1689.** It
   says not to accept "ANY provider-supplied name as a literal key" without an `is_safe_s3_leaf` check.
   Applied to a whole path that would split `/a/../b.txt` and refuse the `..` — a key CPE-1689
   established must be reachable. They reconcile once you notice `is_safe_s3_leaf` decides whether a leaf
   may be *surfaced as a navigable child*, not whether a key is *addressable*; and a leaf that guard
   refuses never becomes a `ProviderEntry`, so it cannot be "provider-supplied" here in the first place.
   Written up on `provider_path_to_object_key`.

**The `delete` decision, and what the user is told**

S3 answers `204` to a `DELETE` of a key that never existed, so a 2xx proves *"this key is absent now"*
and not *"an object was removed"*. `delete` claims only the former, in its doc and in the module doc. It
deliberately does **not** add a `HEAD`-before-`DELETE`: that is racy by construction and still could not
prove the `DELETE` removed anything — paying a round trip for a stronger-sounding claim that is not
actually stronger.

That is benign for one object and **dangerous for a directory**, which is where the ticket's warning
bites: a plain single-key `DELETE` of `photos/2024` returns 204 while the entire subtree stays put. So
one `ListObjectsV2` probe decides the case first:

- real entries under the prefix → **refused**, for the same reason `rename` is. S3 has no atomic
  multi-key delete; a per-key loop that failed halfway would leave part of the tree gone while reporting
  success. Recursive delete is out of scope for v1 — the ticket offered this as one of its two options.
- only the zero-byte marker (an empty directory) → that *is* one key, so it is deleted honestly.
- nothing under the prefix → an ordinary object; delete the key.

Two subtleties found while building it, both now pinned by tests: the marker key is a strict prefix of
every key beneath it, so S3's lexicographic order **always** returns it first — `max-keys=1` would make a
thousand-object directory look empty. And S3 may legally return fewer keys than asked for, so
`IsTruncated` (not `max-keys`) is the load-bearing half of the check; the probe's doc records which is
which rather than claiming both are essential.

**Evidence — every new guard broken on its own** (full pasted output in the PR body). Ten probes, each
restored with `git checkout --` and each re-run confirming `Compiling cpe-s3`: the path guard, the `read`
chunk cap (two distinct reds, one of them the deadline harness catching what would otherwise hang CI),
the bodiless-404 arm, the bodiless-403 arm, the `delete` directory refusal, the `IsTruncated` half of the
probe on its own, the `mkdir` marker key shape, the `rename` refusal (replaced with a working
copy-then-delete, which the request counter caught), the missing-`Content-Length` refusal, and the
marker's contribution to `raw_entries`. One probe cost an uncommitted doc edit to `git checkout --` —
exactly the trap the ground rules name — and it was re-applied and committed before continuing.

`cargo test`: 164 passed. `cargo clippy --all-targets -- -D warnings`: clean. `crates/s3` has no feature
modes in CI (`.github/workflows/ci.yml:427` runs exactly those two commands).

**One test-rig correction worth recording.** The first version of the missing-`Content-Length` test used
the `tiny_http` fixture and **passed while measuring nothing**: `tiny_http` always emits a
`Content-Length` and silently drops a header it cannot parse as one (`response.rs:266-270`), so `stat`
received `Content-Length: 0` and dutifully reported a zero-byte object. That response shape can only be
produced over a raw socket, and now is.

**Docs.** `src/docs/31-network.md` already anticipated this work in the future tense and made one promise
the shipped code deliberately breaks — *"Deleting a 'folder' will mean deleting the objects under that
prefix"*. Corrected to the refusal, moved to the present tense, and given the two new user-visible facts:
the 204-proves-nothing semantics of a successful delete, and the unreachable `.`/`..` keys.

### 2026-08-13 — round 2, from the UAT

**Blocker: `delete` reported success on a non-empty directory and deleted nothing.** The real bug of this
ticket, and my own. `probe_prefix` reported "real entries" as `page.entries.len()` — a count taken
**after** `is_safe_s3_leaf` filtering. That guard refuses a leaf containing `\`, a control byte, or a
literal `..`/`.`, every one of which is a legal S3 key. Such an object landed in `raw_entries` but not in
`entries`, so `delete` read the directory as marker-only, removed the marker, took S3's `204`, and
returned `Ok(())`. On a **conforming** server. The marker-present variant is the worse one: the folder
then vanishes from every listing while the object survives underneath it, unreachable through the UI.

Fixed to `page.entries.len() + page.filtered_count`. What makes it worth recording is that
`is_safe_s3_leaf`'s own doc already said *"A leaf this refuses is a real S3 key"* — the model was written
down correctly and the probe consulted the wrong number anyway. `filtered_count` is part of "is there
content here", not a diagnostic sideline.

The suite passed **identically with and without the fix**, so the fix was the easy half. Reaching the case
at all needed a new fixture sentinel (`.s3unsafe`), because a filesystem-backed fixture cannot hold a file
named `holiday\2024.jpg` — which is exactly why no existing test covered it. Reproduced red before fixing
(`Ok(())`), then green, then re-probed after committing: dropping `filtered_count` again reds that one
test and nothing else.

**Blocker: two doc claims the code did not honour.** The first — *"deleting a folder that still has
things in it is refused"* — was falsified by the bug above and is true again now the code is fixed. The
second was real and separate: `provider_path_to_object_key`'s `trim_matches('/')` collapses leading and
trailing slashes **one layer above** `object_target`, which preserves them exactly as CPE-1689 intended.
Measured to the wire: `"//a.txt"` → `/test-bucket/a.txt`, so `write("//report.pdf", …)` overwrites
`report.pdf`.

Decided: **fix the doc now, file the collapse as CPE-1722** — not fixed inside this ticket, because the
sibling `provider_path_to_key_prefix` (merged, CPE-1683) carries the identical trim and feeds
`list`/`mkdir`, so changing one alone would leave two path grammars inside one provider; and
trailing-slash insignificance is a cross-backend `FileSystemProvider` contract that `stat`/`delete` rely
on when they build `format!("{key}/")`. Reachable only by a hand-typed path. The reasoning is on
`provider_path_to_object_key` as a KNOWN GAP section, so the CPE-1689 citation there no longer implies a
guarantee this layer does not provide.

**`max-keys=2` had no test** — changing it to `1` left the suite green. It is only observable against a
**non-conforming** server, since on a conforming one the `IsTruncated` belt catches the same case first;
that is the division of labour between the two halves. Added a server that honours `max-keys` but always
claims `IsTruncated=false`; `max-keys=1` now reds that test alone.

**Corrected a wrong comment on a security guard.** The `%2e` arms of `is_url_dot_segment` were described
as "pure future-proofing". The UAT's wider probe measured `ureq` actually rewriting `a/%2e%2e/b.txt` →
`b.txt` — they are a **live match for real behaviour**, merely unreachable through this crate's own
encoder (which escapes `%` to `%25`). The guard is exactly as wide as the defect, which is the property to
keep; the old wording invited a later reader to delete the arms as dead weight.

**Fixture sprawl contained, not fixed.** CPE-1693 owns the class. The uniqueness stamp now runs once per
test-binary run with numbered subdirectories, so a run leaves one top-level `cpe-s3-fixtures-*` entry
instead of one per spawn site — nothing is deleted, but the count stops being multiplied by the number of
tests and CPE-1693 gets a single path to remove.

**Verified against PR #895 before and after it merged.** It adds `crates/s3/clippy.toml` banning bare
`std::fs::rename` under `-D warnings`. `crates/s3/src` has none (its `rename` is a refusal that never
touches the filesystem). Pre-merge I fetched that exact file from the #895 branch, dropped it in, forced a
real recompile and re-ran clippy: clean. Post-merge, rebased and re-ran for real: clean. Worth noting that
changing `clippy.toml` alone does **not** invalidate cargo's cache — the first run reported a cached
`Finished` and measured nothing.

166 tests pass; `cargo clippy --all-targets -- -D warnings` clean on the merged state.
