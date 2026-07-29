---
id: CPE-1007
title: Near-identical-folder detection via Jaccard similarity
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1002
---

# CPE-1007 — Near-identical-folder detection via Jaccard similarity

## Summary

A pure near-identical-**folder** detector, child of epic CPE-1002 ("File inspection & safety
utilities"). A level up from the existing exact-file duplicate finder (`crates/server/src/
duplicates.rs`, byte-identical files): this finds folders that are **~the same** even though they
aren't identical as a whole — e.g. `Photos/` vs. `Photos (backup)/` sharing 90% of their files.
Operates on caller-supplied `(folder-path, set-of-file-content-hashes)` pairs — the filesystem walk
+ per-file hashing that build those sets is the adapter's job, out of scope here (reuses
`duplicates.rs`'s hashing there later). No dependencies, no I/O.

New module `crates/server/src/folder_similarity.rs`.

## Design

- `pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64` — `|A∩B| / |A∪B|`, range
  `[0.0, 1.0]`.
- **Empty-set convention**: `jaccard(∅, ∅) = 0.0` by definition (documented on the function) — two
  empty folders (no file hashes at all) are not a meaningful "similar" match, so this is defined as
  "not similar" rather than the set-theoretic `0/0 = 1` convention. This also guarantees the result
  is **never `NaN`** for any input (the empty/empty case is special-cased before the division, with
  a second defensive `union == 0` guard).
- `pub struct FolderHashes { pub path: String, pub hashes: BTreeSet<String> }` (`Debug, Clone`) — a
  folder's path plus its (already-computed) file-content-hash set.
- `pub fn cluster_similar_folders(folders: &[FolderHashes], threshold: f64) -> Vec<Vec<String>>` —
  groups folder paths whose pairwise `jaccard >= threshold` (inclusive boundary), transitively
  (single-link clustering via union-find over indices), mirroring `perceptual::cluster`'s
  union-find shape but keyed on a **float similarity floor** instead of an integer Hamming-distance
  ceiling. Only 2+-member groups are returned; each group's paths are sorted; groups are sorted by
  their own (already-sorted) first path — fully deterministic regardless of input order.
- `threshold` is a plain caller parameter, no default baked into the function. **Recommended
  default for "near-identical folder": 0.8** (documented here, not hardcoded) — chosen as a
  reasonable "obviously the same folder, just a few files added/removed/renamed" cutoff; the
  ticket's own worked example (`Photos/` vs `Photos (backup)/`) uses 90% overlap as the canonical
  motivating case, comfortably above that floor.
- `pub mod folder_similarity;` added to `lib.rs` with a short doc comment.

## Acceptance Criteria

- [x] `jaccard(a, b) -> f64` returns `|A∩B|/|A∪B|` in `[0.0, 1.0]`.
- [x] `jaccard(∅, ∅) == 0.0`, never `NaN`, documented as an explicit convention.
- [x] `FolderHashes { path, hashes }` derives `Debug, Clone`.
- [x] `cluster_similar_folders(folders, threshold) -> Vec<Vec<String>>` groups folders by
  transitive single-link Jaccard similarity `>= threshold`, returns only 2+-member groups, each
  group's paths sorted, groups in deterministic order independent of input order.
- [x] Threshold boundary is inclusive (`>=`): a pair whose similarity lands exactly at `threshold`
  is grouped; a pair just above that same similarity value (higher threshold) is not.
- [x] Transitive single-link clustering proven directly: A~B and B~C at/above threshold, but A~C
  below threshold directly — still one group.
- [x] Pure std, zero new dependencies, zero filesystem I/O.
- [x] `pub mod folder_similarity;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib folder_similarity` passes.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `folder_similarity.rs` end-to-end: `jaccard(a, b)`, `FolderHashes`, and
  `cluster_similar_folders(folders, threshold)`.
  - **Empty-set convention**: `jaccard(∅, ∅)` is defined as `0.0`, not the set-theoretic `0/0 = 1`
    convention some libraries use. Rationale: two folders with no file hashes at all (empty, or
    every file unreadable) aren't a meaningful "these are the same folder" signal, so treating them
    as "not similar" is the safer default for a dedup-style feature — it avoids two unrelated
    always-empty folders showing up as a spurious 100%-similar pair. This also structurally
    guarantees no `NaN`: the empty/empty case returns early before any division, with a second
    defensive `union == 0` check as a belt-and-braces guard against future edits.
  - **Default threshold (not hardcoded, documented here)**: recommend **0.8** for "near-identical
    folder" as the out-of-the-box UI/adapter default. The ticket's own motivating example
    (`Photos/` vs `Photos (backup)/`) sits at 90% overlap, comfortably above 0.8; 0.8 leaves enough
    headroom to still catch "a handful of files added/removed/renamed" without also matching
    folders that just happen to share a few common files (installers, `.gitignore`, etc.).
  - **Algorithm**: `jaccard` is a direct `BTreeSet::intersection`/`union` count ratio.
    `cluster_similar_folders` mirrors `perceptual::cluster`'s union-find shape (same two local
    `find`/`union` helper functions, same "bucket by root, keep 2+, sort paths, sort groups by
    first path" finish) but swaps the integer `hamming <= max_distance` comparison for a float
    `jaccard >= threshold` comparison — the only structural difference from the sibling module.
  - **Test fixtures**: hand-built `BTreeSet`/`FolderHashes` fixtures, no real filesystem. Covered:
    identical sets → 1.0; disjoint → 0.0; a known ratio (`{a,b,c,d}` vs `{a,b,c,e}` → exactly 0.6,
    epsilon-compared); empty∩empty → 0.0 not NaN; one-empty-one-not → 0.0; result always in
    `[0,1]`; a 90%-overlap near pair clustering while a disjoint third folder stays a dropped
    singleton; identical folders clustering; an explicit transitive single-link chain (A-B and B-C
    each at exactly 0.8, A-C directly at ≈0.636, still one 3-member group); an inclusive
    `>=`-boundary test (exactly-0.6 pair groups at threshold 0.6, the same pair does *not* group at
    threshold 0.600001); no-match → empty; empty input → empty; single folder → empty; input-order
    independence (reversed input list produces identical output); and empty-hash-set folders never
    clustering at any realistic (>0) threshold. All float comparisons use an epsilon
    (`(x - expected).abs() < 1e-9`), never `==` on a computed `f64`.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib folder_similarity` → 15/15 passed; `cargo clippy --all-targets -- -D
    warnings` clean; `cargo clippy --all-targets --features index -- -D warnings` clean (exit code
    0 confirmed explicitly). No clippy fixes needed.
  - Status → Done; ACs checked; moving to
    `Tickets/Done/2026/Q3/July/Week-30/CPE-1007_folder-similarity.md`.
