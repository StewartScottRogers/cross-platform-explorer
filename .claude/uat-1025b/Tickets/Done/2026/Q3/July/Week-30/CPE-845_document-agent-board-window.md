---
id: CPE-845
title: Document the standalone Agent Board window
type: docs
component: Frontend
priority: low
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
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
- [x] `src/docs/06-agent-board.md` covers the standalone window (pop-out, singleton, both-views).
- [x] A design note documents the standalone-window pattern (mirror + trust difference vs the AI Console).
- [x] `sectionDocs.ts` mapping stays valid (the guard test passes); `npm run check` clean.

## Resolution
- `src/docs/06-agent-board.md` — new "Its own window" section: the two launchers (the **⧉** title-bar
  button and the palette's "Open Agent Board in a window"), the app-wide singleton (relaunch focuses),
  that the embedded view still works, and size/position persistence.
- `docs/design/STANDALONE-WINDOWS.md` (new) — the design note: the one-bundle/many-surfaces pattern
  (`bootMode` marker → `main.ts` mounts the matching root, with the `?float`/`?board`/explorer table), the
  singleton `WebviewWindow` + `getByLabel` + window-state persistence, and — the key point — the
  **trusted-vs-isolated** distinction: the Agent Board window renders our own code and is listed in
  `capabilities/default.json` so it can `invoke`, whereas the AI Console window hosts untrusted sidecar
  content and is deliberately in **no** capability (no Tauri API).

No new `Section` was added (the `agent-board` doc slug is unchanged), so the `sectionDocs` registry +
guard are untouched. `npm run check` → 0/0; `sectionDocs.test.ts` → 2 passed.

## Notes
Prereq: **CPE-844**. This closes the last child of epic CPE-841.

## Work Log
- 2026-07-21 — Picked up. Documented the standalone window in the in-app Agent Board page and added a
  `docs/design/STANDALONE-WINDOWS.md` design note (pattern + trusted/isolated capability distinction).
  check clean; docs guard green. Closing — epic CPE-841's children are now all Done.
