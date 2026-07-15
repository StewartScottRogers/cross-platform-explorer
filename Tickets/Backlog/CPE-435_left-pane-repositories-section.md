---
id: CPE-435
title: "Left-pane Repositories section"
type: Feature
status: Open
priority: High
component: Frontend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-429
---

## Summary
A dedicated left-pane section (CPE-429) surfacing the repos sidecar: connected providers, local repos,
remote repos - distinct from Quick Access / drives / Agent Watch.

## Acceptance Criteria
- [ ] A Repositories sidebar section: add/connect a provider, list local + remote repos, per-repo
      status + actions (browse, clone, sync).
- [ ] Idle-by-default when the sidecar is off (delete-test); no cost with the platform disabled.
- [ ] Component tests for rendering + actions.

## Work Log
2026-07-15 — Core landed: a **Repositories** entry in the explorer left pane (Sidebar) opens the RepoBrowser (CPE-434) to browse GitHub & other forges in-app. Wired via a `repos` Sidebar event → App overlay. Remaining: saved repo connections list under the section, rendering the tree inline in the pane, and clone-from-here — kept open. (This is the native-explorer surface; a full sidecar-hosted repos pane per ADR 0001 is the heavier alternative.)
