---
id: CPE-1011
title: Disk-backed snapshot capture & restore engine (content-addressed, deduped)
type: feature
component: Backend
priority: medium
tags: ready
status: Doing
created: 2026-07-24
epic: CPE-735
estimate: 3-4h
---

## Summary
The CPE-735 (local snapshots) epic's remaining **core**. `crates/server/src/snapshot.rs` (CPE-969) is only
the **pure in-memory bookkeeping half** — a refcounted `BlobStore` + `plan_capture`/`apply_capture`/`release`
— and its own docs say *"the bytes behind each hash are the caller's to persist."* Nothing persists them yet.
This ticket adds the **disk-backed engine** that walks a folder, hashes + persists blobs content-addressed on
disk (deduped), records a snapshot manifest, and **restores** a snapshot back to a directory byte-for-byte.
Fully headless — verified by tempdir round-trip tests, no GUI.

## Scope
New module `crates/server/src/snapshot_capture.rs` (declare `pub mod snapshot_capture;` in `lib.rs`),
building **on top of** the existing pure pieces — do **not** reinvent them:
- `crate::snapshot::{BlobStore, CaptureBudget, plan_capture, apply_capture, release}` — the dedup/refcount core.
- `crate::restore_plan::{Snapshot, FileState}` — `Snapshot = BTreeMap<String, FileState>` (`path → {hash,size}`).
- `crate::checksum::hash_file` (sha256) for hashing — reuse it; no new hash impl, no new deps (std + serde only).

Provide roughly:
- `scan_dir(root) -> Result<Snapshot, String>` — walk `root` recursively, hash each **file** via
  `checksum::hash_file`, key by path **relative to `root`** (forward-slash, stable/sorted). Skip entries it
  can't read rather than failing the whole scan (mirror the `list_dir` skip-on-error guardrail). Symlinks:
  do not follow into dirs (loop-safe); record or skip link files sensibly and document the choice.
