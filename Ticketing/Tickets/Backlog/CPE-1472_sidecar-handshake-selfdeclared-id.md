---
id: CPE-1472
title: "Sidecar handshake() trusts the child's self-declared hello.sidecar_id (latent cross-namespace escalation)"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-862
created: 2026-08-08
---
## Vector (found in the crypto/IPC deep audit, 2026-08-08)
`sidecar/host/src/supervisor.rs:~141-146`: `HandshakeOutcome { sidecar_id: hello.sidecar_id, .. }` returns the
child's self-declared id UNVALIDATED against the id the host actually spawned. If any host wiring keys grants /
keychain secrets / storage namespace off this returned id, a compromised sidecar A could set
`hello.sidecar_id = "B"` and read sidecar B's stored secrets or private storage dir (cross-namespace escalation).

## Reachability
LATENT / not currently exploitable — the shipped runtime hardcodes the literal `"ai-console"` at `set_grants`
(`src-tauri/src/lib.rs:~7612`), `dispatch` (`~7650`), consent load (`~8832`), and the secrets/storage service
names, so the self-declared id is never used. This is a trap for the planned MULTI-sidecar host.

## Fix direction
Give `handshake` an `expected_id` param and reject a mismatched `hello.sidecar_id` (return
`HandshakeError::Untrusted`), the same way the per-launch `auth_token` (CPE-275) is checked; OR document loudly at
the return site that callers MUST discard the returned id and use the spawned manifest id. Prefer the former.

## Effort / blast radius
S / supervisor.rs handshake. Epic CPE-862. Serialize with CPE-1471 (same file).
