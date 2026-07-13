---
id: CPE-275
title: IPC security hardening (auth, no cross-sidecar visibility)
type: Task
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Lock down the host↔sidecar channel. Each sidecar's channel is authenticated to that
process, capabilities are enforced server-side, and there is no discoverable path
for one sidecar to reach another or for an outside process to impersonate a
sidecar. Complements the secret-broker guarantees.

## Acceptance Criteria

- [ ] Per-sidecar channel with a launch-time token; unauthenticated/foreign
      connections rejected.
- [ ] Capability checks enforced at the broker, not trusted from the client side.
- [ ] No shared channel/registry that exposes one sidecar to another.
- [ ] Threat-model note in the ADR; tests for impersonation + undeclared-capability
      rejection.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]], [[CPE-265]]. **Phase:** P5. **Epic:** [[CPE-260]].

## Resolution

Hardened the channel with a **per-launch auth token** (contract 1.2, additive): `spawn_process` generates a random token (`generate_launch_token`), passes it to the child via env `AUTH_TOKEN_ENV`, and exposes it via `ProcessConnection::launch_token()`. Each sidecar (echo, hello, ai-console, and the scaffolder template) echoes it in `Hello::auth_token`; `handshake(.., expected_token)` rejects a Hello whose token doesn't match with `Rejected{Untrusted}` + `HandshakeError::Untrusted`, so a foreign process can't impersonate a sidecar. **Verified end-to-end over real processes** (the E2E tests now pass the real spawned token) plus unit tests (matching/wrong/missing token, token uniqueness+format). The other CPE-275 guarantees already hold by construction and are covered: capability checks are enforced server-side by the broker (not trusted from the client), and each sidecar has its own private stdio channel with no cross-sidecar path. 72 host + 53 ai-console + 7 contract tests + clippy green.

**Note:** a STRIDE-style threat-model sign-off across the whole boundary is [[CPE-304]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Added per-launch channel auth token (contract 1.2), verified end-to-end, during dayshift. Done.
