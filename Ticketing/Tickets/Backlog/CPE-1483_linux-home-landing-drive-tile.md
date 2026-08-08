---
id: CPE-1483
title: "Linux: Home landing doesn't render the / root as a drive tile (.qa-card drive-root) even though list_drives returns it"
type: Bug
status: Open
priority: Low
component: Frontend/GUI
tags: [ready]
epic: CPE-810
parent: CPE-1481
created: 2026-08-08
---
## Context
Filed from CPE-1481 (gui-smoke Linux-green work), round 5. The gui-smoke `drive-menu.smoke.ts` spec's
two **Home-landing drive-TILE** tests are currently **gated off on Linux** (`SKIP_HOME_DRIVE_TILE =
process.platform === "linux"`) because the Home landing never renders a `.qa-card` with a drive-root
path on the headless Linux/WebKitGTK-under-Xvfb CI runner — even after a full 30s wait (CPE-1481 rounds
3-4). The sidebar drive-ROW right-click tests still run and pass, so drive-context-menu behaviour is
still covered; only the Home-*tile* surface is unverified on Linux.

## The puzzle
On read, the categorization looks correct and the tile *should* render:
- `list_drives_impl` (src-tauri/src/lib.rs) unconditionally returns `[{name:"File System", path:"/",
  kind:"drive"}]` on non-Windows (no I/O; pinned by the `list_drives_returns_at_least_one_root` unit
  test).
- App sets `drives` from `commands.listDrives()` (App.svelte ~5419-5426) and passes `{drives}` to the
  pane/HomeView.
- HomeView builds `cards = [...places, ...drives, ...pinned]` and renders each as a `.qa-card` with a
  `.qa-sub` = `{place.path}`; `quickOpen` defaults `true`, so the `.qa-grid` is expanded.
- So a `.qa-card` whose `.qa-sub` is `/` should exist — which `pointOfDriveTile()` matches via `p === "/"`.

Yet on the Linux CI runner it never appears (`pointOfDriveTile()` returns null after 30s), while the rest
of the Home landing renders fine (the same spec's sidebar drive-row + other Home specs pass). So the gap
is environment-specific and non-obvious — candidates to investigate:
- Does the `drives` prop actually reach the Home-landing `HomeView` instance on Linux (vs a pane-wiring
  gap), or is it empty there?
- Is the `Place` for `/` being dropped/deduped/filtered somewhere between `listDrives()` and `cards`
  (e.g. a keyed-`{#each}` collision on `place.path` if a `place` also carries `/`)?
- A WebKitGTK-specific render/reactivity quirk for that one tile?
- Or is the single-`/`-root simply *intended* not to surface as a Home Quick-access drive tile on
  POSIX (only in the sidebar), making the gui-smoke expectation wrong for Linux rather than the app?

## Acceptance
- Root-caused: categorization/render gap vs intended-POSIX-behaviour, with evidence (a Linux DOM dump of
  the Home landing's `.qa-grid`).
- If it's a real gap: fix so the `/` root renders as a Home drive tile on Linux (or adjust the expectation
  if intended).
- **Un-gate** the two Home-landing drive-tile tests in `gui-smoke/specs/drive-menu.smoke.ts` (remove/relax
  `SKIP_HOME_DRIVE_TILE`) and confirm the ubuntu gui-smoke leg stays green.

## Notes
Low priority: sidebar drive-row coverage already exercises the drive context menu on Linux; this is about
restoring the Home-*tile* assertion + understanding the render gap. See the `SKIP_HOME_DRIVE_TILE` comment
in `gui-smoke/specs/drive-menu.smoke.ts` and CPE-1481's Work Log (rounds 3-5).
