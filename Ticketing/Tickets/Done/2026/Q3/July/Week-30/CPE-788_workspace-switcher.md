---
id: CPE-788
title: Workspace switcher UI (save / switch / rename / delete)
type: feature
status: Done
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-708
estimate: 2-3h
---

## Summary
The workspace switcher for epic CPE-708: save the current tabs/views as a named workspace (CPE-787), and
switch / rename / delete from a menu; selecting one applies its tabs. Persisted in settings.

## Acceptance Criteria
- [x] Save current window state as a named workspace; switch applies its tabs (path + view/sort/filter).
- [x] Rename/delete work; persists across sessions; menus follow MENUS.md + CPE-748 icons.
- [x] check + suite green; GUI-verified.

## Resolution
Built `src/lib/components/WorkspacesDialog.svelte` over the tested `workspaces` store: save the current
window as a named workspace, and switch / rename / delete saved ones. App captures the current tabs
(`captureCurrentTabs` → each tab's path + the global view/sort/filter) and applies a chosen workspace
(`switchWorkspace` → rebuilds the tabs via `createHistory`, adopts the first tab's view/sort/filter, and
navigates). Workspaces persist via settings (`cpe.workspaces`, tolerant load through `parseWorkspaces`).
Opened from the command palette ("Workspaces…", Go group; all 12 locales).

**GUI-verified in the running dev app (CDP):** set up 2 tabs (Documents + a Home tab) → saved as "WS1" →
listed as `WS1 · 2 tabs` → opened a 3rd tab (state now 3) → **switched to WS1 → tabs collapsed back to the
saved 2 (Documents + Home)** → reopening showed WS1 persisted → **renamed** to "Renamed" → **deleted** →
list empty. Test workspace cleaned up. `npm run check` clean; workspaces/settings/i18n suites green.

Note: view/sort/filter are global in this app's tab model, so a workspace records them once (adopted from
the first tab on switch); genuinely per-tab view state is a future enhancement.
