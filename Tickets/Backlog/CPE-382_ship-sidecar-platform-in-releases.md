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

The threat model is **NOT production-signed-off** (`docs/security/threat-model.md`): shipping the
platform to end users is gated on **CPE-296** (capability/manifest consent-gate UI, ⛔) and the
**CPE-304** milestone, and non-Windows keychains (**CPE-322**). Do not ship the platform to users
before those land — bundled first-party manifests are auto-consented, but the broader platform
sign-off is incomplete.

## Notes
Part of [[CPE-260]]/[[CPE-261]]. Surfaced while proving the catalog pipeline end-to-end (v0.12.0):
the catalog publish/verify loop works, but shipped apps can't fetch until the platform ships.

## Work Log
2026-07-14 — Filed after confirming no sidecar bundling exists + the threat-model gate.
