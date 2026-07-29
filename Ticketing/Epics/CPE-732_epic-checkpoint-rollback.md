---
id: CPE-732
title: "EPIC: Checkpoint & rollback of agent work"
type: Task
status: In Progress
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Let the user snapshot the watched tree at moments during a session and one-click revert the agent's
filesystem changes back to any checkpoint, or cherry-revert a single mutation from the timeline.

## Why
The safety net that makes watching an agent low-stakes: if it goes wrong, undo it. Complements diff-peek and
audit by making the history not just visible but reversible.

## Rough scope (areas, not child tickets)
- Efficient content-addressed snapshots of the watched subtree (bounded, dedup).
- A revert engine that respects the "skip unreadable entries" rule.
- Checkpoint markers on the timeline + a restore UI.
- Single-mutation cherry-revert from a timeline entry.

## Open questions (resolve at activation)
- Snapshot storage/size budget and dedup strategy for large trees.
- Shared snapshot store with diff-peek ([[CPE-727]]) and replay ([[CPE-728]]).
- Revert safety when files changed outside the agent since the checkpoint.

## Definition of Done
- Users can checkpoint the watched tree and one-click revert the agent's changes to any checkpoint.
- A single mutation can be cherry-reverted from the timeline.
- Snapshots are bounded/deduped; revert respects unreadable-entry handling.

## Work Log
2026-07-22 (nightshift) — **Activated.** First slice: **CPE-917** — `restore_plan::plan_restore` /
`revert_one` / `summarize_plan`: the pure diff that turns a checkpoint snapshot + current tree state into the
minimal Create/Overwrite/Delete revert ops (+ cherry-revert of one path). Remaining: content-addressed
snapshot capture/store (dedup, bounded), the revert engine, and the timeline checkpoint-marker + restore UI.

2026-07-24 (dayshift) — **CPE-969** landed the content-addressed **snapshot store**:
`cpe-server::snapshot` — a refcounted, deduplicated `BlobStore` + `plan_capture`/`apply_capture`/`release`
under a per-file + whole-store byte `CaptureBudget` (the "bounded, dedup" DoD). Reuses CPE-917's
`Snapshot`/`FileState`, so a checkpoint = a `Snapshot` + the blobs its hashes resolve to. 11 tests, pure std.
**Remaining (GUI/attended):** the revert **engine** (execute a `restore_plan`, skip-unreadable) and the
timeline checkpoint-marker + restore UI.

2026-07-27 (workshift) — **Headless scope COMPLETE.** The pre-built engines (snapshot/snapshot_capture/
restore_plan/revert_engine/revert_safety/BlobStore) are now wired into a live feature:
- CPE-1123 command layer — per-root checkpoint store (SHA-256 key, tolerant JSONL index mirroring audit_journal)
  + 5 commands (create/list/preview_revert/revert/revert_one), revert write-safe (safe_segments). PR #439.
- CPE-1124 engine round-trip integration test (capture→mutate→plan/drift→revert→byte-match + skip-unreadable),
  mutation-proven. PR #438.
- CPE-1125 palette action tool.checkpoint + CheckpointDialog (confirm-before-revert) + docs 16-checkpoints. PR #441.
- CPE-1127 manifest_id path-traversal hardening (read path). PR #440.
Each passed an independent Reviewer + UAT gauntlet; every merge CI-green.
**Still open (epic stays In Progress):** CPE-1126 — the rich visual **restore panel + timeline checkpoint
markers** (the attended GUI cap, ~15%), DEFERRED to a user-present GUI-verification session (on the QA MVD
ledger). Optional headless follow-up: thread revert_attribution into checkpoint_preview_revert so drift flags
only truly-outside changes (currently conservative "warn about everything", documented).
