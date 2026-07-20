---
id: CPE-811
title: Transport-neutral contract envelope (serde-JSON RPC)
type: feature
component: Backend
priority: high
status: Open
tags: ready
created: 2026-07-20
epic: CPE-810
estimate: 3-4h
---

## Summary
Child of CPE-810. Define the transport-neutral request/response **envelope** that both the local
in-process path and the future network Client(Rust) speak. Own **serde-JSON** framing (decision:
wire format), reusing the proven `sidecar/contract` pattern: a `ContractVersion` + `negotiate()`,
a Hello/Welcome handshake, and a `schema_version` field. Pure crate, no Tauri, headless-testable.

Carry a `principal`/`session` slot in the envelope from day one so the future **multi-client** model
(decision: both, single-user first) is not precluded — even though single-user ships first and leaves
it defaulted.

## Acceptance Criteria
- [ ] A `contract` crate with the request/response envelope, `ContractVersion`, and `negotiate()`.
- [ ] Hello/Welcome handshake + `schema_version`, mirroring `sidecar/contract`'s approach.
- [ ] Envelope reserves a `principal`/`session` field (defaulted for single-user) for later multi-client.
- [ ] Version-mismatch negotiation is unit-tested; no Tauri dependency; clippy clean both modes.

## Work Log
