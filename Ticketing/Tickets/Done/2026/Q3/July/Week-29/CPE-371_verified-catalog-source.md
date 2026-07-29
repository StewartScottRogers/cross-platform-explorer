---
id: CPE-371
title: "Verified agent-catalog source loading (part 1 of CPE-308)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The first, headlessly-verifiable slice of CPE-308: load agent manifests from an **untrusted**
source (a user-pointed / later-fetched catalog dir) only when each carries a valid **first-party
ed25519 signature**, mirroring the host trust format (CPE-295) sidecar-locally (the sidecar can't
import the host crate — same precedent as the sidecar-local redactor). Additive: a bad or empty
source never removes already-loaded agents (last-known-good). The *remote* subscription / fetch /
auto-update UX is part 2 (design only, this ticket).

## Acceptance Criteria

- [x] `catalog::verify_manifest(bytes, sig_hex, trusted_keys)` — ed25519 over the manifest bytes,
      format-compatible with the host's `trust::verify_signature` (CPE-295).
- [x] `AgentRegistry::load_signed_source(dir, trusted_keys)`: each `*.json` needs a sibling
      `*.json.sig` (hex) that verifies against a trusted key, else skipped with a warning; verified
      manifests override by id (like a user dir); parse/validate reused from the normal loader.
- [x] A bad/empty/unverified source leaves the existing registry intact (last-known-good).
- [x] Optional wiring: load `CPE_AICONSOLE_CATALOG` dir with `CPE_AICONSOLE_CATALOG_KEYS`
      (unset ⇒ no-op, so default behaviour is unchanged).
- [x] Tests: verified accepted; tampered/untrusted-key/unsigned skipped; override-by-id;
      additive-load preserves existing. clippy clean.

## Notes
Trust anchor is injected (env), not hardcoded — key distribution + the single-authority question
(host `TrustStore` vs sidecar-local verify) are part-2 decisions. Schema migration (CPE-300) is a
no-op until a v2 exists; the existing `validate()` already rejects unknown-future schema.

## Work Log
2026-07-14 — Filed as part 1 of CPE-308.
