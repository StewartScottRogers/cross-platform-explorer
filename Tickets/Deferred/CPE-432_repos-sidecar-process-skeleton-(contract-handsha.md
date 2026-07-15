---
id: CPE-432
title: "Repos sidecar process skeleton (contract handshake)"
type: Feature
status: Deferred
priority: High
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-429
---

## Summary
Stand up repos as a real sidecar tenant (CPE-429/260): handshake + capability request + protocol loop,
and its own loopback UI server, like the AI Console skeleton (CPE-277/271).

## Acceptance Criteria
- [x] Emits Hello, reaches Ready, requests only needed capabilities (secrets, network-broker).
- [ ] Serves its own UI on loopback; announces the URL to the host.
- [ ] Bundled + wired behind the sidecar-platform feature; conformance kit passes.
- [x] One-way dependency (only sidecar-contract); process isolation preserved.

## Work Log
2026-07-15 - Nightshift (work-all). Estimate 2-3h. Plan: implement the pure protocol skeleton (hello/on_message/Reaction + requested capabilities) mirroring the AI Console CPE-277, plus a stdio main.rs handshake loop. The UI server, host launch/supervision, bundling, and conformance are the heavy integration that remains.
2026-07-15 - Landed the handshake skeleton. New `src/protocol.rs`: `SIDECAR_ID="repos"`, `REQUESTED_CAPABILITIES=[Context, Secrets, Storage, Events, Network]`, `Reaction`, pure `hello()`/`on_message()` (Welcome→Ready, Rejected→exit 1, WillQuit/`sidecar.shutdown`→exit 0, other Request→ack) — mirrors ai-console CPE-277. New `src/main.rs`: thin stdio driver (emit Hello, read JSON-line envelopes, Welcome→Ready, route via on_message). Depends only on `sidecar-contract` (+ serde/serde_json) — one-way dependency preserved.
2026-07-15 - Verified headlessly: 4 new unit tests in `protocol` (23 crate unit tests total) + a real-process integration test `tests/handshake.rs` (spawns the built `repos` binary, asserts Hello → Ready → clean exit on shutdown). `cargo test` green, `cargo clippy --all-targets -D warnings` clean. **AC1 + AC4 done.**
2026-07-15 - REMAINING (kept open): AC2 (own loopback UI server + `ui:<url>` announce) and AC3 (bundle the `repos` sidecar behind the `sidecar-platform` feature + host launch/supervision + conformance kit). These are the heavy UI + host-integration slices that need a real GUI run to verify; AC2 overlaps the left-pane work in CPE-435. The base process is ready to grow those arms exactly as ai-console's `main.rs` did.

## Work Log
2026-07-15 (dayshift) — Moved to Deferred. Deferred-on: **superseded for v1** by the native Repositories feature (host `forge_browse`/`forge_clone`/`forge_*_token` + `RepoBrowser.svelte`), which delivers browse+clone+credentials without launching a separate repos sidecar. The handshake skeleton (this ticket's landed core) stays valid. Revisit-when: process isolation of forge operations (untrusted-repo containment beyond the clone hardening) is required, or the two-way *mirror* engine (CPE-438 planner) needs a long-lived tenant. Not externally gated — a deliberate architecture choice; pickable if we decide to move forge into its own sidecar.
