---
id: CPE-1196
title: "Wire snapshot retention: thin() → prune-to-budget command (preview + apply)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [ ] `cargo test -p cpe-server`: manifests at spread timestamps → apply keeps GFS survivors, removes the rest
      from `manifests/` + `index.json`, store bytes drop, survivors still `restore` byte-for-byte; preview is
      non-destructive.
- [ ] clippy both feature modes clean; bindings regenerated (drift guard green).

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-735). Backend batch (with 1197-backend + 1198, shared
  lib.rs/bindings), one worker sequential.
