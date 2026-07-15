---
id: CPE-451
title: "Client fetch + verify + hot-reload of the snapshot"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-15
epic: CPE-444
---

## Summary
The client downloads the signed model-catalog snapshot (host-mediated, allow-listed), verifies signature + anti-rollback, and hot-reloads the registry without a restart. Offline/stale handling + refresh cadence.

## Acceptance Criteria
- [~] Host-mediated fetch of the snapshot from the app's Releases; **signature + monotonic-version verified before use**. *(Verify core done: `verify_snapshot` + `accept_snapshot`; the host-mediated allow-listed fetch that feeds them is the remaining wiring.)*
- [ ] Hot-reload swaps the model catalog live (mirror CPE-375 registry reload).
- [ ] Offline/stale: fall back to the last good snapshot with a clear 'as of <date>' indication; manual + periodic refresh. *(`generated_at` is carried on `ModelSnapshot` for the "as of" indicator; the fallback/refresh runtime is not built.)*
- [x] Unit tests on the verify logic. *(Verify + anti-rollback covered; stale-fallback tests come with the fallback runtime.)*

## Work Log

- 2026-07-15 — Landed the **client verify half of the model-snapshot core** on branch
  `CPE-450-451-model-snapshot-core` (same `sidecar/ai-console/src/model_snapshot.rs` module as
  CPE-450): `verify_snapshot(&snap, sig_hex, &[trusted_pubkey_hex])` — fail-closed detached-ed25519
  verification over `canonical_bytes`, and `accept_snapshot(current_version, &incoming)` —
  strictly-monotonic anti-rollback mirroring `sidecar_host::catalog::gate_manifest`. Unit-tested (8
  tests total for the module): verify fails under a wrong key / tampered models / tampered
  version / tampered timestamp / bad-hex or wrong-length signature; anti-rollback accepts a strictly
  higher version and rejects equal-or-lower; malformed JSON never panics. `cargo test model_snapshot`
  and `cargo clippy --all-targets -D warnings` clean.
- **Still open (runtime/GUI follow-ups, deliberately not built here):** the host-mediated,
  allow-listed **fetch** of the snapshot from Releases (CPE-376 path) that feeds `verify_snapshot`;
  **hot-reload** swapping the live `ResellerRegistry` (CPE-375 pattern) without restart; the
  **offline/stale** last-good-fallback with the "as of &lt;date&gt;" UI and manual/periodic refresh;
  and stale-fallback unit tests once that runtime exists. Ticket stays in Backlog.

## Notes
needs-prereq: CPE-450 (the snapshot must exist) + shares its signing-key gate.
