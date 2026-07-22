---
id: CPE-462
title: "Two-way forge mirror — git status + Pull/Push in the explorer"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
The forge epic's remaining thread: two-way mirror. First safe increment — the explorer becomes
git-aware. When the current folder is a git repo, the status bar shows branch + ahead/behind + dirty
and offers **Pull** (fast-forward) and **Push** (no force), driven by the already-built + tested
sync planner (CPE-438) and status parser (CPE-437).

## Acceptance Criteria
- [x] Host `forge_repo_status(path)` runs `git status --porcelain=v2 --branch`, parses it
      (`repos::parse_status`), and plans a **safe** sync (`repos::plan_sync`, never force); returns
      branch/ahead/behind/dirty + the planned actions. Non-repo → `is_repo:false`.
- [x] Host `forge_sync(path, action)` executes only safe steps: `pull` = `--ff-only`, `push` = no
      force; anything that could rewrite history is refused (diverged surfaces in the status).
- [x] The status bar shows the git segment (branch, ↑ahead ↓behind, dirty dot) + Pull/Push when
      applicable; Pull/Push call the host and re-list.
- [x] Graceful in the plain build (command absent → no git segment). Tests: StatusBar git segment +
      dispatch (3 jsdom); the parse/plan logic is unit-tested in the repos crate.

## Resolution
Host commands `forge_repo_status` + `forge_sync` (feature-gated, reuse the `repos` crate's tested
`parse_status`/`plan_sync`). `StatusBar.svelte` gains a git segment (branch + ahead/behind + dirty +
Pull/Push) driven by App.svelte, which fetches `forge_repo_status` on folder change and runs
`forge_sync` on the buttons (then re-lists). Safe-by-default: Pull is ff-only, Push never forces; a
diverged history shows but isn't auto-reconciled. 3 StatusBar jsdom tests; svelte-check 0, host clippy
clean both feature modes, frontend suite 441 green.

## Follow-ups (the mirror continues under CPE-429)
Diverged **merge/rebase** execution (the planner already plans it), a commit affordance for the dirty
state, conflict surfacing, and full auto-mirror. Live git behaviour is GUI/runtime-verified.
