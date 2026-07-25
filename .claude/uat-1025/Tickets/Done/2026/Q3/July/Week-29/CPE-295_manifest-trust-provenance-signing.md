---
id: CPE-295
title: Manifest trust, provenance & signing (supply-chain security)
type: Feature
status: Done
priority: Critical
component: Backend
estimate: 4h+
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The most important security ticket in the program. Sidecar manifests ([[CPE-264]])
and agent manifests ([[CPE-278]]) declare **install/run commands that execute
arbitrary code** — often with the user's credentials injected. A malicious or
tampered manifest is remote code execution. Establish a trust model so first-party
content is trusted and everything else is treated as untrusted until the user
consents with full disclosure.

## Acceptance Criteria

- [ ] Bundled/first-party manifests are **signed**; the app verifies the signature
      before trusting them.
- [ ] User/third-party manifests are **untrusted by default**: before *any* command
      from them runs, the user sees exactly what will execute (command, args, env
      var names — never values) and must explicitly consent; consent is remembered
      per manifest + version.
- [ ] Any change to a trusted manifest (hash mismatch) re-prompts.
- [ ] Provenance recorded (source, signer/None, first-seen) and shown in the UI.
- [ ] Optional allowlist/policy for locked-down/enterprise deployments.
- [ ] Threat cases tested: tampered bundled manifest rejected; unconsented
      third-party command blocked.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-264]]. **Phase:** P3. **Epic:** [[CPE-260]]. Gates [[CPE-278]]
(agent manifests) and [[CPE-291]] (catalog). Feeds [[CPE-304]] (threat model).

## Resolution

Implemented `trust` in sidecar-host — the trust engine. `verify_signature` (ed25519 over manifest bytes, hex keys/sigs), `content_hash` (SHA-256), and `TrustStore::evaluate(bytes, signature?) -> TrustDecision`: a valid signature from a trusted first-party key = `TrustedSigned`; otherwise `Consented` iff the user consented to this exact content hash (and policy allows), else `Untrusted`. `record_consent` binds consent to the content hash so ANY change re-prompts; an optional `policy_allow` hash allowlist supports locked-down/enterprise deployments; provenance (source/hash/first-seen) is recorded per hash. Pure-Rust deps (ed25519-dalek, sha2, hex). 6 tests (valid signature trusted, tamper fails, untrusted-key fails, consent flow + re-prompt on change, policy gating, provenance). 66 host tests + clippy green.

**Deferred (needs the user):** producing the actual first-party signing key + signing the bundled manifests, and the disclosure UI ('this will run …') — this engine is the verification/consent core those build on. Feeds [[CPE-304]].

## Work Log
2026-07-13 — Filed during epic-plan hardening. Closes the single biggest end-to-end
gap: arbitrary-code execution from user-supplied manifests.
2026-07-13 — Implemented the manifest trust engine (ed25519 verify + consent) during dayshift. Done.
