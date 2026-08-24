---
id: CPE-1864
title: the blob witness compares hashes case-sensitively on a case-insensitive filesystem
type: bug
priority: Low
status: Doing
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

- [x] `manifests_naming` compares hashes case-insensitively (`to_ascii_lowercase()` on both sides, or
      normalise at parse).
- [x] Decide whether `validate_blob_name` should keep accepting uppercase at all. If the store only ever
      writes lowercase, accepting uppercase buys nothing and creates exactly this class of mismatch —
      but refusing it would reject an existing store that somehow contains one, so say which way and why.
- [x] Check every other place a hash is compared, looked up, or used as a filename — the witness is the
      one that was found, not necessarily the only one. `contains`, `release`, `total_bytes` and the blob
      delete loop all take a hash.
- [x] Test the shape above: a survivor spelling its hash uppercase, a victim pruned, and the survivor must
      still restore byte-for-byte. Assert the fixture is live — that the uppercase spelling really reached
      the manifest on disk — before asserting the harm.
- [x] Red-proof with the minimal realistic change, observe red, revert, record the line.

## Work Log

**2026-08-23 — Investigated and fixed.**

**What the user loses today, worked out before choosing a fix.** `manifests_naming` compared a disk
manifest's `hash` field against the candidate set with plain `==`. `validate_blob_name` accepts uppercase
hex, and nothing in the app ever *writes* one — but a manifest is trusted-but-external input the same way
a plain file copy is (CPE-1861 already documents a duplicate manifest file as a legitimate second namer),
so a survivor manifest hand-edited (or produced by a different tool, or an imported store) to spell its
hash uppercase was invisible to this witness. Pruning any OTHER manifest sharing that same content then
freed the blob: `to_release` included the shared hash because `still_named` never contained it, `release`
dropped the refcount, and the delete loop's `fs::remove_file` actually removed the file — not a lookup
that resolved to the wrong path, a real deletion. So this is a **false "blob missing"** from the witness's
point of view, which is the dangerous direction: it deletes content a live checkpoint still names, `prune`
reports success and frees bytes, and the harm only surfaces later when that checkpoint's own restore fails
to find a file that should be there — exactly the Security Auditor's measured
`Err("...\blobs\05C200FE…B8: cannot find the file")`. The opposite mistake, a false "blob present" (an
absent blob reported as there, or `restore` silently handing back the wrong bytes), is not this bug's
shape — nothing in the exact-string mismatch ever makes an absent blob look present.

**`validate_blob_name` decision: kept permissive, on purpose.** Tightening it to refuse uppercase would
not close this hole — the mismatch is between two hash *strings* being compared to each other, not about
whether either one is individually well-formed hex — and it would open a worse one: `blob_source` (the
read half `restore` uses) calls the same validator, so refusing uppercase would make `restore` fail a
manifest that legitimately names an existing uppercase-spelled blob (an imported store, a different
capture tool, a hand-recovered manifest — the exact shapes the ticket's own "Why Low" section names).
That trades a rare, contrived witness gap for a real regression against restoring an otherwise-valid
checkpoint. Left permissive; the comparison inside `manifests_naming_strict` (CPE-1867's split-out scan)
is fixed instead.

**Fix.** `manifests_naming_strict` now builds a `lowercase -> wanted's own spelling` map once per call and
matches each disk manifest's hash against it case-insensitively, inserting `wanted`'s ORIGINAL spelling
into the result (not the disk manifest's) — every caller does `BTreeSet` algebra (`difference`,
`contains`) between the return value and a set built in `wanted`'s casing, so returning the disk
manifest's own case would just move the mismatch one call further up instead of closing it.

**Every other hash comparison surveyed, per the acceptance criteria** — `contains`/`get`/`release` on
`BlobStore` and `prune`'s delete loop all key off `index.json` (written exclusively by this app's own
capture, always lowercase — confirmed: `sha256_file` documents "Lowercase hex", and every blob filename
capture writes is that same hash string, so `blob_files_on_disk`'s candidates are always lowercase too) or
off the pruned manifest's own hashes (usually the victim's own honest capture). None of them is the site
where an externally-supplied spelling reaches a case-sensitive lookup; `manifests_naming`'s comparison was.

**Red-then-green.** Reproduced the bug directly with the new
`cpe_1864_a_survivor_spelling_its_hash_uppercase_still_protects_the_shared_blob` test (a victim capture,
a copied-and-uppercased survivor manifest naming the same content, prune the victim, assert the blob
survives and the survivor still restores byte-for-byte) by temporarily reverting
`manifests_naming_strict`'s comparison to plain `wanted.contains(&f.hash)`:

```
RED (exact-string comparison, reverted):
thread '...cpe_1864_...' panicked at src\snapshot_capture.rs:3272:9:
HARM: pruning the victim deleted a blob the survivor's manifest still names (uppercase spelling)
— a false "blob missing" from the witness deleted content a live checkpoint still needs
test ...cpe_1864_a_survivor_spelling_its_hash_uppercase_still_protects_the_shared_blob ... FAILED
```

Reapplied the fix and reran:

```
GREEN (case-insensitive comparison, the fix):
test snapshot_capture::tests::cpe_1864_a_survivor_spelling_its_hash_uppercase_still_protects_the_shared_blob ... ok
```

Full `cargo test` run (crates/server, default features): 2382 passed, 0 failed, 8 ignored (pre-existing,
unrelated) — includes CPE-1871's pin, CPE-1861's manifest-witness suite, CPE-1844's `store_total_bytes`
suite, and CPE-1867's racing-rename test, all still green; no guard weakened. `cargo clippy --all-targets
-- -D warnings` clean in both default and `--features index` modes.

The test needs no case-insensitive filesystem to fail (it is deterministic across the 3-OS CI matrix):
the harm is a real `fs::remove_file` of the blob under its actual on-disk (lowercase) name, driven by the
app's own string comparison, not by how any given OS resolves a path.

## Notes

Found by the independent Security Auditor during CPE-1861's audit and classed as contrived but real, one
of four non-blocking follow-ups from a MERGE recommendation.

Related: CPE-1861 (the witness this hardens), CPE-1863 (the byte-cap loop that consumes the same answer).
