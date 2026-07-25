---
id: CPE-789
title: Launch-time workspace auto-restore (graceful missing paths)
type: feature
status: Done
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-708
estimate: 2h
---

## Summary
Auto-restore for epic CPE-708: persist the last session and reopen it on launch, using `pruneMissing`
(CPE-787) to drop moved/absent paths so restore never fails. No change to default single-tab startup when
unused.

## Acceptance Criteria
- [x] Last session reopens on launch; missing/moved paths are skipped, valid tabs restored.
- [x] Default single-tab startup is unchanged when the feature is off/empty.
- [x] check + suite green; GUI-verified.

## Notes
Prereq: CPE-787. Attended GUI. Opt-in / off by default to preserve predictable startup (PURPOSE.md).

## Work Log
- 2026-07-20 — Picked up. Estimate 2h. Prereq check: `pruneMissing` (CPE-787) present in `workspaces.ts`;
  the workspace capture/restore path (`captureCurrentTabs` / `switchWorkspace`, CPE-788) already exists in
  App, so auto-restore is thin — persist the open session + reopen it on launch through the same restore path.
- 2026-07-20 — Added settings: `cpe.autoRestore` (bool, default **false**) + `cpe.lastSession`
  (`WorkspaceTab[]`, tolerant-parsed by wrapping in a throwaway workspace through `parseWorkspaces`). App:
  `restoreLastSession()` (async-existence-checks each saved path via `entry_info`, `pruneMissing` drops the
  gone ones, then `switchWorkspace(pruned)` reopens the survivors + adopts the first tab's view/sort/filter);
  onMount restores before falling back to the default HOME tab; a `sessionReady`-gated reactive persists the
  session only while the feature is on (so **off = byte-for-byte the old single-tab startup**). Toggle added
  to the Workspaces dialog ("Reopen last session on launch"); turning it on captures immediately.
- 2026-07-20 — 3 settings tests (defaults off/empty; flag + tabs round-trip; corrupt tabs dropped). `npm run
  check` clean; frontend suite **889 green**.

## Resolution
Opt-in launch-time session restore. Persists the open tabs (`cpe.lastSession`) whenever they change *while
enabled*, and on launch — if `cpe.autoRestore` is on — reopens them, dropping any whose path no longer exists
(`pruneMissing` fed by async `entry_info` existence checks) so a moved/deleted folder never breaks startup;
the first tab's view/sort/filter is adopted. Reuses the CPE-788 `switchWorkspace` restore path. Off by
default and gated so single-tab startup is unchanged. Files: `src/lib/settings.ts` (keys + accessors),
`src/App.svelte` (`restoreLastSession`/`setAutoRestore` + onMount + reactive capture),
`src/lib/components/WorkspacesDialog.svelte` (toggle), `src/lib/settings.test.ts` (+3 tests).

**GUI-verified in the sidecar dev build (CDP):** with auto-restore **on** and a saved 3-tab session (2 real
folders + 1 nonexistent), reload reopened **exactly the 2 real tabs** (`cross-platform-explorer`, `repos`),
dropped the missing one, and applied the first tab's `list` view (`existing=2 pruned=2`). With auto-restore
**off** (session still saved), reload showed a **single Home tab** — default startup unchanged. All three ACs
met. CPE-789 → Done.
