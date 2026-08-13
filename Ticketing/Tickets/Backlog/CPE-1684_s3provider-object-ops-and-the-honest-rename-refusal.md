---
id: CPE-1684
title: "S3Provider object ops — stat/read/write/delete/mkdir, and rename refused honestly rather than faked"
type: Feature
status: Backlog
priority: Medium
component: Backend
tags: [needs-prereq]
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

## `ureq` will SILENTLY DROP a header whose value has any byte outside {SP, HTAB} ∪ [0x21,0x7E]

Added by the Foreman from the PR #883 (CPE-1695) review, 2026-08-13. This is a landmine sitting exactly
where this ticket works, found by tracing the real send path rather than reading the obvious functions.

`ureq`'s write loop (`ureq-2.12.1/src/unit.rs:467-473`) does **not** pass a raw value to
`write_header`. It calls `header.value()` first and skips the header entirely when that returns `None`:

```rust
for header in &unit.headers {
    if let Some(v) = header.value() {
        prelude.write_header(header.name(), v)?;
    }
}
```

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

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereqs: **CPE-1681** and **CPE-1682**.
Independent of CPE-1683 apart from the shared marker-key convention — agree that shape in whichever lands
first and make the other's test depend on it.
