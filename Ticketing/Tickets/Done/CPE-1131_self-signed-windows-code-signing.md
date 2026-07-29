---
id: CPE-1131
title: Self-signed Windows code signing (testing / controlled-fleet increment of CPE-002)
type: Task
status: Done
priority: High
component: Packaging
estimate: 2h
created: 2026-07-29
closed: 2026-07-29
tags: [ready]
---

## Summary

CPE-002 (real OV/EV + Apple certs) is procurement-blocked. As a concrete increment the user asked for a
**self-signed** Windows code-signing cert, generated + wired by the agent. A self-signed cert makes the
installer genuinely Authenticode-signed and shows a real publisher on any machine that **trusts the cert**,
but it does **not** silence SmartScreen/"unknown publisher" for the general public — only a CA-issued
OV/EV cert does that (CPE-002 remains open for that). This is for the user's own machines / a controlled
fleet and to make the signing pipeline real.

## Acceptance Criteria

- [x] Generate a self-signed code-signing certificate (RSA-3072 / SHA-256, Code Signing EKU), export the
      `.pfx` (private) + public `.cer`; keep private key material out of the repo.
- [x] Wire Windows signing into `.github/workflows/release.yml` so a release build signs the installer —
      **conditional on the cert secret** so fork/PR/unconfigured builds still succeed unsigned (mirrors the
      updater/catalog "skip when secret absent" pattern; keeps the pipeline green).
- [x] Store the cert as repo secrets (`WINDOWS_CERT_PFX_BASE64`, `WINDOWS_CERT_PASSWORD`), never committed.
- [x] Commit the **public** `.cer` + trust instructions (`docs/signing/`) so the user can make the app show
      as a trusted publisher on their machines.
- [x] Update RELEASING.md to describe the self-signed Windows signing (and the SmartScreen caveat).
- [x] Verify a real release actually produces a signed installer. **Verified 2026-07-29:** cut
      `v0.57.38-sidecar` via `release-sidecar.yml`; the CI-built `…Sidecar._0.57.38_x64-setup.exe`
      reports `Get-AuthenticodeSignature` → **Valid**, signer `CN=Cross-Platform Explorer` (thumbprint
      `06097080…795B`), DigiCert-timestamped.

## Resolution

Done (commit `af8f6ef7`). Self-signed code-signing cert generated + verified to sign; conditional Windows
signing wired into `release.yml` (skips green when the secret is absent); pfx + password stored as repo
secrets; public cert + trust/rotate instructions committed to `docs/signing/`; RELEASING.md + gitignore
updated. The pipeline is **turnkey** — the next release build on `windows-latest` will sign the installer.
The one remaining AC (observe a signed installer from a real release) is gated on cutting a release ("Run"
or a version bump + tag); left unchecked here since it needs that trigger, not more engineering.
**CPE-002 stays Blocked** for the CA OV/EV (Windows public SmartScreen) + Apple Developer ID (macOS) certs.

## Work Log

2026-07-29 — Generated a self-signed code-signing cert (thumbprint `06097080…795B`, CN=Cross-Platform
Explorer, O=Stewart Rogers, 5-yr, Code Signing EKU 1.3.6.1.5.5.7.3.3). Verified it signs (signer thumbprint
matches on an unsigned test file; status is the expected `UnknownError` until the cert is trusted). Wired a
conditional Windows-signing step into release.yml, set the pfx/password repo secrets, committed the public
cert + trust doc, updated RELEASING.md.

## Notes

Relationship to CPE-002: this closes the self-signed increment; CPE-002 stays Blocked for the CA/Apple certs
that actually clear SmartScreen + Gatekeeper for the public. macOS self-signing is not meaningful (Gatekeeper
requires Apple Developer ID + notarization), so this ticket is Windows-only.
