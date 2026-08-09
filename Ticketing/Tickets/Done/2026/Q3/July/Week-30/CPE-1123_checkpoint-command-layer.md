---
id: CPE-1123
title: "Checkpoint & rollback: command layer (store + create/list/preview/revert)"
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-26
epic: CPE-732
---

## Summary
CPE-732 first wave (PM slices A+B+C, combined — they share the `lib.rs` command seam so they're one PR). The
entire checkpoint/rollback *engine* already exists and is cargo-tested in `crates/server/src/` but is wired to
**zero commands** (verified: `grep checkpoint_|restore_plan|snapshot_capture|revert_engine src-tauri/src/lib.rs`
= 0). Wire it into a live command layer + a per-root checkpoint store. BACKEND (cpe-server + thin `lib.rs`
dispatchers + bindings regen).

## Existing engine to wire (READ these — do NOT reimplement)
`crates/server/src/`: `snapshot.rs` (Snapshot + BlobStore dedup content store, CPE-969), `snapshot_capture.rs`
(`capture`/`restore`/`prune`/`scan_dir`), `restore_plan.rs` (`plan_restore` + `summarize_plan`),
`revert_engine.rs` (`execute_restore`), `revert_safety.rs` (drift detection), `revert_attribution.rs`,
`snapshot_retention.rs`. Read their signatures + tests before wiring.

## What to build
1. **`crates/server/src/checkpoint_store.rs` (new)** — a per-watched-root checkpoint store: `checkpoint_create`
   captures the tree via `snapshot_capture::capture` into a store dir and appends `{manifest_id, label, ts}` to a
   **tolerant-read** `checkpoints.json` index; `checkpoint_list` reads the index. **Mirror the on-disk pattern of
   `audit_journal.rs` / metrics journal** (JSONL/JSON under the app-data dir, keyed per root) — resolve the store
   location + index schema inline (no separate spike; that pattern is the proven template). Go through the
   `ServerCtx` seam per SERVER-ARCHITECTURE.md.
2. **Thin `#[tauri::command]` dispatchers in `src-tauri/src/lib.rs`** (one-liners into cpe-server; register in
   `generate_handler!`), async + `spawn_blocking` per the async-commands guardrail:
   - `checkpoint_create(root, label)` / `checkpoint_list(root)`.
   - `checkpoint_preview_revert(root, manifest_id)` → `restore_plan::plan_restore` + `summarize_plan` + a
     `revert_safety` drift report ("N files changed outside since checkpoint") so the UI can warn first.
   - `checkpoint_revert(root, manifest_id)` → plan + `revert_engine::execute_restore`, honour skip-unreadable,
     return an `OpResult`-style summary.
   - `checkpoint_revert_one(root, manifest_id, path)` → cherry-revert a single path.
   - Regen `bindings.gen.ts`.

## ⚠ Guardrails
- Reuse the existing engines — no new algorithm logic, no new deps. Domain logic in `cpe-server`, `lib.rs` stays a
  thin dispatcher (SERVER-ARCHITECTURE.md). Async + `spawn_blocking` for all fs/blocking commands. Preserve
  skip-on-error. Off-means-off (no cost when not used). This is the ONLY `lib.rs`-touching / bindings-regen ticket
  in its wave — keep it that way.

## Acceptance Criteria
- [ ] `checkpoint_create`/`checkpoint_list`/`checkpoint_preview_revert`/`checkpoint_revert`/`checkpoint_revert_one`
      exist, registered, and dispatch into cpe-server; bindings regenerated.
- [ ] A checkpoint captures the tree into a per-root store + a tolerant-read index; list returns them; preview
      returns a plan + drift report; revert restores the tree (skip-unreadable honoured) and returns a summary.
- [ ] A command-level integration test covers create → mutate → preview (plan+drift) → revert. `cargo test` +
      `cargo clippy --all-targets -D warnings` (both feature modes) green; `npm run check` green; no new deps.

## Work Log
2026-07-26 (sprint) — CPE-732 first wave (PM slices A+B+C). Engine pre-built+tested but unwired; this is pure
integration. Dispatched to an opus worker (integration + orchestration on the ServerCtx/lib.rs seam).

2026-07-27 (sprint) — Built (PR #439, merged da598e7d). Opus Reviewer APPROVE + UAT PASS: 5 commands (create/list/preview_revert/revert/revert_one) wired to the pre-built engines; per-root store keyed SHA-256(abs root); tolerant JSONL index (skip-malformed, newest-first) mirroring audit_journal; revert write-safe (safe_segments rejects ../abs/drive — even a poisoned manifest can't escape root); skip-on-error preserved; revert_one proven no over-reach; bindings regen'd (5 methods+5 types); clippy clean all 4 feature-mode combos. Non-blocking nit -> CPE-1127 (harden manifest_id read path). Drift is conservative (empty agent-touched set) until attribution threaded — documented.
