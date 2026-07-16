---
id: CPE-495
title: "Two-way mirror UI — pull / push / sync with per-repo policy + dry-run preview"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-488
---

## Summary
The mirror **engine** (CPE-438 planner) and the `forge_sync` / `forge_repo_status` host commands +
ahead/behind status bar (CPE-462) already exist. Build the UI that drives **two-way** sync end-to-end:
Pull (merge/rebase), Push, and a **per-repo sync policy**, with a **dry-run/preview** of the plan
before it runs and clear divergence/dirty-tree warnings. Safe-by-default — never force-push.

## Acceptance Criteria
- [ ] The Repositories surface offers Pull / Push / Sync actions for a local repo, driven by the
      existing `forge_sync` + `plan_sync` engine.
- [ ] A per-repo sync policy (merge / rebase / manual on-diverge; `allow_force` stays off) is settable
      and persisted.
- [ ] A dry-run **preview** shows the planned `SyncActions` (ahead/behind, conflict-risk, dirty-tree)
      before executing; the user confirms.
- [ ] Never force-pushes; a divergence surfaces for the user rather than silently reconciling.
- [ ] Frontend + backend tests; `npm run check` clean; Rust green.
