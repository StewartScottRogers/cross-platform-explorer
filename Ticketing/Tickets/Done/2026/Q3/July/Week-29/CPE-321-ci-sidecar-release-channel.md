---
id: CPE-321
title: "CI: opt-in sidecar-enabled release channel (Windows)"
type: Task
status: Done
closed: 2026-07-13
priority: Medium
component: CI
estimate: 1h
created: 2026-07-13
---

## Summary

The main `release.yml` builds the plain, sidecar-free app (the shipped default). We need a
way to produce **sidecar-enabled installers** (the AI Console bundled in) automatically,
without committing the public release to shipping an unfinished console (placeholder UI,
security sign-off CPE-304 still open). Add a separate, **opt-in** release channel.

## Design

New workflow `.github/workflows/release-sidecar.yml`:

- **Trigger:** `workflow_dispatch` with a `tag` input — fully manual/opt-in, no tag
  pollution of the main stream.
- **Platform:** **Windows only** for now. The feature compiles on all three OSes (CI
  `backend` job proves it) and `ai-console` builds cross-OS, but the host's OS-keychain
  secrets backend (`keyring`) is Windows-only today; on macOS/Linux secrets fall back to
  in-memory, which breaks the "secrets only in the OS keychain" invariant (ADR 0001 /
  CPE-268). Shipping those platforms waits on their keychain backends — see CPE-322.
- **Steps:** build the sidecar release binary (`sidecar/ai-console` → `ai-console.exe`),
  then `tauri-action` with `args: --features sidecar-platform --config
  src-tauri/tauri.sidecar.conf.json`. Publishes a **draft** release; reuses the updater
  signing secret so bundles sign identically. `includeUpdaterJson: false` — the sidecar
  channel is an installable preview, kept out of the auto-update stream for now.

## Acceptance

- Manually dispatchable; produces Windows MSI + NSIS with `sidecars/ai-console.exe` +
  `sidecar.json` + agent manifests bundled, as a draft GitHub Release.
- Main `release.yml` untouched (plain app still ships by default).
- macOS/Linux explicitly deferred with a follow-up ticket (CPE-322).

## Work Log
2026-07-13 — Added `.github/workflows/release-sidecar.yml`: `workflow_dispatch` (tag input),
Windows-only, builds `sidecar/ai-console` then `tauri-action` with
`--features sidecar-platform --config src-tauri/tauri.sidecar.conf.json`, draft + prerelease,
reuses the updater signing secret, `includeUpdaterJson: false`. Main `release.yml` untouched.
YAML validated. Filed CPE-322 for macOS/Linux keychain backends + cross-OS matrix. Done.
