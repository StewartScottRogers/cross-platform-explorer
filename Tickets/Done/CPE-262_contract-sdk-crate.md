---
id: CPE-262
title: Contract/SDK crate — protocol & message envelope
type: Task
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The single shared surface between host and every sidecar. A small standalone
crate (`sidecar-contract`) defining the wire protocol: message envelope,
request/response + event types, handshake, lifecycle states, and capability
request/grant shapes. Both host and sidecars depend on **this crate only** — never
on each other. Transport-agnostic (serde types) so it can ride stdio/pipe/socket.

## Acceptance Criteria

- [ ] `sidecar-contract` crate with serde-serializable envelope (id, kind,
      version, payload) and typed Request/Response/Event enums.
- [ ] Handshake message (host↔sidecar) carrying contract version + capability set.
- [ ] Lifecycle states defined (Starting, Ready, Draining, Stopped, Failed).
- [ ] No dependency on `app_lib` or any sidecar crate; compiles standalone.
- [ ] Unit tests for round-trip (de)serialization of every message type.

## Resolution

Created the standalone `sidecar/contract` crate (`sidecar-contract`): the framed
`Envelope` (with its own `schema_version`, seeding [[CPE-300]]), the tagged
`Message` union (Hello/Welcome/Rejected/Request/Response/Event/Lifecycle/Error),
the `Capability`, `Lifecycle`, and error taxonomy (`ErrorCode`/`ContractError`,
seeding [[CPE-299]]), plus a `codec` module for JSON-line framing. Deliberately
standalone — depends on nothing in the explorer and is not in its workspace, so the
delete-test holds. 7 unit tests (round-trip every message, capability serde);
`cargo test` + `cargo clippy -D warnings` green.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented + tested the crate (7 tests, clippy clean). Version
negotiation primitive lives here too (see [[CPE-263]]). Done.
