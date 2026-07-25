---
id: CPE-324
title: "Sidecar build: distinct product identity so installers don't collide with the plain release"
type: Bug
status: Done
closed: 2026-07-13
priority: High
component: CI
estimate: 45m
created: 2026-07-13
---

## Summary

The sidecar-enabled build carried the **same version + identifier** as the plain release
(`0.11.0`, `com.example.crossplatformexplorer`). Installing the sidecar installer over an
existing plain install **no-ops** (NSIS sees the same product+version and skips), so the
AI Console never appears — the user keeps running the plain build. Verified: after
"installing" the sidecar build, the on-disk app was still the plain exe with no `sidecars/`.

## Fix

Give the sidecar/preview build a **distinct product identity** in the bundle overlay
(`tauri.sidecar.conf.json`), so it's a separate installable app that never collides with
the plain release regardless of version:

- `productName`: "Cross-Platform Explorer (Sidecar)"
- `identifier`: "com.example.crossplatformexplorer.sidecar"
- `bundle.createUpdaterArtifacts: false` — the preview channel isn't in the auto-update
  stream (release-sidecar.yml already sets `includeUpdaterJson: false`), and this also
  drops the "public key found but no private key" build error.

This is the opt-in preview-channel pattern (cf. VS Code Insiders): the sidecar build
installs to its own location, has its own data dir, and coexists with the plain build.
The overlay is feature-build-only, so the plain release keeps its identity untouched (the
delete-test).

## Acceptance

- Sidecar installer installs as "Cross-Platform Explorer (Sidecar)" to its own location,
  regardless of whether the plain build is present.
- Plain release build is unchanged (same name/identifier/version).
- README + release-sidecar.yml note the distinct identity.

## Work Log
2026-07-13 — Added `productName` "Cross-Platform Explorer (Sidecar)", `identifier`
`com.example.crossplatformexplorer.sidecar`, and `bundle.createUpdaterArtifacts: false` to
`tauri.sidecar.conf.json`. Rebuilt: produces `Cross-Platform Explorer (Sidecar)_0.11.0_*`
installers (MSI + NSIS), the distinct identifier is baked into the exe (overlay reaches the
compiled binary), and the updater-signing build error is gone. README + release-sidecar.yml
noted. Plain release build untouched. Done.
