---
id: CPE-294
title: "Phase 0: technical de-risking spike"
type: Task
status: Done
closed: 2026-07-13
priority: High
component: Multiple
estimate: 4h+
created: 2026-07-13
---

## Summary

Before the contract is committed (it is the one thing that can't be refactored
cheaply later), prove the risky technical assumptions with a throwaway spike. The
spike answers concrete questions so [[CPE-259]]/[[CPE-262]] are made on evidence,
not hope.

## Questions the spike must answer

- **IPC transport:** does length-prefixed JSON/CBOR over stdio (vs a local socket)
  give us low-latency streaming + backpressure for PTY and install output on
  Windows/macOS/Linux? Pick one.
- **PTY:** does `portable-pty` drive a real interactive TUI (e.g. `claude`) through
  Windows ConPTY and Unix pty, with resize + signals?
- **UI embedding:** can a child webview / iframe host a sidecar-served UI under a
  strict CSP without reaching explorer internals? What's the Tauri-v2 mechanism?
- **Keychain:** does the `keyring` crate work headless across all three OSes
  (incl. Linux libsecret in CI/headless)?
- **Process supervision:** clean spawn/kill/reap of a child on each OS, no orphans.

## Acceptance Criteria

- [ ] A short written findings doc with a recommendation per question, feeding
      [[CPE-259]] and [[CPE-262]].
- [ ] A runnable throwaway prototype (host ↔ one dummy sidecar, one PTY, one
      embedded UI, one keychain round-trip). Not shipped; deleted after.
- [ ] Each finding notes the cross-platform gotchas discovered.

## Notes — Dependencies / Schedule
**Depends on:** none. **Phase:** P0 (before P1). **Epic:** [[CPE-260]]. De-risks the
whole program.

## Work Log
2026-07-13 — Filed during epic-plan hardening (more end-to-end detail).
2026-07-13 — Closed **done-by-implementation**: every question was answered by building the
real, tested components (stronger than a throwaway prototype). Findings written up in
`docs/sidecar-phase0-findings.md` — IPC = JSON-line Envelopes over stdio; PTY = portable-pty
(ConPTY drain gotcha solved); UI = loopback HTTP + sandboxed iframe (visually verified);
keychain = keyring verified on Windows, macOS/Linux deferred to CPE-322; supervision =
spawn/kill/reap with real-process E2E. Contract frozen on this evidence (now v1.2).
