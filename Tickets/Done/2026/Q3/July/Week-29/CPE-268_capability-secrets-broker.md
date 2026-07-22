---
id: CPE-268
title: "Capability: secrets broker (OS keychain, scoped)"
type: Task
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
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
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]]. Gated by user
consent [[CPE-296]]; redaction shares the [[CPE-298]] utility; foundation for the
AI Console vault [[CPE-279]]; reviewed in [[CPE-304]].

## Resolution

Implemented `providers::secrets`: a `SecretBackend` trait (set/get/delete) and
`SecretsProvider` serving `Capability::Secrets` via `secrets.set/get/delete`. The
keychain "service" embeds the broker-supplied sidecar id
(`com.cross-platform-explorer.sidecar.<id>`), so a sidecar can only ever touch its
**own** namespace — one sidecar cannot read another's secret (tested). Values are
returned only to the requesting sidecar process (for injecting into its child); the
provider never logs them. Real backend `KeyringBackend` (Windows Credential Manager
via `keyring`, windows-native) with **no plaintext-on-disk fallback** — it fails
closed. 5 in-memory tests (round-trip, missing→null, delete, namespace isolation, bad
params) + a real-keychain round-trip test (verified passing with `--ignored`). 44 unit
+ 3 E2E + clippy green.

**Deferred (small):** macOS/Linux keychain backends (same `keyring` API, needs their
store features when built there); the shared log-redaction utility is [[CPE-298]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented during dayshift; verified the real Windows Credential Manager
round-trip. Done.
