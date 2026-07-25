---
id: CPE-509
title: "Agent Grid — persist layout + active view across relaunch/reattach"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
closed: 2026-07-16
---

## Summary
Remember the workspace layout ([[CPE-501]]): which view was active (Tabs vs Grid) and the grid
arrangement, restored on relaunch and on session **reattach** ([[CPE-309]]). So reopening the AI
Console brings back the same tiled view of the same agents.

## Acceptance Criteria
- [x] The active view (tabs/grid) + grid arrangement persist per session-set and restore on relaunch.
- [x] On daemon reattach (CPE-309), restored sessions repopulate their tiles in the remembered layout
      (best-effort: a session that didn't survive is simply absent, no crash).
- [x] Persistence is scoped so unrelated workspaces don't clobber each other's layout.
- [x] Clearing / closing all sessions resets to the default view.
- [x] Tests for the (de)serialization of the layout state.

## Resolution
Persist the Tabs⇄Grid view per workspace and restore it on relaunch/reattach, in `launcher.html`.

- With **auto-grid**, the only durable "arrangement" is the **view mode + focused agent** — the tile
  order is whatever order the CPE-309 daemon restores sessions in. So layout state is
  `{ viewMode, activeId }`, kept in `localStorage` under `cpe.aiconsole.layout` **keyed by the project
  folder** (`catalog.cwd`), so unrelated workspaces don't clobber each other.
- **Pure `serializeLayout` / `parseLayout`** (tolerant — garbage ⇒ default tabs) are the tested core;
  `saveLayout` / `loadLayout` wrap them with the per-workspace map.
- **`restoreLayout()`** runs right after `reattachSessions()` populates the restored sessions: it
  re-focuses the saved agent (if it survived) and re-applies the saved view. A session that didn't
  survive is simply absent — no crash.
- Persistence points are deliberately **only** the user's explicit `setView` (toggle) and the
  close-all reset, so the reattach loop can't clobber the stored value before `restoreLayout` reads it.
- **`closeAllSessions`** resets `viewMode` to the default tabs view and persists that.

Tests: serialize/parse round-trip + garbage tolerance; persist-then-restore across a simulated relaunch;
workspace scoping (a different `cwd` defaults to tabs); close-all resets to tabs. 46 launcher + 521
frontend tests pass; `npm run check` clean.

## Work Log
2026-07-16 — Picked up (dayshift; prereq CPE-506). Estimate: 2-3h.
2026-07-16 — Added per-workspace layout persistence (localStorage keyed by cwd), pure serialize/parse, restoreLayout after reattach, close-all reset. Saved only on user toggle + reset to avoid reattach clobber. 4 new jsdom tests.
2026-07-16 — Verified: 46 launcher + 521 frontend tests pass; `npm run check` clean. **Assumption logged:** with auto-grid the persisted "arrangement" is view mode + focused agent (no manual splits to store); durability across a full app restart depends on a stable WebView origin (within-session reattach + reopen is covered). All ACs met.

## Notes
**needs-prereq:** [[CPE-506]] (a layout to persist); ties into [[CPE-309]] reattach. Per-session-set
persistence per the CPE-501 activation decision.
