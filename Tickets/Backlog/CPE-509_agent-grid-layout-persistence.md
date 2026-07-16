---
id: CPE-509
title: "Agent Grid — persist layout + active view across relaunch/reattach"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
---

## Summary
Remember the workspace layout ([[CPE-501]]): which view was active (Tabs vs Grid) and the grid
arrangement, restored on relaunch and on session **reattach** ([[CPE-309]]). So reopening the AI
Console brings back the same tiled view of the same agents.

## Acceptance Criteria
- [ ] The active view (tabs/grid) + grid arrangement persist per session-set and restore on relaunch.
- [ ] On daemon reattach (CPE-309), restored sessions repopulate their tiles in the remembered layout
      (best-effort: a session that didn't survive is simply absent, no crash).
- [ ] Persistence is scoped so unrelated workspaces don't clobber each other's layout.
- [ ] Clearing / closing all sessions resets to the default view.
- [ ] Tests for the (de)serialization of the layout state.

## Notes
**needs-prereq:** [[CPE-506]] (a layout to persist); ties into [[CPE-309]] reattach. Per-session-set
persistence per the CPE-501 activation decision.
