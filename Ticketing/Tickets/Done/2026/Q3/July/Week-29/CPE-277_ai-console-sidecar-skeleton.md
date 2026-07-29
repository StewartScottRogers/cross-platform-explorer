---
id: CPE-277
title: AI Console sidecar skeleton (contract impl + empty pane)
type: Task
status: Done
priority: High
component: Multiple
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Stand up the AI Console as a real sidecar: its own crate/binary + its own frontend,
implementing the contract handshake, declaring the capabilities it needs (context,
secrets, storage, events), and rendering an empty pane through the host mount. Does
nothing useful yet — it proves the tenant boundary before any feature.

## Acceptance Criteria

- [ ] `sidecars/ai-console` process implements the contract; handshakes and reaches
      Ready under the supervisor.
- [ ] Serves its own minimal UI, embedded via the host UI mount ([[CPE-271]]).
- [ ] Declares its capability set; runs with no access it didn't request.
- [ ] Delete-test still green ([[CPE-272]]); explorer builds without it.

## Resolution

Created the standalone `sidecar/ai-console` crate (lib + bin), depending only on
`sidecar-contract` (one-way rule). The lib holds the sidecar's identity (`SIDECAR_ID`),
`REQUESTED_CAPABILITIES` (Context/Secrets/Storage/Events), and the base protocol as a
pure `on_message` state machine (Welcome→Ready, shutdown/WillQuit→exit, other requests
answered correlated) — the domain modules (agent registry, provider routing, secret
vault, lifecycle) will be added on top. `main.rs` is a thin stdio wrapper. Added
`ai-console` to the cross-OS `sidecar` CI job. 6 unit tests; **verified as a real
process** (piped a Welcome → it emits Hello with the four caps, then Ready). Delete-test
holds (standalone crate).

**Deferred to [[CPE-271]]:** "serves its own minimal UI embedded via the host UI mount"
— the UI mount doesn't exist yet; the backend skeleton (handshake + capabilities + runs
under the supervisor) is complete and validated.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Built the sidecar crate skeleton (lib+bin), added to CI, verified as a real
process. Done (UI portion carried to CPE-271). Starts the AI Console backend.
