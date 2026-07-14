---
id: CPE-382
title: "Ship the sidecar platform in release builds (bundle sidecar + enable feature)"
type: Feature
status: Open
priority: Medium
component: Packaging
tags: [big-design, needs-prereq]
created: 2026-07-14
---

## Summary

Make production installers actually contain the AI Console (today `sidecar-platform` is OFF in
releases — the delete-test, CPE-272 — and no sidecar binary is bundled, so the whole platform,
including catalog auto-update, does nothing in shipped builds).

## What it takes (NOT a flag flip)

- [ ] Build the `ai-console` sidecar binary for every release target (mac universal, windows, linux).
- [ ] Bundle it as a Tauri `externalBin`/resource; `resolve_ai_console_bin` finds the bundled path
      in production (today it only has a dev-tree fallback).
- [ ] Enable `--features sidecar-platform` in `release.yml` (and confirm updater/signing still pass).
- [ ] Bundle-size + startup review; the delete-test (CPE-272) must still hold when the flag is off.

## Blocker — security sign-off (needs-prereq)

The threat model is **NOT production-signed-off** (`docs/security/threat-model.md`). The consent gate
(**CPE-296**) is now **DONE** — that no longer blocks. The remaining gate is **CPE-322** (non-Windows
keychains, so off-Windows secrets persist) plus the final **CPE-304** review pass. A **Windows-first**
ship is the closest: capability consent is enforced + tested and only bundled first-party signed
manifests run. Confirm CPE-304 sign-off before a public platform release.

## Notes
Part of [[CPE-260]]/[[CPE-261]]. Surfaced while proving the catalog pipeline end-to-end (v0.12.0):
the catalog publish/verify loop works, but shipped apps can't fetch until the platform ships.

## Work Log
2026-07-14 — Filed after confirming no sidecar bundling exists + the threat-model gate.
