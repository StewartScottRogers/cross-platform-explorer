---
id: CPE-262
title: Contract/SDK crate — protocol & message envelope
type: Task
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
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

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-259]]. **Phase:** P1. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
