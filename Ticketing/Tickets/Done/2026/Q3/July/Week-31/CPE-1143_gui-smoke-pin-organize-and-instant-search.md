---
id: CPE-1143
title: "QA: pin the auto-organize dialog + Ctrl+K instant-search overlay renders in gui-smoke"
type: chore
component: Testing
priority: medium
status: Done
tags: ready
created: 2026-07-30
epic: CPE-688
---

## Summary
QA-Architect pass. Two new user-facing GUI surfaces shipped today with only jsdom/component tests — they are
NOT pinned by the headless `gui-smoke` harness that drives the real `tauri build` binary, so a real-build
regression (a broken mount, a bad selector, a build-config issue) would go unnoticed:
- **Ctrl+K instant-search overlay** (CPE-1139) — flagged as a QA follow-up when it shipped.
- **Auto-organize dialog** (CPE-1142) — Tools → "Organize this folder…".

Add a `gui-smoke` spec for each, mirroring the established pattern (`gui-smoke/specs/cost-history.smoke.ts` /
`replay.smoke.ts`, CPE-1130/1135): drive the real built binary, reach the surface, assert its key elements
render non-empty. Non-blocking CI signal (`continue-on-error`, per CPE-1048), like the other smoke specs.

## What exists (verified)
- `gui-smoke/wdio.conf.ts` opens the app on a seeded tmpDir (`--open`) with fixtures + `onComplete` cleanup;
  `--test-mode` exposes hooks; specs are auto-discovered by the `specs` glob.
- Instant search: global **Ctrl+K** opens `InstantSearch.svelte`; with no index it shows a "Build index"
  affordance (off-means-off). `index_build`/`index_search`/`index_status` commands exist.
- Auto-organize: opened via the command palette (`tool.organize`) or Tools menu (`organize-folder`);
  `OrganizeDialog.svelte` shows a rule picker + a preview grouped by target subdir; `organize_plan` is
  read-only.

## Design
- `gui-smoke/specs/instant-search.smoke.ts`: press **Ctrl+K**, assert the overlay opens and (with no index
  built) its **"Build index"** affordance renders (the reliable off-means-off state — no index crawl needed).
  Optionally, if cheap + non-flaky, trigger a build over the tmpDir then type and assert a result row renders;
  keep the affordance assertion as the guaranteed one.
- `gui-smoke/specs/organize.smoke.ts`: open the Organize dialog (via the command palette or the test-mode
  path used by the other specs), pick a rule, and assert the **grouped preview** renders proposals for the
  tmpDir's seeded files (the harness's fixtures include mixed types). Do NOT click Apply (keep the spec
  non-destructive — or, if applying, do it only on the throwaway tmpDir and rely on the checkpoint).
- Reach the surfaces the same way the existing specs do (breadcrumb wait, `--test-mode` hooks, WebdriverIO
  keys/clicks). Wire both specs as non-blocking (`continue-on-error`) alongside the existing smoke specs.
- If a surface needs a minimal `--test-mode`-gated seam to be reachable headlessly, add the smallest one
  (mirror `__CPE_TEST_INGEST_SESSION__`/`__CPE_OPEN_DIR__`); prefer using existing openers (palette/keybind)
  with no app change.

## Acceptance Criteria
- [x] `instant-search.smoke.ts` drives the real build, opens the Ctrl+K overlay, and asserts its render
      (at minimum the "Build index" off-means-off affordance) non-empty.
- [x] `organize.smoke.ts` opens the auto-organize dialog on the seeded tmpDir, selects a rule, and asserts the
      grouped proposal preview renders; non-destructive (no unintended file moves outside the throwaway tmpDir).
- [x] Both specs wired into the `GUI smoke` job as non-blocking (`continue-on-error`, CPE-1048); existing smoke
      specs (open-dir/cost-history/replay) still pass.
- [x] Any new test-mode seam (if needed) is `--test-mode`-gated with zero production effect; `npm run check`
      passes.

## Notes
- Burns down manual-test debt for the two newest GUI surfaces (the user's standing "never test by hand" goal).
- This machine has tauri-driver + msedgedriver, so the worker CAN run the real gui-smoke locally to prove the
  specs green (~30-40m) — worth it (past pins caught real bugs that way). WebView2 runner args are in the
  research-library entries if a local run misbehaves.

## Work Log
- `gui-smoke/specs/instant-search.smoke.ts`: waits for the initial `--open=<tmpDir>` breadcrumb, sends the
  real global **Ctrl+K** shortcut (App.svelte's `handleKeydown` — no test-mode seam needed, the existing
  opener is reachable via WebdriverIO's `browser.keys()`), asserts `.is-panel` renders and is visible, then
  asserts the guaranteed off-means-off state: `.is-offer` renders, its HTML includes "Build index", and
  `.is-offer button.btn.primary` exists and is enabled. Reliable regardless of any real index a dev machine
  might have on disk elsewhere, because `index_status` only reports **resident** (in-memory) volumes
  (`crates/server/src/index_service.rs`) and this harness launches a fresh app process per spec file, so
  `hasIndex` is always `false` on first paint.
- `gui-smoke/specs/organize.smoke.ts`: waits for the breadcrumb, opens the Command Palette with
  **Ctrl+Shift+P** (the same `tool.organize` command the Tools menu's "Organize this folder…" entry runs),
  types "organize", locates and clicks the `.cp-row` labelled "Organize this folder…", waits for
  `[data-testid="rule-picker"]`, explicitly selects the **By extension** rule
  (`[data-testid="rule-by_extension"]`), then asserts the grouped preview: `[data-testid^="group-"]` rows
  exist, and specifically `[data-testid="group-PNG"]` / `[data-testid="group-ZIP"]` contain the seeded
  `CPE-1143-photo.png` / `CPE-1143-archive.zip` filenames, plus `[data-testid="summary"]` renders. Never
  clicks Apply — dismissed via `[data-testid="cancel-btn"]`. No files were moved on disk.
- `gui-smoke/wdio.conf.ts`: added `seedOrganizeFixture()` (called from `onPrepare`, no restore needed since
  it writes into the disposable tmpDir `onComplete` already `rm -rf`s) — three extra mixed-kind files
  (`CPE-1143-photo.png`, `CPE-1143-archive.zip`, `CPE-1143-song.mp3`) alongside the pre-existing marker/code
  fixture, so the organize preview always has real cross-category variety to group.
- No `--test-mode` seam was needed for either surface — both are reachable through their existing real
  openers (the global Ctrl+K binding and the Command Palette), which WebdriverIO can drive directly.
- No workflow edit needed: `gui-smoke.yml`'s `specs` glob (`./specs/**/*.smoke.ts`) auto-discovers new spec
  files, and the job is already `continue-on-error: true` at the job level (CPE-1048).
- `npm run check`: **0 errors, 0 warnings.**
- `gui-smoke`'s own `npm run typecheck` (tsc --noEmit): clean.
- Ran the **real** `gui-smoke` suite locally end-to-end (this machine has tauri-driver + msedgedriver):
  `npm run build` → `npm run tauri build -- --no-bundle` → `npm test` (with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` set as the CI job does). Result: **`Spec Files: 5 passed, 5 total
  (100% completed)`** — `open-dir`, `cost-history`, `replay`, `instant-search`, `organize` all green
  (`instant-search`: 1 passing; `organize`: 1 passing), confirming the two new specs exercise the real
  built binary and pass alongside every pre-existing smoke spec.
