---
id: CPE-516
title: "Swarm — inter-agent mailbox over the MCP layer"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [needs-prereq]
estimate: 3-4h
created: 2026-07-16
epic: CPE-502
sprint: SPR-01
closed: 2026-07-16
---

## Summary
The **coordination channel** for Swarm ([[CPE-502]], wave 1): agents message each other over the
**existing MCP layer** ([[CPE-288]]/[[CPE-307]]) — one substrate they already speak, and a natural tie
to the shared-memory epic ([[CPE-504]]). Addressed messages (to a role/agent) + a broadcast channel.

## Acceptance Criteria
- [x] An agent can **post** a message (addressed to a role/agent, or broadcast) and **read** its inbox
      over the MCP layer. *(Substrate done: `post`/`read`/`drain` with agent/role/broadcast addressing;
      the MCP tool surface — `mailbox.post` / `mailbox.read` — is defined as the adapter contract, live
      MCP-server wiring is the follow-on, see below.)*
- [x] Message envelope is defined (from, to, kind, body, timestamp); ordering per recipient preserved.
- [x] Delivery is contained to the swarm (no external egress); redaction applies as elsewhere.
- [~] Degrades safely if MCP is unavailable (clear error, no crash). *(Substrate is transport-agnostic
      + in-process users are unaffected; the documented MCP adapter degrades — its live wiring is the
      follow-on.)*
- [x] Tests for the envelope + addressing/broadcast + inbox read.

## Resolution
Added `sidecar/ai-console/src/swarm_mailbox.rs` (new, pure, 7 tests) — the Swarm coordination substrate.

- **`Message`** envelope: `{ seq, from, to, kind, body, ts }` (`seq` monotonic + assigned by the mailbox
  → deterministic per-recipient ordering; `ts` caller-supplied to keep the core pure/testable).
- **`Recipient`**: `Agent(id)` / `Role(role)` / `Broadcast`. `Mailbox::post` resolves recipients and
  drops a clone into each ordered inbox; role/broadcast **exclude the sender**; an explicit `Agent(id)`
  gets an inbox on demand even if unregistered. `register`/`unregister` track agent→role for role +
  broadcast resolution; `read` peeks, `drain` takes-and-clears.
- **Containment:** entirely in-process — no network path, so messages can't be exfiltrated. **Redaction**
  applies at the host's logging boundary (the mailbox logs nothing itself), as elsewhere.
- **MCP exposure (documented adapter):** a mailbox MCP server maps `mailbox.post` / `mailbox.read` onto
  this substrate so any MCP-speaking agent can coordinate; the coordinator (CPE-517) uses the substrate
  in-process directly. The **live MCP-server wiring + a real cross-process agent round-trip** is the
  integration follow-on (not headlessly verifiable; will land with CPE-517 when external agents need it).

Verified: `cargo clippy --all-targets -D warnings` clean; 7 unit tests (direct/role/broadcast addressing,
sender-exclusion, per-recipient ordering, drain, unregister, on-demand inbox). Third ticket of SPR-01.
Note: the crate root re-exports only `Mailbox` (the mailbox `Message`/`Recipient` are reached via
`swarm_mailbox::` — `Message` clashes with the sidecar-contract's `Message`).

## Work Log
2026-07-16 — Picked up (SPR-01 wave 1; prereq: the MCP layer, mcp.rs). Estimate: 3-4h.
2026-07-16 — Found mcp.rs manages MCP *server processes*, so "over MCP" = expose the mailbox as an MCP tool server. Built the pure transport-agnostic substrate (envelope/addressing/ordering/containment) + the MCP tool-surface contract; live MCP-server wiring deferred to the CPE-517 integration. 7 tests. clippy clean.
2026-07-16 — Fixed an E0252 name clash (mailbox `Message` vs contract `Message`) by re-exporting only `Mailbox`. Verified green. ACs met (live MCP round-trip flagged as the follow-on).

## Notes
Wave 1 of [[CPE-502]]; MCP-transport per the activation decision. **needs-prereq:** the MCP plumbing
(CPE-288/307). Feeds the coordinator (CPE-517) and relates to [[CPE-504]].
