---
id: CPE-1923
title: `verify-release-artifacts` passes at exit 0 on three hostile manifests — including a genuinely-signed **downgrade** to an older installer
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

An independent Security Auditor built hostile manifests against the updater verification gate and
found **three that pass at exit 0**. All three are pre-existing in
`crates/updater-verify/src/bin/verify-release-artifacts.rs` and `.../src/lib.rs` — none were
introduced by CPE-1908 — but that PR makes this binary the gate for **both** release channels, so
its blind spots now cover everything users install.

Every case was run exactly as `release-sidecar.yml` invokes the binary, with fixtures signed by a
throwaway minisign keypair inside a scratch worktree. No real signing material was used.

For contrast, the gate **correctly rejected**: a mixed plain/sidecar manifest; a sidecar-correct
basename on a foreign host; a signature made over different bytes; empty/absent `platforms`; and a
plain asset with a `#sidecar` URL fragment.

## Finding 1 — signed downgrade (the serious one)

`verify-release-artifacts.rs:352-383` binds the manifest's `version` to `tauri.conf.json`'s
`version`, but **never binds either to the artifact**. Nothing checks that a referenced asset's
basename or bytes belong to the version being shipped.

Exploitation: an actor with only **release-asset write** — a leaked PAT, or any workflow whose
`contents: write` `GITHUB_TOKEN` can be induced to upload; **no signing-key access needed** —
uploads the old, vulnerable `Cross-Platform.Explorer_(Sidecar)_0.1.0_x64-setup.nsis.zip` and its
**genuine** old signature to the new draft tag, and writes a `latest.json` whose `version` is the new
one. Demonstrated: `OK: verified 1 of 1 platform signature(s)`, **EXIT 0**. `latest.json` is itself
unsigned and the Tauri updater compares only the manifest's `version`, so published users
auto-"update" onto the older signed build.

This is the same downgrade outcome CPE-1873's endpoint pin exists to prevent, reached through the
**asset** instead of the endpoint.

**Note when fixing:** a blanket "basename must contain `version`" rule breaks macOS, whose updater
artifact is `<productName>.app.tar.gz` with no version in the name.

## Finding 2 — platform/asset mismatch passes

Same site. A manifest where `darwin-aarch64` serves the sidecar `.nsis.zip` and `windows-x86_64`
serves the sidecar `.app.tar.gz`, each with its own genuine signature: channel purity, URL prefix and
all signatures pass — `verified 2 of 2 platform signature(s)`, **EXIT 0**. Outcome is
denial-of-update (wrong-platform payload) rather than code execution, but the platform→asset mapping
is exactly what a channel-mixing bug corrupts.

Cheap fix: assert each platform key's expected extension set — `windows-*` → `.nsis.zip`/`.msi.zip`,
`darwin-*` → `.app.tar.gz`, `linux-*` → `.AppImage.tar.gz`/`.deb`.

## Finding 3 — channel inference is an unanchored substring match

`crates/updater-verify/src/lib.rs:397-404` decides the channel with
`basename.to_ascii_lowercase().contains("sidecar")`. A plain-channel installer uploaded as
`Cross-Platform.Explorer_1.2.3_x64-setup.nsis.zip.sidecar` reads as `Channel::Sidecar` and passes:
**EXIT 0**, `verifying: Cross-Platform.Explorer_1.2.3_x64-setup.nsis.zip.sidecar`.

So the guard proves "the name contains the word sidecar", not "this asset came from the sidecar
build". Anyone who can name a release asset can flip its apparent channel in **either** direction.

Fix: match against the real `productName` token (`Explorer_(Sidecar)_`), anchored, rather than a free
substring.

## Finding 4 (Low) — vacuous success when the signing secret is absent

`release-sidecar.yml:691`/`:716` (and identically `release.yml:291`/`:328`) gate both real steps on
`steps.sig.outputs.has == 'true'`. With `TAURI_SIGNING_PRIVATE_KEY` unset the job runs, skips both,
and concludes `success` — and `RELEASING.md`'s new publish gate reads that conclusion as proof of
verification. Impact is bounded today (with no signing key the matrix legs fail before producing a
`latest.json`), but **deleting one repo secret silently converts both gates into green no-ops.**

Fix: have the sig-detect step `exit 1` on a tag dispatch instead of emitting `has=false`, or have the
verify step emit a marker the doc check greps for.

## Acceptance criteria

- [ ] Bind the artifact to the version for findings 1 — and handle the macOS naming exception rather
      than breaking it.
- [ ] Assert the platform-key → extension mapping (finding 2).
- [ ] Anchor the channel inference to the real product-name token (finding 3).
- [ ] Close the absent-secret vacuous success (finding 4).
- [ ] **Land the auditor's hostile manifests as fixtures**, so each fix has something that goes red
      without it. Three of these passed at exit 0 with genuine signatures — reasoning about them is
      not enough.
- [ ] Re-run the full hostile set after the fixes and record which now fail and with what message.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1039's independent Security Auditor. Deliberately
scoped **out** of #1039, which fixes the channel-purity coverage gap it was filed for.

Related: **CPE-1908** (the channel-purity guard), **CPE-1873** (the endpoint pin this routes around),
**CPE-1901** (`--skip-pin-check` as a one-token kill switch, plus the unconditional
"matches the second in-repo pin" success line that prints even when the check was skipped),
**CPE-1874** (six shipped releases never signature-checked), **CPE-1917** (plain release broken 27 days).

## Coupling note added 2026-08-27 — read before fixing finding 4

CPE-1908's coverage ratchet (`src/lib/channelPurityCoverage.test.ts`) now **requires** the verify
step's `if:` to be exactly `steps.sig.outputs.has == 'true'`, via a `SIGNING_KEY_STEP_IF` constant.
That is the very condition finding 4 says must change.

So whoever closes finding 4 must update `SIGNING_KEY_STEP_IF` **in the same change**, or the ratchet
goes red on the fix — a guard blocking its own remedy. Flagged by PR #1039's Security Auditor on
re-audit, and a pointer comment was added at the constant.
