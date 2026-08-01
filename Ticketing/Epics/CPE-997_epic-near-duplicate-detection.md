---
id: CPE-997
title: "EPIC: Near-duplicate & similar-image detection"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-24
closed:
---

> **Filed + activated 2026-07-24** (workshift, Foreman). First slice = the **pure perceptual-hash +
> similarity-clustering core** (CPE-998), model-free and reusing the already-vendored `image` crate. The
> review/act UI is attended.

## Goal
Find files that are **visually/near-identical but not byte-identical** — the same photo re-saved at a
different quality, a screenshot cropped a few pixels, a resized copy — and group them so the user can review
and clean up. Complements the existing exact-duplicate finder ([[CPE-706]] / `duplicates.rs`, which only
catches *byte-identical* files via SHA-256) with **perceptual** similarity.

## Why
Exact-hash dedup misses the most common real-world duplicates: the same image saved twice at different
compression, a photo and its resized thumbnail, near-identical burst shots. Perceptual hashing (a small,
robust image fingerprint) + distance clustering catches those. Pure, model-free, and reuses the `image`
crate already in the tree — no AI backend, no new deps, delete-testable.

## Rough scope (areas, not child tickets)
- A **pure perceptual-hash core**: `phash(image_bytes) -> u64` (a difference/average hash over a downscaled
  grayscale grid), `hamming(a,b)`, and `cluster(items, max_distance)` grouping near-identical images.
- Wiring: hash the images in a folder/selection (reuse the thumbnail/decode paths), cluster, present groups.
- A **review UI**: show near-duplicate groups side by side, pick keepers, delete/hardlink the rest (reuse
  the transfer/secure-delete + checkpoint machinery).
- Later: extend beyond images (audio fingerprint, near-identical documents) behind the same clustering core.

## Open questions (resolve at activation)
- Hash algorithm + size (dHash vs aHash, 64-bit vs larger) and the default distance threshold for "similar".
- How images are enumerated + decoded at scale (reuse the thumbnail pipeline; stream/cancel).
- Safety on cleanup: never auto-delete; always a reviewed, undoable action (checkpoint via [[CPE-732]]).

## Definition of Done
- The app groups visually-similar (non-identical) images in a folder/selection and lets the user review +
  clean up safely (reviewed, undoable).
- The perceptual core is pure + cargo-tested; with the feature unused there is no cost.

## Child tickets
1. **CPE-998** — Pure perceptual-hash + clustering core (`cpe-server::perceptual`): `phash` (image → u64
   dHash via the `image` crate), `hamming`, `cluster(items, max_distance)` (union-find, 2+-member groups,
   deterministic). ✅ **Done + independently reviewed (workshift QA gate: opus reviewer APPROVED — re-ran
   tests + both clippy clean, verified logic + adversarial edges). PR #318.**
2. **CPE-999** — SimHash near-duplicate **text/documents**: `simhash(text) -> u64` (token-shingle SimHash)
   which reuses CPE-998's `cluster`/`hamming` directly (a u64 fingerprint is a u64 fingerprint) to group
   near-identical documents. Pure, no deps. *Headless — buildable now.*
3. **CPE-1000+** — Folder/selection hashing pipeline (reuse decode/thumbnail paths) + the side-by-side
   review & safe-cleanup UI. **GUI/attended.**

## Follow-ups noted
- **Golden-value dHash test (from the CPE-998 review):** the deterministic/near tests run on all-zero-hash
  gradient fixtures; only `column_bands` exercises a non-zero hash and no test pins an exact `u64`, so a
  bit-order/direction swap in packing could go uncaught. Add one golden-value assertion on a structured
  fixture to harden. Non-blocking (code verified correct), captured here.

2026-07-25 (workshift) — **Golden-value dHash test done (CPE-1030, PR #346).** The captured follow-up is
resolved: a golden `u64` assertion now pins the dHash bit layout. The first attempt used a symmetric
`column_bands` fixture (all 8 rows identical → hash `0x5555…`, invariant under row-reversal); the
independent reviewer caught that it left cross-row packing unguarded, and it was replaced with an
asymmetric `diagonal_staircase` fixture (8 distinct packed bytes, golden `0xe0f0f87c3e1f0f07`) plus
`hash != hash.swap_bytes()` assertions — so a row-order/packing swap now fails the test. Follow-up closed.

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Folder/selection hashing pipeline + side-by-side review/cleanup UI unbuilt (only perceptual/SimHash cores).
