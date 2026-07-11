---
id: CPE-002
title: Add code signing for Windows and macOS installers
type: Task
status: Open
priority: High
component: Packaging
estimate: 3-4h
created: 2026-07-10
closed:
---

## Summary

Without OS code signing, users see "unknown developer" / SmartScreen warnings on install, and macOS
Gatekeeper blocks the app. Acquire signing credentials and wire them into the release workflow so
published installers are trusted. The `.github/workflows/release.yml` already has a commented-out
`APPLE_*` block to fill in.

## Acceptance Criteria

- [ ] Windows: obtain an OV/EV code-signing certificate; sign the NSIS/MSI installer in CI
- [ ] macOS: enrol in the Apple Developer Program; set APPLE_* secrets and enable notarization
- [ ] Signing secrets stored as GitHub repo secrets, never committed
- [ ] A fresh install on each OS shows no unsigned-developer warning
- [ ] RELEASING.md updated to describe the signing setup as done rather than optional

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

Depends on the user acquiring paid certificates (external cost). Updater signing is separate and
already configured. If certs are not yet available, this may move to Blocked/ with a Next Actions note.
