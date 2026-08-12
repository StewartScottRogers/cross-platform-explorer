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

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereqs: **CPE-1681** and **CPE-1682**.
Independent of CPE-1683 apart from the shared marker-key convention — agree that shape in whichever lands
first and make the other's test depend on it.
