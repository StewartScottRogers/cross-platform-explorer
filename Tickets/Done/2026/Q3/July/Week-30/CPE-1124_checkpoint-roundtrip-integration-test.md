---
id: CPE-1124
title: "Checkpoint & rollback: engine round-trip integration test"
type: test
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-26
epic: CPE-732
---

## Summary
CPE-732 first wave (PM slice D). An end-to-end **round-trip** integration test at the `cpe-server` ENGINE level
(the engines already exist + are unit-tested; this proves they compose correctly end-to-end). Independent of the
command-layer ticket (CPE-1123) — tests the engines directly, so it runs in parallel and validates the risky
part NOW. New test file, zero collision with any other ticket.

## What to build
`crates/server/tests/checkpoint_roundtrip.rs` (new): in a temp tree —
1. `snapshot_capture::capture` a baseline (mix of files + nested dirs).
2. Mutate: create new files, overwrite existing, delete some, rename one.
3. `restore_plan::plan_restore` → assert the plan lists the right create/overwrite/delete/rename reversions;
   `revert_safety` → assert the drift report counts the out-of-checkpoint changes.
4. `revert_engine::execute_restore` → assert the tree now **byte-matches** the captured baseline.
5. A skip-unreadable path case → assert it's skipped (not a hard failure), consistent with the `list_dir`
   skip-on-error guardrail.

## ⚠ Guardrails
- Test-only; no production changes, no new deps. Use the existing engine APIs (read their signatures/tests). Do
  NOT assert exact filesystem byte *sizes* across OSes (the 3-OS CI matrix varies) — assert content equality /
  presence/absence, not stat sizes. Real assertions, not hollow.

## Acceptance Criteria
- [ ] A temp-tree capture → mutate → plan/drift → revert → byte-match round-trip test passes, plus a
      skip-unreadable case. `cargo test` green on the 3-OS matrix; `cargo clippy -D warnings` clean; no new deps.

## Work Log
2026-07-26 (workshift) — CPE-732 first wave (PM slice D). Parallel-safe (new test file); validates the pre-built
engines compose correctly before/alongside the command layer (CPE-1123). Dispatched to a sonnet worker.

2026-07-26 (workshift) — Built (PR #438, merged 5636e939). Reviewer APPROVE + UAT PASS. UAT ran a MUTATION test (broke apply_delete -> test failed as expected -> reverted), proving the assertions catch real regressions. Covers capture->mutate(incl rename=Create+Delete)->plan->drift(5 Conflict unattributed -> 5 Safe when attributed)->revert->byte-match Snapshot equality, + portable skip-unreadable (missing-blob). No prod changes, no new deps.
