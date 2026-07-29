---
id: CPE-1128
title: Restructure Tickets/ into a Ticketing/ container with Epics & Sprints as sibling queues
type: Task
status: Done
priority: Medium
component: Multiple
estimate: 1h
created: 2026-07-29
closed: 2026-07-29
---

## Summary

The `Tickets/` folder conflated two orthogonal axes on one directory level: the ticket **status
flow** (`Backlog → Doing → Blocked/Deferred → Done`) and the two **separate queues** (`Epics`,
`Sprints`). Structurally, `Epics/` and `Sprints/` sat as peers of `Backlog/`/`Done/`, implying they
were just more status buckets — contradicting the docs, which call each a "separate queue" orthogonal
to the lifecycle.

This introduces a `Ticketing/` container holding three siblings — the status-flow `Tickets/`, plus
`Epics/` and `Sprints/` — and lifts the system-level `wiki.md` + `_template.md` to the container root.

## Acceptance Criteria

- [x] `Ticketing/{Tickets/, Epics/, Sprints/, wiki.md, _template.md}` layout via history-preserving `git mv`
- [x] Backend `cpe-server` + `lib.rs` scan the new paths (status columns under `Ticketing/Tickets/`,
      epics under `Ticketing/Epics/`, recursive card lookup rooted at `Ticketing/`)
- [x] `nearest_project_root` detects a project by its `Ticketing/` folder (was `Tickets/`)
- [x] All five `/ticketing-*` slash commands reference the new paths
- [x] `CLAUDE.md`, `wiki.md`, `scripts/organize-done.mjs`, and the SessionStart hook updated
- [x] `cargo check` + `npm run check` pass; no stale `Tickets/{Epics,Sprints,Backlog,…}` refs outside UAT copies

## Resolution

Created `Ticketing/` as the container. `git mv Tickets Ticketing/Tickets`, then lifted `Epics/`,
`Sprints/`, `wiki.md`, and `_template.md` up to `Ticketing/`. Updated the backend path joins and the
`nearest_project_root` detection key, the five slash commands, the layout docs, the `organize-done.mjs`
archive script, and the card-detail `location` prefix. The plain explorer is untouched; only the
ticket-management surface moved.

## Work Log

- 2026-07-29 — Moved the tree; updating backend + docs + tooling references; verifying build.

## Notes

Asymmetry to remember: `Epics`/`Sprints` moved **up** (`Tickets/Epics → Ticketing/Epics`) while the
status folders moved **down** (`Tickets/Backlog → Ticketing/Tickets/Backlog`). The `.claude/uat-1025`
and `uat-1025b` worktree copies are intentionally left untouched.
