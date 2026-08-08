---
id: CPE-1472
title: "Sidecar handshake() trusts the child's self-declared hello.sidecar_id (latent cross-namespace escalation)"
type: Bug
status: Done
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

## Work Log

- 2026-08-08 — Fixed (took the preferred/strong option).

  Gave `handshake()` a new required `expected_id: &str` parameter — the id the host actually spawned — and
  added a check right after the existing `auth_token` check: if `hello.sidecar_id != expected_id`, the host
  sends `Rejected { code: Untrusted, reason: "sidecar id mismatch: expected '<X>', got '<Y>'" }` and returns
  `HandshakeError::Untrusted`, mirroring exactly how the CPE-275 auth-token mismatch is already handled (same
  error variant, same "send Rejected then bail" shape). `HandshakeOutcome.sidecar_id` is otherwise unchanged
  (still returned, now validated rather than trusted at face value) so downstream callers don't need to
  change how they consume the outcome.

  **Callers updated** (every `handshake(...)` call site in the workspace):
  - `src-tauri/src/lib.rs` — `sidecar_start_ai_console` passes `"ai-console"`; `sidecar_start_agent_board`
    passes `"agent-board"` (these are the literal ids those two functions spawn today).
  - `sidecar/host/tests/supervisor_e2e.rs`, `restart_e2e.rs` (×2) — `"echo"` (the bundled `echo_sidecar`
    test binary's `SIDECAR_ID`).
  - `sidecar/host/tests/hello_sidecar_e2e.rs` (×2) — `"hello"` (the bundled `hello_sidecar` test binary's
    `SIDECAR_ID`).
  - `sidecar/host/tests/ai_console_flow.rs` (ignored diagnostic test against a real built ai-console binary)
    — `"ai-console"`.
  - `sidecar/host/src/supervisor.rs` internal unit tests — `"fake"` (the `FakeConn` test helper's Hello
    always declares `sidecar_id: "fake"`).

  Added `handshake_rejects_a_mismatched_sidecar_id`: a Hello declaring `"fake"` against an `expected_id` of
  `"not-fake"` is rejected with `HandshakeError::Untrusted` and a `Rejected{code: Untrusted}` is sent, same
  shape as the existing wrong-token test.

  **Verification:**
  - `cargo build` + `cargo clippy --all-targets -- -D warnings` (sidecar/host) — clean.
  - `cargo test` (sidecar/host) — 102 passed, 1 ignored; new mismatch test + all existing handshake tests
    green.
  - `cargo build --lib --features sidecar-platform` and `cargo clippy --lib --features sidecar-platform -- -D
    warnings` (src-tauri, the only other crate calling `handshake`) — both clean, confirming the two
    production call sites compile with their new `expected_id` argument.

  PR: branch `cpe-1471-sidecar-ipc-hardening`, bundled with CPE-1471/CPE-1473 (same file cluster).
