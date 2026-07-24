---
id: CPE-969
title: Content-addressed snapshot capture + dedup store (bounded)
type: feature
component: Backend
priority: high
tags: ready
status: Done
created: 2026-07-24
epic: CPE-732
estimate: 3-4h
---

## Summary
Next headless slice of CPE-732 (checkpoint & rollback), also serving CPE-735 (local snapshots). CPE-917
(`restore_plan`) already computes the revert diff over two `Snapshot`s (`path → FileState{hash,size}`); this
ticket lands the **store side** the epic DoD calls for — *"efficient content-addressed snapshots… bounded,
dedup."* A new pure `cpe-server::snapshot` module: a refcounted content-addressed blob store + a
`plan_capture` that decides which blobs are **new (must store) vs reused (dedup)** under a per-file and
whole-store byte budget, plus `apply_capture` / `release` for the refcounted store lifecycle. No FS, no GUI
— a pure transform over `Snapshot` maps, fully cargo-testable.

Filed + worked in the dayshift tier-4 flow (2026-07-24): tiers 1–3 held no workable headless items (Backlog
empty, Deferred gated, every epic already activated with its pure cores landed), so continuing the
highest-value **In-Progress** epic with an undone headless child (per an Explore sweep). High priority; the
same store feeds two epics.

## Design (pure, reuses CPE-917's `Snapshot`/`FileState`)
- **`BlobStore`** — `hash → BlobMeta{size, refs}` (BTreeMap, deterministic). Content-addressed: identical
  content across files/snapshots is one blob; `refs` counts how many snapshots hold it (GC when it hits 0).
  Accessors: `contains`, `total_bytes`, `blob_count`.
- **`CaptureBudget{ max_blob_bytes, max_total_bytes }`** (0 = unlimited) — the *bounded* rule: a file bigger
  than `max_blob_bytes` is skipped (don't snapshot huge blobs); a new blob that would push the store past
  `max_total_bytes` is skipped. Reused blobs never re-count.
- **`plan_capture(store, scan, budget) -> CapturePlan`** — dedups the scan to distinct hashes, splits into
  `to_store` (new) vs `reused` (already present = dedup win), records `skipped` (Oversize / Budget) with the
  offending path, and totals `added_bytes`. Deterministic order (by hash).
- **`apply_capture(store, plan)`** — commit: insert new blobs (refs=1) and bump refs on reused ones.
- **`release(store, hashes)`** — a retention drop ([[snapshot_retention]]) decrements refs on a released
  snapshot's blobs and GCs any that reach 0, returning freed bytes. Closes the bounded lifecycle.

## Acceptance Criteria
- [x] `snapshot` module: `BlobStore` + `CaptureBudget` + `plan_capture` / `apply_capture` / `release`,
      reusing `restore_plan::{Snapshot, FileState}`.
- [x] Dedup: a hash under many paths, or already in the store, is stored once; `total_bytes` counts unique
      content only (`plan_dedups_repeated_content_within_a_scan`, `apply_dedups_store_footprint_across_snapshots`).
- [x] Bounded: per-file cap skips oversize files; store cap skips budget-exceeding new blobs — both reported
      as a `SkippedFile{path,size,reason}`, never silent (`per_file_cap_skips_oversize_content`,
      `store_cap_skips_new_blobs_that_would_overflow`); reused blobs never count against the cap.
- [x] Refcount lifecycle: capture→release round-trips the store to its prior footprint and GC frees freed
      bytes (`capture_then_release_round_trips_the_store`, `release_gcs_unreferenced_blobs_and_frees_bytes`);
      a blob shared by two snapshots survives releasing one (`a_shared_blob_survives_releasing_one_snapshot`).
- [x] Cargo-tested (11 tests); `clippy --all-targets -D warnings` clean both feature modes; not feature-
      gated, no new deps (pure std) — plain build unaffected.

## Work Log
- 2026-07-24 (dayshift) — Built `cpe-server::snapshot`. Reuses CPE-917's `Snapshot`/`FileState` so a
  checkpoint is a `Snapshot` + the blobs its hashes resolve to. Determinism via `BTreeMap`/hash-sorted
  iteration; `projected_total` makes the store cap account for a capture's own additions; budgets use `0 =
  unlimited` so existing callers are unbounded by default. `referenced_hashes()` is the snapshot's manifest
  for a later `release`. 11 tests, clippy clean both modes. The revert engine + timeline restore UI (the
  GUI/attended remainder of CPE-732) are still open.

## Notes
- Feeds `restore_plan` (CPE-917): a checkpoint = a `Snapshot` + the blobs its hashes resolve to in the
  store. The revert engine + timeline restore UI remain (GUI/attended) later in CPE-732.
- Not feature-gated — pure std, tiny, always-available domain logic like `restore_plan`/`snapshot_retention`.
