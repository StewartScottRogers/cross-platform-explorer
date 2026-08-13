---
id: CPE-1683
title: "S3Provider::list — virtual directories over ListObjectsV2, paginated, with has_real_dirs = false"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [needs-prereq]
epic: CPE-1503
estimate: M
created: 2026-08-12
closed: 2026-08-13
---

## What

`S3Provider` implementing the `list` half of `cpe_server::provider::FileSystemProvider`, so an
`s3://region@endpoint/bucket/prefix` location produces `ProviderEntry` rows that the existing listing machinery can render
without knowing it is looking at an object store.

## Before you scope this: "B2/GCS/Wasabi come free" is overstated (added 2026-08-12)

The epic's headline claim is that every S3-compatible store works for free once addressing is right.
The CPE-1681 worker, having just built the signer, flagged that this is **necessary but not
sufficient**, and that **this ticket is where it bites** rather than theirs:

- **Backblaze B2 and Wasabi** are genuinely drop-in.
- **Google Cloud Storage's S3 shim (the XML API)** has its own SigV4 quirks and **does not support
  ListObjectsV2 the same way** — which is precisely the call this ticket is built on.

Do not discover this at the end. Decide up front whether GCS is in scope: either verify it against the
XML API's actual listing behaviour, or state plainly that GCS is out of scope for the first version and
correct the epic's claim to match. An unverified "it comes free" is the confident-wrong-answer failure
this crew keeps closing — see the Evidence Rules in `Ticketing/wiki.md`, particularly the one about
stating the scope of a claim.

## Why the delimiter, and not a client-side key split

A bucket is flat: there are no directories, only keys with slashes in them. The naive implementation lists
every key in the bucket and splits on `/` client-side — which works on a fixture and falls over on a real
bucket with a million objects, because it downloads the entire keyspace to render one folder.

ListObjectsV2 with `delimiter=/` is the server-side answer: it returns the keys directly under the prefix
as `<Contents>`, and the next level of prefixes as `<CommonPrefixes>`, which are exactly the virtual
directories. The cost is then proportional to one level, which is what makes an S3 location usable in an
explorer at all.

`provider.rs` already anticipates this. Its test
`a_provider_can_override_capabilities_eg_s3_style_no_real_dirs` was written against a hypothetical S3
provider — this ticket is the real one that test was waiting for.

## Scope

- `list(path)` → ListObjectsV2 with `prefix` and `delimiter=/`; `<CommonPrefixes>` become directory
  entries, `<Contents>` become file entries with size and last-modified.
- **Pagination is not optional.** S3 caps a response at 1000 keys and returns `IsTruncated` plus a
  `NextContinuationToken`. A listing that stops at 1000 silently loses files, which is worse than failing.
  Follow the tokens to completion, and cap the total so a pathological bucket cannot exhaust memory.
- Capability override: `has_real_dirs = false`, `supports_rename = false` (CPE-1684 covers the refusal
  itself). Everything else stays at the trait default.
- The bucket's own prefix marker objects (a zero-byte key ending in `/`, the convention CPE-1684 writes for
  `mkdir`) must not show up as a spurious empty file inside their own directory.
- Remote names flow through the same path-traversal guards the listing layer already applies to SFTP and
  WebDAV — an object key is attacker-controlled text and `../` in a key must not escape anything.

## Verify (headless)

An in-process `tiny_http` fixture serving canned ListObjectsV2 XML, the same technique
`crates/webdav/src/lib.rs` uses for its PROPFIND tests. No Docker, no MinIO, no credentials, and it runs
identically on all three CI OSes.

## Acceptance criteria

- [ ] Listing a prefix returns its immediate files and its immediate virtual directories, and nothing from
      deeper levels.
- [ ] A truncated response is followed to completion: a fixture returning three pages yields all three
      pages' entries, and a test proves it — deleting the continuation-token loop turns that test red.
- [ ] `capabilities().has_real_dirs` is `false`, and the row count for a prefix does not depend on how many
      objects exist elsewhere in the bucket (the fixture asserts the request carried `delimiter=/`).
- [ ] A zero-byte `prefix/` marker object does not appear as a file entry.
- [ ] A key containing `../` or a leading `/` cannot produce an entry that escapes the listed prefix.
- [ ] Non-2xx responses report through CPE-1682's shared error path, not an ad-hoc string.
- [ ] `cargo test` green; `cargo clippy --all-targets -D warnings` clean.

## Notes

Filed by the sprint PM at the CPE-1503 activation, 2026-08-12. Prereqs: **CPE-1681** (config, addressing,
signing) and **CPE-1682** (the error path). Independent of CPE-1684 — the two can be built in either order
once the foundation lands.
