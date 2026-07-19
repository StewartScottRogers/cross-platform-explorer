---
id: CPE-676
title: Extract an <ExplorerPane> component from App.svelte
type: refactor
component: Frontend
priority: low
status: In Progress
tags: ready
created: 2026-07-18
epic: CPE-617
estimate: 4h+
---

## Summary
Foundation for dual-pane (CPE-617). Extract one explorer view's state + operations from the ~3,112-line
App.svelte monolith into a reusable `<ExplorerPane>` component (its own path/history, entries, selection,
sort, view, archive/smart-folder, search, DnD, file ops), so single-pane renders one instance and
dual-pane can render two. App becomes a thin shell (window chrome, layout, shared services).

## Acceptance Criteria
- [ ] `<ExplorerPane>` owns per-pane state + operations; App holds only shared/window-level concerns.
- [ ] Single-pane behaviour is byte-for-byte unchanged (all existing App tests pass, GUI-verified).
- [ ] No measurable single-pane startup/memory regression.
- [ ] `npm run check` + full suite green; clippy unaffected.

## Deferred
deferred-on: this is a large, high-regression-surface refactor of the app core — it must be done
**attended with live GUI verification**, not shipped blind in an unattended nightshift. revisit-when: an
attended session dedicated to the extraction. It's the gating prereq for CPE-677/678/679.

## Work Log
2026-07-18 — Picked up (attended, user chose big-bang). On a branch; merge only if check+full suite+build green so main stays safe.
2026-07-18 — Slice 1 landed: extracted the file-listing region (Home/agent-strip/tag-bar/FileList) into ExplorerPane.svelte (presentational; App owns state via props/binds/events). check clean; 669 tests pass; build clean. Next slices push per-pane state ownership down into ExplorerPane. Still In Progress (multi-slice).
2026-07-18 — Slice 2 landed: moved the view/sort toolbar + context bar into ExplorerPane, so it now owns the whole middle column. check clean; 669 tests; build clean. Still presentational (App owns state). Next: push per-pane state ownership down.
2026-07-18 (nightshift) — Paused at slice 2. Confirmed hands-on that the remaining work is the all-at-once
state-ownership move (derivation pipeline + view/selection state), whose outputs feed the top NavToolbar,
bottom StatusBar, the sidebar, and ~40 App operations/keyboard handlers. It needs live GUI verification the
test suite doesn't fully cover (selection timing, keyboard routing, archive/smart-folder transitions), so it
is NOT safe to land autonomously overnight. Slices 1-2 (middle-column extraction) are landed + green on main.
Resume attended: do the state move with a running app to verify. NavToolbar/StatusBar stay app-level until
CPE-677 (they become per-pane when the split is added).
