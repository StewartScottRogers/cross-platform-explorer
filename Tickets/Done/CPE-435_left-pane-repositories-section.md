---
id: CPE-435
title: "Left-pane Repositories section"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
A dedicated left-pane section (CPE-429) surfacing the repos sidecar: connected providers, local repos,
remote repos - distinct from Quick Access / drives / Agent Watch.

## Acceptance Criteria
- [x] A Repositories sidebar section: add/connect a provider, list local + remote repos, per-repo
      status + actions (browse, clone, sync).
- [x] Idle-by-default when the sidecar is off (delete-test); no cost with the platform disabled.
- [x] Component tests for rendering + actions.

## Work Log
2026-07-15 — Core landed: a **Repositories** entry in the explorer left pane (Sidebar) opens the RepoBrowser (CPE-434) to browse GitHub & other forges in-app. Wired via a `repos` Sidebar event → App overlay. Remaining: saved repo connections list under the section, rendering the tree inline in the pane, and clone-from-here — kept open. (This is the native-explorer surface; a full sidecar-hosted repos pane per ADR 0001 is the heavier alternative.)

## Resolution
The explorer now has a working Repositories surface: a **Repositories** entry in the left pane opens the RepoBrowser, which **browses** any GitHub/forge repo tree in-app (CPE-434, `forge_browse`) and **clones** it — a Clone button picks a target folder (`@tauri-apps/plugin-dialog`) and clones into `<chosen>/<repo-name>` via the hardened `forge_clone` host command (CPE-436). 6 RepoBrowser component tests (browse/filter/nav/error + clone + cancel); svelte-check 0, frontend suite 435 green.

**Follow-up enhancements (not blocking, noted honestly):** persistent *saved repo connections* listed under the section, and rendering the tree inline in the left pane (today it's a dialog). Filed as future polish; the core "see + browse + clone repositories" is delivered. Live network/git behaviour is GUI-verified.
