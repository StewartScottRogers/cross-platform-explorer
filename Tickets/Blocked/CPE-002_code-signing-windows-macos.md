---
id: CPE-002
title: Add code signing for Windows and macOS installers
type: Task
status: Blocked
priority: High
component: Packaging
estimate: 3-4h
created: 2026-07-10
closed:
---

## Summary

Without OS code signing, users see "unknown developer" / SmartScreen warnings on install, and macOS
Gatekeeper blocks the app. Acquire signing credentials and wire them into the release workflow so
published installers are trusted.

## Acceptance Criteria

- [ ] Windows: obtain an OV/EV code-signing certificate; sign the NSIS/MSI installer in CI
- [ ] macOS: enrol in the Apple Developer Program; set APPLE_* secrets and enable notarization
- [ ] Signing secrets stored as GitHub repo secrets, never committed
- [ ] A fresh install on each OS shows no unsigned-developer warning
- [ ] RELEASING.md updated to describe the signing setup as done rather than optional

## Resolution

*(Not closed — deferred, not declined. See Notes.)*

## Work Log

2026-07-11 — Picked up as part of "work the open tickets". Assessed feasibility before writing code.
2026-07-11 — Determined this cannot be completed by the agent: both acceptance criteria depend on purchased, identity-verified certificates. Apple requires Developer Program enrolment (~$99/yr, identity verification); Windows requires an OV/EV Authenticode cert from a CA (paid, with vetting that commonly takes days to weeks). No amount of code closes this gate.
2026-07-11 — Did the engineering that CAN be done without certs: expanded the signing block in .github/workflows/release.yml with the exact secret names, formats, and the Windows-vs-macOS mechanism difference. Kept it COMMENTED — deliberately. Setting APPLE_* to empty/missing secrets makes tauri-action's signing step fail rather than skip, which would break the currently-green release pipeline for zero benefit. Turnkey once certs exist: uncomment the block, add the secrets.
2026-07-11 — Moved to Blocked/ with the owner checklist below.

## Notes

**Blocked on:** paid, identity-verified code-signing certificates. This is a procurement gate, not an
engineering one — no code change can clear it.

**Unblocks when:** the certificates below are obtained and their secrets added to the repo.

### Next Actions — Owner

**macOS** (unblocks the .dmg / .app):
- [ ] Enrol in the Apple Developer Program (~$99/yr) — https://developer.apple.com/programs/
- [ ] Create a **Developer ID Application** certificate; export as `.p12`
- [ ] Generate an app-specific password for the Apple ID (for notarization)
- [ ] Add repo secrets: `APPLE_CERTIFICATE` (base64 of the .p12), `APPLE_CERTIFICATE_PASSWORD`,
      `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`
- [ ] Uncomment the `APPLE_*` block in `.github/workflows/release.yml`

**Windows** (unblocks the .msi / .exe):
- [ ] Buy an Authenticode code-signing certificate (OV is cheaper; **EV** is what actually clears
      SmartScreen reputation immediately)
- [ ] Note: EV certs are usually hardware/HSM-bound, so signing on a GitHub-hosted runner generally
      requires a **cloud signing service** (e.g. Azure Trusted Signing, SSL.com eSigner) or a
      **self-hosted runner** with the token attached. Decide which before buying.
- [ ] Configure `bundle.windows` in `src-tauri/tauri.conf.json` (`certificateThumbprint`,
      `digestAlgorithm`, `timestampUrl`) — Windows signing is NOT driven by env vars in the workflow
- [ ] Verify a fresh install shows no SmartScreen warning

**Then:** move this ticket back to `Backlog/` (or work it directly) and update RELEASING.md.

**Note on scope:** this is unrelated to the *updater* signing key, which is already configured and
working — that key signs update payloads, not the installers, and is not a CA-issued certificate.
