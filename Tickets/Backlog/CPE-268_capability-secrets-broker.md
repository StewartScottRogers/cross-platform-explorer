---
id: CPE-268
title: "Capability: secrets broker (OS keychain, scoped)"
type: Task
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
---

## Summary

Brokered access to the OS secret store (Windows Credential Manager / macOS Keychain
/ Linux libsecret, via the `keyring` crate). Sidecars store and fetch named secrets
within **their own namespace** — they never see the raw store, other sidecars'
secrets, or a plaintext file. Secret values are handed out only for injection into
child processes, never returned to any UI/webview and never logged.

## Acceptance Criteria

- [ ] `secrets.set/get/delete(name)` scoped to the requesting sidecar's namespace.
- [ ] Backed by `keyring`; no plaintext-on-disk fallback (fail closed if no store).
- [ ] Values redacted from all logs/telemetry; never sent to the host UI.
- [ ] Cross-sidecar access impossible (namespace-enforced).
- [ ] Tests: set/get/delete round-trip, namespace isolation, redaction.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]]. Foundation for
the AI Console vault [[CPE-279]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
