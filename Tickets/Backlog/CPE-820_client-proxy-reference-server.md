---
id: CPE-820
title: Client(Rust) proxy + reference Server binary + end-to-end guards
type: feature
component: Multiple
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. Close the remote loop: a **Client(Rust)** proxy that implements the contract over
the network, and a deployable **headless Server** binary (the CPE-815 crate + a network listener +
the CPE-816–818 security stack active). Prove `GUI → Client(Rust) → Server(Rust)` end-to-end for
single-user, with the security stack enforcing. Land the epic's CI guards: a **local-fast benchmark**
(local in-process path unregressed) and a **contract conformance** test (version negotiation across a
mismatched client/server). Prereqs: CPE-815, CPE-816, CPE-819.

## Acceptance Criteria
- [ ] Client(Rust) proxy + headless Server binary; end-to-end browse/preview over the network (loopback + one real remote).
- [ ] Security stack enforces on the remote path (authN + authZ + transport); default-deny holds.
- [ ] Local-fast benchmark guard: in-process path within budget of pre-epic baseline.
- [ ] Conformance test: mismatched client/server versions negotiate or fail cleanly.
- [ ] Architecture documented under `docs/design/`; GUI-verified end to end.

## Work Log
