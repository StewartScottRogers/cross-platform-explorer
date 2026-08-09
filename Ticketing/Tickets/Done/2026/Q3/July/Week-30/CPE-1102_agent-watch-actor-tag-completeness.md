---
id: CPE-1102
title: "Agent Watch: extend 'user' actor-tagging to the remaining mutation commands"
type: bug
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
Fast-follow from the CPE-1101 review. CPE-1101 tags app-initiated writes as `actor:"user"` at **8** file-op
commands, but other user-initiated mutations that fire watcher events inside a watched folder are NOT tagged —
so they're mis-attributed to the owning agent session (or `"unknown"`), which will mislead the CPE-1100
conflict radar. Extend `note_app_op` coverage to the remaining sites for consistent attribution.

## Sites to cover (from the reviewer, file:line approx — reconfirm)
- **`delete_permanent` (`src-tauri/src/lib.rs:~1313`)** — the glaring one: its sibling `delete_to_trash` IS
  tagged `"user"`, so a trash-delete reads "user" but a Shift+Del permanent-delete reads as the agent. Fix first.
- `run_transfer` / `run_watch_actions` (`~:1710` / `~:1485`) — the progress-reporting copy/move engine (note:
  these run async with their own progress; record the destination paths before the transfer, mindful of the
  streaming nature — may need per-item recording or a coarse root record).
- `extract_archive` / `extract_archive_entry` (`~:3119` / `~:3098`) — extracted output paths.
- `template_stamp` (`~:2667`) — stamped output path(s).

## Design
Reuse the existing `note_app_op(app, || <target paths>)` helper (no-op without `sidecar-platform`, no-thread
ledger) from CPE-1101 — call it in each command's async wrapper before the mutation, mirroring the 8 existing
sites. For streaming transfers, record the planned destination paths up front (best-effort; an
auto-rename-on-collision dest may miss and fall back to the session id, which is still honest).

## ⚠ Notes
- Same guardrails as CPE-1101: no thread/timer, no-op without the feature (plain build untouched), no new deps.
- `normalize_op_path` is separator+case only (not 8.3/symlink canonical) — a differently-canonicalized watcher
  spelling silently falls back to the session id. Documented best-effort boundary; out of scope unless a real
  mismatch is observed.

## Acceptance Criteria
- [ ] `delete_permanent`, the transfer engine, archive extraction, and template stamping tag their outputs
      `"user"` via `note_app_op` (consistent with `delete_to_trash`/rename/copy/move/create/write).
- [ ] Plain build still compiles + `cargo test` (default) unaffected; clippy clean both modes; no new deps/thread.
- [ ] A test (or extension of the CPE-1101 actor test) covers at least the `delete_permanent` = "user" case.

## Work Log
2026-07-26 (sprint, GUI) — Filed from the CPE-1101 reviewer's non-blocking finding #1 (attribution
completeness). Low priority; improves conflict-radar (CPE-1100) accuracy. The `delete_permanent` vs
`delete_to_trash` inconsistency is the most user-visible piece.
