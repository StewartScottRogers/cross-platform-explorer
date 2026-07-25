---
id: CPE-859
title: Build the sidecar Windows release NSIS-only (drop flaky WiX MSI target)
type: bug
component: Release
priority: high
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
The `release-sidecar.yml` Windows job fails at the WiX `light.exe` (MSI) bundling step with a bare
`failed to run light.exe` and **no diagnostic** — `light.exe` crashes (exits non-zero ~10s in) with no
`LGHT####` code, so it's not a content-validation rejection. It failed on both the initial run and a
clean rerun for v0.57.4-sidecar, blocking the deploy. The Unix builds (deb/AppImage/dmg) succeed.

`bundle.targets` is `"all"` in the base `tauri.conf.json`, so Windows builds **both** NSIS
(`*_x64-setup.exe`) and MSI. But our install script (`/run`) downloads and installs the **NSIS**
`_x64-setup.exe`, and the Tauri updater consumes the NSIS artifact — the MSI is never used. The MSI is
pure flaky liability on the release path.

## Fix
Override `bundle.targets` to `["nsis"]` in `tauri.sidecar.windows.conf.json` (the Windows sidecar
overlay only — the base config's `"all"` is untouched for every other build path). NSIS still emits the
updater artifacts (`createUpdaterArtifacts: true`), so auto-update is unaffected.

## Acceptance Criteria
- [x] `tauri.sidecar.windows.conf.json` sets `bundle.targets: ["nsis"]`.
- [x] A re-dispatched `v0.57.4-sidecar` Windows build produces the NSIS `_x64-setup.exe` + `.sig`.
- [x] Base `tauri.conf.json` and the non-sidecar build path are unchanged.

## Work Log
- 2026-07-21 — light.exe MSI crash blocked v0.57.4 twice; scoping Windows sidecar bundle to NSIS.
- 2026-07-21 — Fix merged to main via PR #135 (commit 8ead548), CI green. Re-dispatched sidecar release
  succeeded; the published `v0.57.4-sidecar` release now carries `..._0.57.4_x64-setup.exe` + `.sig`
  (NSIS) and **no MSI**. All acceptance criteria met. Closing.
