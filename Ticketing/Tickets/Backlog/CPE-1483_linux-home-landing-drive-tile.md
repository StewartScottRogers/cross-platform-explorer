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

## Work Log — Round 1 (2026-08-09, Sprint Worker)

**Status: PR opened, ubuntu gui-smoke CI leg is the real verification (cannot reproduce WebKitGTK-under-Xvfb
locally on Windows).**

### Root cause — code-level categorization is correct; gap is environment-specific
Read `HomeView.svelte` (`cards = [...places, ...drives, ...pinned]`, keyed `{#each cards as place
(place.path)}`), `ExplorerPane.svelte` (`{drives}` passed straight through to `<HomeView>`), `App.svelte`
(~5571-5586, `drives = d` from `commands.listDrives()`), `Sidebar.svelte` (`{#each [...places, ...drives] as
place, i (place.path)}` — the SAME keyed pattern), and `list_drives_impl`/`special_folders_impl`
(src-tauri/src/lib.rs). Findings:
- No keyed-`{#each}` collision: `special_folders_impl` only returns places whose folder *actually exists*
  under `$HOME` (Desktop/Documents/Downloads/Pictures/Music/Videos) — none of those paths can equal `/`.
- The `drives` prop DOES reach the app correctly: the **sidebar drive-ROW test passes on Linux** using the
  exact same `drives` array and the exact same keyed-each pattern as HomeView — so the data pipeline
  (`listDrives()` → `App.drives` → prop) is proven fine by that passing test.
- **New evidence this round**: added a jsdom regression test (`HomeView.test.ts`, describe block "HomeView
  Quick Access drive tile (CPE-1483)") that renders `<HomeView>` with the EXACT prop shape App/ExplorerPane
  feed it on non-Windows (`drives: [{name:"File System", path:"/", kind:"drive"}]`) and confirms a
  `.qa-card` with `.qa-sub === "/"` IS produced. This proves the categorization/render code path itself is
  correct — not a dedupe/filter/keyed-each bug reproducible outside the actual Linux CI environment.
- **Leading remaining theory**: HomeView's Quick-Access section (`.qa-grid`) has its OWN independently
  persisted collapse state (`quickOpen`, HomeView.svelte:57) — separate from the sidebar's `drivesOpen`
  section state. The sidebar is ALWAYS mounted (present in every view); HomeView only mounts while
  `inHome`. The OLD `goHome()` wait condition (`.qa-grid` OR `.home` existing) could be satisfied by `.home`
  alone even with Quick Access collapsed, silently hiding every `.qa-card` (not just the drive one) without
  the test ever detecting the collapse. This is consistent with "sidebar row renders, Home tile doesn't"
  without requiring any code bug.

### Fix — spec-only, defensive + diagnostic (no `src/` app change; none was warranted by the evidence)
`gui-smoke/specs/drive-menu.smoke.ts`:
1. **Un-gated** both Home-landing drive-tile tests — removed `SKIP_HOME_DRIVE_TILE` and its two `this.skip()`
   call sites entirely.
2. `goHome()` now calls a new `ensureQuickAccessOpen()` that expands Quick Access if its twisty shows
   collapsed (structural, locale-independent selector — the first `.section-head` inside `.home`) — the
   same action a real user would take, and closes the leading theory above regardless of whether it's the
   true Linux CI cause.
3. `waitForDriveTile()` now catches its timeout and rethrows with live `qaGridDiagnostics()` (whether
   `.home`/`.qa-grid` exist, the twisty's open state, and every current `.qa-card`'s `.qa-sub` text) folded
   into the error message — so if the ubuntu leg is still red, the CI log carries real Linux-only DOM
   evidence instead of a bare "expected null to not equal null", enabling a fast, evidence-based follow-up
   round instead of another blind one.

### Verification
- `HomeView.test.ts` (19/19, incl. 2 new CPE-1483 tests) — pass.
- `npm run check` (svelte-check) — 0 errors, 0 warnings.
- Full `npx vitest run` — 233 files / 2643 tests pass, no regressions.
- `gui-smoke && npm run typecheck` — clean.
- `gui-smoke && npm run test:unit` — 21/21 pass.
- **Cannot verify locally**: the actual Linux/WebKitGTK-under-Xvfb render behaviour — no tauri-driver +
  WebKitWebDriver + xvfb environment on this Windows dev box. The ubuntu `gui-smoke-linux` CI leg on the PR
  is the real, authoritative verification of whether `ensureQuickAccessOpen()` actually clears the gap.