- A store layout on disk: `store_dir/blobs/<hash>` for blob bytes + a persisted `BlobStore` index and the
  per-snapshot manifests as JSON (serde_json is already a workspace dep). Content-addressed ⇒ writing a blob
  whose file already exists is a no-op (the dedup win, matching `plan_capture`'s `reused`).
- `capture(root, store_dir, budget) -> Result<CaptureOutcome, String>` — `scan_dir` → `plan_capture` against
  the loaded store → write each `to_store` blob's bytes to `store_dir/blobs/<hash>` → `apply_capture` →
  persist the store index + a new manifest (id/time/`path→hash` map + the plan's `skipped` list). Return an
  outcome summarising new/reused/skipped counts + bytes added (surface skipped files — never silently drop).
- `restore(store_dir, manifest_id, dest) -> Result<(), String>` — recreate every manifest path under `dest`
  from `store_dir/blobs/<hash>`, creating parent dirs. Byte-for-byte identical to the captured content.
- (Optional, if cheap) `prune(store_dir, manifest_id)` using `release` to GC unreferenced blobs.

Keep it **std + serde only** (no new crates). CoW/reflink/hardlink optimisation is **out of scope** — v1 is
plain copy-into-store + copy-out; note that in the work log as a deliberate deferral.

## Acceptance Criteria
- [x] `snapshot_capture` module compiles and is declared in `lib.rs`; not feature-gated (always available,
      like `snapshot`/`snapshot_retention`).
- [x] Round-trip test: capture a tempdir tree → `restore` into a fresh tempdir → every file byte-for-byte
      equal (including nested dirs).
- [x] Dedup test: two identical-content files in one capture write **one** blob; a second capture of an
      unchanged tree writes **zero** new blobs (all `reused`) and the store footprint is unchanged.
- [x] Budget/skip test: an oversize file (per `CaptureBudget`) is reported in `skipped`, not stored, and the
      rest of the capture still succeeds.
- [x] Skip-on-error preserved: an unreadable entry doesn't fail the whole scan/capture.
- [x] `cargo test -p cpe-server snapshot_capture` green; `cargo clippy --all-targets -D warnings` clean in
      **both** feature modes; no new dependency added to any Cargo.toml.

## Notes
- Grep-first done (Foreman): no disk-backed snapshot capture/restore exists anywhere (`snapshot.rs` is pure;
  no `capture`/`restore`/`write_blob` fn in `crates/` or `src-tauri/`). Safe to build fresh.
- This is backend-only; the timeline/diff/restore **UI** is a later, attended child of CPE-735.
- Follow the repo conventions: domain logic in `cpe-server` (not `lib.rs`); a Tauri command wrapper is a
  separate later slice — this ticket is the engine + tests only.

## Work Log
- 2026-07-24 — Built `crates/server/src/snapshot_capture.rs` end-to-end: `scan_dir`, `capture`, `restore`,
  and an optional `prune`, on top of `snapshot.rs`'s `BlobStore`/`plan_capture`/`apply_capture`/`release`
  (unmodified except one additive constructor, see below) and `checksum::hash_file`.
  - **On-disk layout**: `store_dir/blobs/<hash>` (one file per unique content hash), `store_dir/index.json`
    (persisted `BlobStore` index: hash → `{size,refs}`), `store_dir/manifests/<manifest_id>.json` (one per
    capture: id, epoch-ms time, `path → {hash,size}` map, and the skipped-file list). All JSON via
    `serde_json` (already a workspace dep) — no new crates.
  - **One additive change to `snapshot.rs`**: `BlobStore`'s `blobs` field is private to that module, so a
    disk-backed loader had no way to rebuild a store from a persisted index. Added
    `BlobStore::from_index(BTreeMap<String, BlobMeta>) -> Self` — a public constructor, not a behaviour
    change to any existing function — plus one new unit test in `snapshot.rs` proving it round-trips
    (`from_index_reconstructs_an_equivalent_store`). No existing `snapshot.rs` test touched or weakened.
  - **Symlinks**: not followed, and not recorded. `scan_dir` reuses `checksum.rs`'s exact technique —
    `DirEntry::metadata()` doesn't traverse symlinks, so a symlinked file or directory is neither `is_dir()`
    nor `is_file()` and simply falls through unhandled. This is loop-safe by construction (no separate
    cycle-detection needed) and matches the sibling `checksum_folder`/`folder_stats`/`disk_usage` walkers'
    established convention in this crate.
  - **Skip-on-error**: an unreadable directory, entry, or file during `scan_dir` is skipped, not fatal
    (mirrors `list_dir`). A file that becomes unreadable *after* the scan but during `capture`'s blob-copy
    step **does** fail that capture call — deliberately, since silently persisting a manifest that
    references a blob nothing ever wrote would be worse than erroring on a narrow race; blob writes always
    happen before the store index/manifest are persisted, so a mid-capture failure never corrupts on-disk
    state (it may leave a stray unreferenced blob file, which a future GC pass or retried capture cleans up
    for free since content-addressed writes are idempotent).
  - **Dedup extends to disk**: writing a blob whose file already exists at `blobs/<hash>` is a no-op —
    verified by a two-capture test where the second capture of an unchanged tree adds zero new blobs and
    leaves the store's byte footprint and blob count unchanged.
  - **Manifest id scheme**: the capture's wall-clock epoch-ms (via the existing `fsutil::to_epoch_ms`), with
    a `-N` suffix appended on collision (two captures inside the same millisecond) — sortable and guaranteed
    unique. Verified two back-to-back captures in the same test process always get distinct ids.
  - **`capture`'s manifest-filtering rule**: a path is included in the manifest iff its content's hash ended
    up `to_store` or `reused` (never `skipped`) — computed via `plan.referenced_hashes()`. This correctly
    excludes *every* path sharing a skipped hash, not just the one `plan.skipped` blames, which the ticket's
    own `SkippedFile` type doesn't carry a hash for.
  - **`prune` (optional, included)**: releases a manifest's hold on its blobs via the existing `release`,
    deletes now-unreferenced blob files, rewrites the index, and deletes the manifest file. Tested: a blob
    unique to the pruned manifest is freed and removed; a blob shared with a surviving manifest is kept and
    that manifest still restores correctly afterward.
  - **CoW/reflink/hardlink**: deliberately out of scope for v1, as directed — blobs are plain-copied in
    (`capture`) and out (`restore`) via `std::fs::copy`. A later ticket can swap the copy primitive without
    touching this module's public API.
  - **Tests** (`crates/server/src/snapshot_capture.rs`, `cargo test -p cpe-server snapshot_capture`):
    non-folder/missing root rejected; forward-slash relative-path keying (nested dirs); symlinked-directory
    loop is never descended (privileged-symlink-gated, skips gracefully on unprivileged Windows, matching
    `disk_usage`'s existing pattern); dangling symlink doesn't fail the walk; unix-only unreadable-file skip
    test (`#[cfg(unix)]`, chmod 0o000); byte-for-byte nested round-trip; unknown-manifest restore is an
    error; dedup + zero-new-blobs recapture; oversize file skipped while the rest of the capture succeeds
    (and restore correctly omits it); prune GC. 9/9 pass on Windows (the one `#[cfg(unix)]` test is compiled
    out here, as expected); `snapshot.rs`'s own 12 tests (11 existing + 1 new) still pass unchanged.
  - **Verified** (PowerShell, `crates/server`, `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"`):
    `cargo build -p cpe-server` — clean. `cargo test snapshot_capture` — 9 passed, 0 failed. `cargo test -p
    cpe-server snapshot::` — 12 passed, 0 failed. `cargo clippy --all-targets -- -D warnings` — clean.
    `cargo clippy --all-targets --all-features -- -D warnings` — clean. No Windows Defender os-error-225
    issue hit this run. `git diff --stat` confirms no `Cargo.toml` anywhere was touched.
  - Opened PR `CPE-1011: disk-backed snapshot capture & restore engine` from branch
    `cpe-1011-snapshot-capture-engine`. Status left as `Doing` pending review/merge (this worker doesn't own
    moving the ticket to `Done`).
- 2026-07-24 (review fix) — Independent review flagged one real defect in `prune()`: it did
  `release` → `save_store` → `remove_file(manifest)`, so if the final manifest delete failed after the
  store was already saved, a retry would `load_manifest` (still present) and `release` the same hashes a
  **second** time, double-decrementing a shared blob's refcount to 0 and deleting content another snapshot
  still needs (silent data loss — violating the `prune_gcs...keeps_shared_ones` guarantee). Fixed by
  reordering so the manifest `remove_file` is the **point of no return, done first** (read the manifest into
  memory, delete the manifest file, then load store / release / remove unreferenced blobs / save store). Now
  a retry-after-failure is always safe: manifest-delete fails → nothing else changed → clean retry;
  manifest-delete succeeds but a later step fails → the manifest is already gone so no second `release` can
  run → residue is only a refcount/space leak, never data loss (the same "leak over corruption" tradeoff
  `capture` already makes). Added a doc-comment paragraph on `prune` explaining the ordering rationale. Did
  NOT touch `scan_dir`/`capture`/`restore`/`from_index` (confirmed correct by the review). Re-verified:
  `cargo test snapshot_capture` 9/9, `cargo test snapshot::` 12/12, `cargo clippy --all-targets -- -D
  warnings` clean, `cargo clippy --all-targets --all-features -- -D warnings` clean. Pushed to PR #336.
