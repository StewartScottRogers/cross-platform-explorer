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

2026-07-20 (attended, scoping only — salvaged from the now-stale `CPE-676-pane-state` branch, which is far
behind main and must NOT be merged) — Measured the redirect surface: **~340 references to per-pane state**
in App.svelte (selectedEntries ×74, currentPath ×66, selection ×51, visible ×41, entries ×34, loadPath ×27,
activeTab ×12), spread across ops, keyboard handlers, palette commands, the derivation pipeline, and render.
No clean sub-slice: `visible` (sort/hidden/search/type/tag derivation) feeds `selectedEntries`, read by ~40
ops, whose mutations drive `entries`/`selection`/`history`. **Target architecture (least-churn):** the pane
owns history/path, entries, loading, the full derivation, selection + selectedEntries, and view/sort/search/
showHidden, exposed via `bind:` + a `bind:this` method surface (navigate/back/forward/refresh/openEntry/
selection setters). App keeps its op *logic* but sources selectedEntries/currentPath from the active pane and
calls that pane's refresh(); NavToolbar/StatusBar/palette/keyboard route to the active pane.

2026-07-21 (attended, resumed) — Approach: instead of one atomic big-bang, move ownership **one domino at a
time**, keeping `npm run check` + the full suite green after each and GUI-verifying the risky ones. Branch
`cpe-676-state-move` off current main (the old branch is abandoned — it predates crates/server, the
agent-board sidecar, the data browser, etc. and would revert ~19k lines).
- **Domino 1 (done, green):** moved `selectedEntries` (the ×74 var) into ExplorerPane. The pane already had
  both inputs — `selection` (bound) + `visible` (prop) — so it needs no new props: it computes
  `selectedEntries` and binds it back to App, leaving App's 74 read-sites untouched. Removed App's `$:`
  derivation; added `let selectedEntries` + `bind:selectedEntries`. No new timing risk vs today (the async
  `$:` boundary already existed; read-after-write in one sync handler was already unsafe). check clean; 902
  tests pass. GUI verify pending (selection → status-bar count/size, preview pop-out enable, multi-select).
- **Domino 2 (done, green):** moved the whole display-derivation pipeline (`shown → filtered → typeFiltered
  → tagFiltered → sizeOf → visible`, plus `searching`) into ExplorerPane. App resolves the base list +
  archive/smart mode and passes them down as props (`baseEntries`, `rawList`, `search`, `fileFilter`,
  `foldersFirst`); the pane computes `visible` + `shown` and binds them back for App's status bar (`X of Y`)
  and ~41 read-sites. Archive stays in App (local `ArchiveView`/`archiveChildren`) — App passes the resolved
  archive children + `rawList=true`, and the pane skips filters in that mode. `folderName`/`crumbs`/
  `folderContexts` stay app-level. Pruned now-unused App imports (`sortEntries`, `makeMatcher`,
  `matchesFileFilter`, `filterEntriesByTag`). check clean (0/0); 902 tests pass incl. wildcard-search +
  folder-nav. GUI verify pending: hidden toggle, search, type filter, tag filter, sort dirs/folders-first,
  folder-size column, archive browsing, status-bar totals.
- 2026-07-21 — **Dominoes 1+2 landed on main via PR #141** (squash `fccac06`). The display-derivation is
  now owned by ExplorerPane. **Remaining = the navigation big-bang:** `loadPath` (streaming + generation
  tokens + nav cache + smart-folder/archive transitions + selection/search/tag reset + folder-size
  invalidation + post-load git/disk/agent-watch hooks) plus `entries`/`currentPath`(×67)/`history`(×33)/tabs
  and the ~40 callers. This is ONE coupled unit; no further safe bind-back sub-slice exists (the pane already
  gets `baseEntries`, so binding `entries` in adds nothing). It must be done with the running app driven
  through nav/keyboard/archive/smart-folder/tab flows — the 902 tests don't cover those. Do NOT merge it
  blind. Next: a focused attended pass (this, or fold into CPE-677 when the split lands).
- 2026-07-21 — **Domino 3a landed (#143), GUI-verified.** The pane now owns the raw `entries` listing too
  (bound back; App's `loadPath` still writes it) and resolves the base list from `smartOverride`/
  `archiveOverride` props. So the pane now owns: entries → the whole sort/hidden/search/type/tag pipeline →
  `visible` → `selectedEntries` (dominoes 1/2/3a). **Remaining = the `loadPath` navigation engine** (streaming
  fetch + generation tokens + nav cache + smart-folder/archive/history/tabs + the ~40 nav/op callers). This
  is the irreducible attended big-bang; the 902 tests don't cover its risky flows (keyboard routing, archive/
  smart-folder/tab transitions, streaming/cache). Best done as a dedicated focused block with live driving —
  not squeezed into the tail of a long multi-feature session.
