---
id: CPE-535
title: "Workbench — friendly edge cases (not-a-repo / no-changes / git-missing)"
type: Bug
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-505
closed: 2026-07-16
estimate: 1-2h
created: 2026-07-16
---

## Summary
The Workbench ([[CPE-526]]) runs a real `git diff`, but its edge cases are rough: on a **non-git
folder** it surfaces git's raw `fatal: not a git repository` in the status line instead of a friendly
message, and it can't distinguish **no repo** from **no changes** from **git not installed**. Flesh it
out with clear, user-friendly states.

## Acceptance Criteria
- [x] `workbench_diff` returns a **structured** result (`is_repo`, `branch`, `diff`) — a non-repo folder
      is a normal (not-a-repo) result, not an error.
- [x] The view shows distinct, friendly states: **not a Git repository** (name the folder + a hint),
      **no changes** ("{branch} matches HEAD"), a real **error** (git failed), and **git not installed**.
- [x] Opening the Workbench on Home / no folder shows "open a folder first" rather than a git error.
- [x] The branch name shows in the header/status bar when it is a repo.
- [x] Tests for the state-selection logic (pure helper).

## Notes
Fix within [[CPE-505]] / [[CPE-526]]. Raised by the user (2026-07-16): "is the Workbench working… does
not tell us that the repository is not connected — flush it out, make it more user friendly on the edge
cases."

## Resolution
Fleshed out the Workbench's edge cases with clear, friendly states.

- **Backend:** `workbench_diff` now returns a **structured** `WorkbenchDiff { is_repo, branch, diff }`.
  It first runs `git rev-parse --is-inside-work-tree`: a **non-repo** folder is a normal
  `is_repo:false` result (not an error), **git-not-installed** is a distinct `git-missing` error, and an
  empty `root` returns `no-folder`. When it is a repo it also reports the **branch**.
- **`workbench.ts` `workbenchState`** (pure, tested): maps the load result → `loading` / `no-folder` /
  `git-missing` / `not-a-repo` / `error` / `clean` / `changes` (loading > error > repo-state priority).
- **WorkbenchView:** distinct friendly bodies per state — "Open a folder first", "Git isn't available",
  "**Not a Git repository** — `<folder>` … open a repo or clone one from Repositories", a real error
  card, "✓ No changes — **<branch>** matches HEAD", or the diff. The **branch** shows in the title;
  stats show only when there are changes.

Raised by the user — the Workbench no longer dumps git's raw `fatal: not a git repository`. `npm run
check` + app clippy clean; 549 frontend tests pass (2 new state tests). Fix within CPE-505/CPE-526.

## Work Log
2026-07-16 — Filed + picked up (user-raised). Structured workbench_diff (is_repo/branch), pure workbenchState (7 states, tested), friendly WorkbenchView bodies + branch in title. npm check + clippy clean; 549 tests. All ACs met.
