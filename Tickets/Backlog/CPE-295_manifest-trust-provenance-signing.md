---
id: CPE-295
title: Manifest trust, provenance & signing (supply-chain security)
type: Feature
status: Open
priority: Critical
component: Backend
estimate: 4h+
created: 2026-07-13
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

## Work Log
2026-07-13 — Filed during epic-plan hardening. Closes the single biggest end-to-end
gap: arbitrary-code execution from user-supplied manifests.
