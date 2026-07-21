---
id: CPE-845
title: Document the standalone Agent Board window
type: docs
component: Frontend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-841
estimate: 30m
---

## Summary
Document the standalone Agent Board window (epic CPE-841). Update the in-app Agent Board doc
(`src/docs/06-agent-board.md`) to cover opening it in its own window — the pop-out, the app-wide
singleton (relaunch focuses it), and that the embedded view still works. Add a short design note (under
`docs/design/`) on the standalone-window pattern: how it mirrors the AI Console window (a `WebviewWindow`
with a fixed label + `getByLabel` singleton) and where it deliberately **differs** — the board window is
trusted and has Tauri API via a capability entry, versus the AI Console's isolated, API-less sidecar
window. Keep the `sectionDocs.ts` registry consistent (CPE-579 guard).

## Acceptance Criteria
- [ ] `src/docs/06-agent-board.md` covers the standalone window (pop-out, singleton, both-views).
- [ ] A design note documents the standalone-window pattern (mirror + trust difference vs the AI Console).
- [ ] `sectionDocs.ts` mapping stays valid (the guard test passes); `npm run check` clean.

## Notes
Prereq: **CPE-844**.

## Work Log
