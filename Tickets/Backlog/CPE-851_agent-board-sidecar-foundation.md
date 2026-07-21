---
id: CPE-851
title: Agent Board sidecar — crate foundation (handshake + loopback UI + manifest)
type: feature
component: Backend
priority: medium
status: Open
tags: ready
created: 2026-07-21
epic: CPE-850
estimate: 2-3h
---

## Summary
Foundation of the out-of-process Agent Board (CPE-850). The scaffolded `sidecar/agent-board` crate, made
a compliant, running sidecar on the `repos` template: it emits `Hello`, reaches `Ready` on `Welcome`,
answers `sidecar.shutdown`, and serves its **own** dependency-free loopback HTTP UI (a board-branded page
to start, real data is CPE-852), announcing the URL via a `ui:<url>` Status event. Depends only on
`sidecar-contract` (ADR 0001). `sidecar.json` names it "Agent Board" with a `local_port` UI. Added to CI's
3-OS `sidecar` job.

## Acceptance Criteria
- [ ] `sidecar/agent-board` builds; a `lib.rs` holds the pure protocol (`hello`, message handling) with
      `main.rs` a thin stdio wrapper (mirrors `repos`).
- [ ] On `Welcome` it emits `Lifecycle::Ready`, starts the loopback UI server, and announces `ui:<url>`.
- [ ] It serves an HTML page (board-branded placeholder) on an ephemeral 127.0.0.1 port; `sidecar.shutdown`
      exits cleanly.
- [ ] `sidecar.json`: `name` "Agent Board", `ui: { local_port, announced }`; depends only on the contract.
- [ ] cargo-tested (protocol handshake + the UI HTML), added to CI's `sidecar` job; clippy clean.

## Work Log
