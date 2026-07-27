---
title: "How to build the next wave of CPE-732 (checkpoint & rollback of agent work)?"
date: 2026-07-26
tags: [checkpoint, rollback, cpe-732, agent-watch, snapshot, revert, blobstore, headless, cpe-1123, cpe-1124, cpe-1125]
status: current
---

## Decision (PM, 2026-07-26): activate CPE-732 next — highest-impact HEADLESS epic.
Completes the Agent-Watch trilogy (CPE-730 radar + CPE-731 cost both closed this shift). ~85% headless.

## Key insight: the entire engine is BUILT + cargo-tested but wired to ZERO commands
Verified: `grep checkpoint_|restore_plan|snapshot_capture|revert_engine src-tauri/src/lib.rs` = 0. Dead-but-tested
code in `crates/server/src/`:
- `snapshot.rs` — Snapshot + `BlobStore` dedup content store (CPE-969)
- `snapshot_capture.rs` — `capture` / `restore` / `prune` / `scan_dir`
- `restore_plan.rs` — `plan_restore` + `summarize_plan`
- `revert_engine.rs` — `execute_restore`
- `revert_safety.rs` — drift detection; `revert_attribution.rs`; `snapshot_retention.rs`
So the next wave is pure **integration + orchestration + e2e tests**, all headless-verifiable in a temp tree.

## Store schema — mirror the audit_journal pattern (no spike needed)
The one open unknown (checkpoint store dir + index) has a proven template: `audit_journal.rs` /
`metrics_journal` (JSONL/JSON under the app-data dir, keyed per root). New `checkpoint_store.rs`:
`checkpoint_create` captures via `snapshot_capture::capture` into a per-root store dir + appends
`{manifest_id,label,ts}` to a tolerant-read `checkpoints.json`; `checkpoint_list` reads it. Go through `ServerCtx`.

## Slices (filed)
- **CPE-1123 (backend, opus)** — store + thin `lib.rs` commands `checkpoint_create/list/preview_revert/revert/
  revert_one` + bindings regen. (PM A+B+C combined — one lib.rs command seam = one PR.)
- **CPE-1124 (backend-test, sonnet, PARALLEL to 1123)** — `crates/server/tests/checkpoint_roundtrip.rs`: capture →
  mutate → plan/drift → revert → byte-match, + skip-unreadable. Tests engines directly (independent of 1123).
- **CPE-1125 (frontend, sonnet, AFTER 1123)** — palette `tool.checkpoint` + self-maintaining docs (sectionDocs).
- **CPE-1126 (DEFERRED GUI cap)** — restore panel + timeline markers; needs a user-present GUI session (on the
  QA MVD ledger).

## Ordering / parallelism
Wave 1: CPE-1123 (opus) ∥ CPE-1124 (sonnet) — disjoint (lib.rs+checkpoint_store.rs vs new tests/ file). Wave 2:
CPE-1125 (frontend) after 1123 merges (needs commands+bindings). No prerequisite spike. No new deps; reuse
engines; lean `lib.rs` dispatcher; async+spawn_blocking; off-means-off.

## Runners-up (why not, per PM)
CPE-717 native metadata (Low pri, mostly done) · CPE-997 near-dup (too thin, core shipped) · CPE-705 archive
(remaining work is GUI) · CPE-718 thumbnails (dep-heavy + needs visual eyes) · CPE-976/977 semantic/copilot
(gated on an embedding/LLM model = user resource).
