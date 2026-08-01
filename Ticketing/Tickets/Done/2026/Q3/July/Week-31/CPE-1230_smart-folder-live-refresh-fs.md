---
id: CPE-1230
title: "Smart folders: live-refresh on filesystem change (not just tag change)"
type: Task
priority: Medium
component: Multiple
tags: [ready]
created: 2026-08-01
epic: CPE-978
closed:
prereq: CPE-1229
---

## Context
Today an open smart folder recomputes reactively on `$tags` changes but NOT on filesystem changes
(create/delete/rename on disk that would change matches). DoD requires "refreshed as files change —
no manual re-run". Wire smart-folder recompute to the existing folder-watch / CPE-833 index-watch
signals.

## Acceptance criteria
- While a smart folder (tag-only OR structured) is open, a relevant filesystem change under its scope
  triggers a recompute + refreshed result view, without a manual re-run.
- Reuse existing watch signals (`crates/server/src/index_watch.rs` / CPE-833 / folder-watch) — do NOT
  add a new watcher. Debounced/streamed for big result sets (STREAMING.md).
- No always-on cost when no smart folder is open.
- REAL test(s) for the recompute-on-change wiring (headless core).

## Notes
Prereq CPE-1229 (the structured open-evaluator). Core is headless; live GUI verify is user-gated.
