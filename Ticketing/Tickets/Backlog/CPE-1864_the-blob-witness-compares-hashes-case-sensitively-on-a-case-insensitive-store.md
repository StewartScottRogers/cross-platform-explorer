---
id: CPE-1864
title: the blob witness compares hashes case-sensitively on a case-insensitive filesystem
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`validate_blob_name` accepts **uppercase** hex, and Windows and macOS open `blobs/05c2…b8` and
`blobs/05C2…B8` as the same file. But `manifests_naming` — CPE-1861's witness for "does any surviving
manifest still name this blob" — compares hash strings **exactly**.

So a survivor whose manifest spells its hash in uppercase is invisible to the witness, and the blob it
depends on is freed out from under it. Measured by the independent Security Auditor:

```
keeper restores BEFORE the prune: true
prune(victim) freed 13 bytes
blobs/05c200fe…b8 exists: false
keeper restores AFTER the prune: Err("...\blobs\05C200FE…B8: cannot find the file")
```

## Why Low

Reaching it requires editing the survivor's own manifest — and an attacker with that access could simply
delete the manifest instead. Nothing in the app writes uppercase hashes; `capture` produces lowercase.

It is filed because the fix is one line and the failure mode is content loss with a success report, which
is the grammar this subsystem keeps closing. It is also the kind of thing a future format change or an
imported store could trip without any attacker at all.

## Acceptance criteria

- [ ] `manifests_naming` compares hashes case-insensitively (`to_ascii_lowercase()` on both sides, or
      normalise at parse).
- [ ] Decide whether `validate_blob_name` should keep accepting uppercase at all. If the store only ever
      writes lowercase, accepting uppercase buys nothing and creates exactly this class of mismatch —
      but refusing it would reject an existing store that somehow contains one, so say which way and why.
- [ ] Check every other place a hash is compared, looked up, or used as a filename — the witness is the
      one that was found, not necessarily the only one. `contains`, `release`, `total_bytes` and the blob
      delete loop all take a hash.
- [ ] Test the shape above: a survivor spelling its hash uppercase, a victim pruned, and the survivor must
      still restore byte-for-byte. Assert the fixture is live — that the uppercase spelling really reached
      the manifest on disk — before asserting the harm.
- [ ] Red-proof with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Security Auditor during CPE-1861's audit and classed as contrived but real, one
of four non-blocking follow-ups from a MERGE recommendation.

Related: CPE-1861 (the witness this hardens), CPE-1863 (the byte-cap loop that consumes the same answer).
