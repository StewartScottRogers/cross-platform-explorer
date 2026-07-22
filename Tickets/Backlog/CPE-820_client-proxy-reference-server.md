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
- 2026-07-22 (nightshift) — **Conformance slice landed (headless).** Added a pure `handshake(hello,
  server_contract, …) -> Result<Welcome, Rejected>` to `cpe-contract` — the server's handshake decision
  (negotiate the wire version → Welcome with the agreed version + session, or Rejected
  `IncompatibleVersion` with a legible reason), total/never-panics. Plus `handshake_conformance_matrix`
  covering same-version, minor-mismatch-both-directions (negotiates down), major-mismatch-both-directions
  (clean reject), and a real-client-vs-CONTRACT_VERSION accept. **AC #4 (conformance) met.** contract
  tests 10/10; clippy clean.
- 2026-07-22 (nightshift) — **Status reconcile: AC #1–#4 substantially met headlessly.** The remote loop
  is built and green in CI:
  - **AC #1 (Client + Server, loopback):** `cpe-net::Client` proxy (`connect`/`call`/`call_stream`) +
    the deployable `cpe-server-ref` binary (dispatcher + security chain over a TCP listener) prove
    `Client(Rust) → Server(Rust)` browse over loopback. The WebSocket transport (CPE-819 slices 1–3,
    #192/#193/#195/#196) adds the **browser-reachable** path end-to-end (upgrade → handshake → request/
    response, Ping/Pong keepalive). *Remaining:* one **real remote** host (not loopback) + browse/preview
    — user-gated.
  - **AC #2 (security enforces remotely):** the chain evaluates Transport→AuthN→AuthZ per request over
    the wire; default-deny, path-scope, capability, and API-token denials are each proven over the socket
    (net suite). **Met.**
  - **AC #3 (local-fast guard):** the in-process timing guard lives in the net suite (best-of-N
    in-process vs loopback, `lib.rs` ~L563), de-flaked in CPE-858. **Met.**
  - **AC #4 (conformance):** met (above).
  - **AC #5:** architecture is under `docs/design/SERVER-ARCHITECTURE.md`; a dedicated remote-transport
    design note + the **GUI-verify end-to-end** are the genuinely user-gated remainder.
  Net of this: CPE-820 is **headless-complete**; what's left (real-remote browse, GUI-verify) needs a
  running server + the GUI, i.e. the user. Left in Backlog rather than closed for that reason.
