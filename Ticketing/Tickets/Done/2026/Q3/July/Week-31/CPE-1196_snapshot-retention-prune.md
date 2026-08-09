---
id: CPE-1196
title: "Wire snapshot retention: thin() → prune-to-budget command (preview + apply)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-735
---

## Summary
Part of CPE-735. The snapshot engine, store, and restore UI already ship (reused from CPE-732/1126). The pure
grandfather-father-son retention (`snapshot_retention::thin`, `RetentionPolicy` 24h/7d/4w/12m) exists but is
**wired to no command**. Add glue commands to prune a root's snapshots to a retention policy (+ optional
total-byte budget). Reuse the existing engine — no new capture/restore logic.

## Build
- New command(s) (in `checkpoint_store.rs` or a new `snapshot_prune.rs`) that: enumerate a root's manifests,
  run `snapshot_retention::thin` under a `RetentionPolicy`, return a keep/prune **preview** (non-destructive),
  and an **apply** that calls `snapshot_capture::prune` per losing manifest. Optional total-store-byte cap.
- Thin `#[tauri::command]` dispatchers in `src-tauri/src/lib.rs` (+ `generate_handler!`); **regenerate
  `bindings.gen.ts`**.
- Preserve the prune invariant (manifest-deleted-first / leak-over-corruption, snapshot_capture.rs:218-247).

## Acceptance Criteria
- [x] `cargo test -p cpe-server`: manifests at spread timestamps → apply keeps GFS survivors, removes the rest
      from `manifests/` + `index.json`, store bytes drop, survivors still `restore` byte-for-byte; preview is
      non-destructive.
- [x] clippy both feature modes clean; bindings regenerated (drift guard green).

## Build (as landed)
- New `crates/server/src/snapshot_prune.rs`: store-dir-based `preview(store_dir, policy)` (read-only) and
  `apply(store_dir, policy, max_total_bytes)` — runs `snapshot_retention::thin` over
  `snapshot_capture::list_manifests` (new pub fn), then `snapshot_capture::prune` per GFS loser. An optional
  `max_total_bytes` further thins survivors **oldest-first** after the GFS pass, but never below 1 survivor
  (never a silent full-wipe knob).
- `checkpoint_store.rs`: ctx-aware wrappers `checkpoint_prune_preview`/`checkpoint_prune_apply` (root →
  store dir → delegate to `snapshot_prune`).
- Thin Tauri dispatchers `snapshot_prune_preview` / `snapshot_prune_apply` in `src-tauri/src/lib.rs`,
  registered in `generate_handler!` + `collect_commands!`; `bindings.gen.ts` regenerated
  (`RetentionPreview`/`RetentionApplyResult` types).
- Prune invariant preserved untouched: this module never opens a manifest/index/blob directly, only calls
  `snapshot_capture::prune` (manifest-deleted-first, `snapshot_capture.rs:218-247`).
- `RetentionPolicy` gained `Deserialize` (was `Serialize`-only) so it round-trips as a command arg / stored
  rule field (needed by CPE-1198 too).

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-735). Backend batch (with 1197-backend + 1198, shared
  lib.rs/bindings), one worker sequential.
- 2026-07-31 — Implemented on branch `cpe-1196-1198-snapshot-backend`. `cargo test -p cpe-server` green
  (1149 tests incl. 4 new `snapshot_prune::` tests: non-destructive preview, GFS survivors restore
  byte-for-byte, byte-cap oldest-first eviction, never-prune-the-last-survivor). Clippy clean (default,
  `index`, `specta`). `npm run check` green with the regenerated bindings. Moved to Done.
