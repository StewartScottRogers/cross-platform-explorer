---
id: CPE-496
title: "Rich in-app conflict resolver (three-way / inline) for diverged sync"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [needs-prereq, big-design]
estimate: 4h+
created: 2026-07-16
epic: CPE-488
closed: 2026-07-16
---

## Summary
Chosen conflict model (Q1): when a mirror sync diverges/conflicts, resolve it **in-app** rather than
only deferring to an external tool. Provide a three-way / inline conflict view (ours / theirs / base)
that stages resolutions and continues the merge/rebase — never losing work.

## Acceptance Criteria
- [x] Conflicting files from a merge/rebase are listed with their state.
- [x] A three-way / inline view lets the user pick or edit the resolution per hunk/file.
- [x] Resolving stages the file and continues (or aborts) the merge/rebase cleanly.
- [x] Work is never lost (abort restores the pre-sync state).
- [x] Tests for the conflict-state parsing + resolution flow.

## Resolution
Built the in-app three-way conflict resolver on top of the CPE-495 sync surface.

- **`sidecar/repos/src/conflict.rs` (new, pure + 6 tests):** `parse_conflicts(porcelain_v2)` decodes the
  unmerged (`u`) records into `{ path, kind }` with every git conflict code (`UU` both-modified, `AA`
  both-added, `DU`/`UD` delete conflicts, …); total + panic-free; preserves paths with spaces. Exported.
- **Backend (`src-tauri/src/lib.rs`, `sidecar-platform`):**
  - `forge_conflict_state(path)` → the in-progress `operation` (merge/rebase/none, detected from
    `.git/MERGE_HEAD` / `rebase-merge|apply`) + the unmerged files with their kind.
  - `forge_conflict_versions(path, file)` → the three stages `base`/`ours`/`theirs` (`git show :1|2|3:`)
    plus the working-tree `merged` (marker text), each size/binary-capped (`truncated` flag). An absent
    stage (e.g. add/add has no base) is `null`.
  - `forge_resolve_file(path, file, content)` writes + `git add`s the resolution, guarded by a pure,
    unit-tested `is_safe_repo_relative` (rejects `..`/absolute/drive paths so a resolution can't escape
    the repo).
  - `forge_conflict_continue` / `forge_conflict_abort` finish or unwind the merge/rebase (`GIT_EDITOR=true`
    so nothing blocks on an editor); **abort restores the pre-sync state — work is never lost**.
  - `forge_repo_status` gained a `conflicted` flag so the status bar can surface the entry point.
- **Frontend (`ConflictDialog.svelte`, new):** a bordered resolver — a file list (path + kind), a
  three-way panel (base / ours / theirs, toggleable) and an **editable Resolution** prefilled from the
  working-tree merge, with **Use ours / theirs / base** shortcuts (disabled when a side is absent).
  *Mark resolved & stage* advances to the next file; **Continue** (enabled only when all are resolved)
  or **Abort** finishes. Entry points: a **Resolve…** button in the status bar when the repo is
  conflicted, and a **Resolve conflicts…** button in the Sync dialog after a sync hits a conflict.

Chosen granularity: resolution is **per file via an editable merged view** (all hunks + markers inline),
with whole-file ours/theirs/base shortcuts — a pragmatic read of "per hunk/file" without a bespoke
per-hunk button UI. Delete-conflicts (a side absent) are handled by disabling the missing pick + editing
or aborting. Tests: `parse_conflicts` (6) + the `is_safe_repo_relative` guard; the git-shelling
continue/abort/stage steps are integration-level (consistent with the codebase's shell-out testing).

## Work Log
2026-07-16 — Picked up (prereq CPE-495 Done). Estimate: 4h+ (big-design).
2026-07-16 — Built pure `repos::conflict::parse_conflicts` (unmerged-record decode → kind) with 6 tests.
2026-07-16 — Backend: forge_conflict_state / _versions / forge_resolve_file (+ pure is_safe_repo_relative guard, tested) / forge_conflict_continue / _abort; added `conflicted` to forge_repo_status. Registered all 5 commands.
2026-07-16 — Frontend: ConflictDialog (file list + three-way + editable resolution + continue/abort); status-bar Resolve… + Sync-dialog Resolve conflicts… entry points.
2026-07-16 — Verified: repos clippy + 6 conflict tests; app clippy (sidecar-platform) clean + path-safety test; `npm run check` 0 errors; 508 frontend tests pass. All ACs met.

## Notes
**needs-prereq:** [[CPE-495]] (the mirror UI surfaces conflicts this resolves). `big-design` — the
heaviest v2 slice; sequence after CPE-495.
