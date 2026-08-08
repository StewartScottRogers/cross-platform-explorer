---
id: CPE-1473
title: "Use ed25519 verify_strict for sidecar manifest & catalog signatures (defense-in-depth)"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-862
created: 2026-08-08
---
## Vector (found in the crypto/IPC deep audit, 2026-08-08)
`sidecar/host/src/trust.rs:~50` `key.verify(msg, &sig)` (consumed by `catalog::verify_index`/`apply_bundle`) uses
NON-strict ed25519 verification, which accepts non-canonical/malleable signatures.

## Reachability / severity
INFO / hardening only — NOT exploitable here: acceptance is gated by content SHA-256 + monotonic version, and
signature malleability cannot forge a signature for a message an attacker lacks one for. `verify_strict` is the
recommended default (rejects small-order/non-canonical points) — worth adopting as defense-in-depth.

## Fix direction
Switch `key.verify(...)` → `key.verify_strict(...)` at trust.rs:~50 (and any sibling ed25519 verify). Confirm the
existing signature-verify tests still pass; add a case with a non-canonical/malleable signature that verify_strict
rejects if easy to construct.

## Effort / blast radius
XS / one-liner + test. Epic CPE-862. Can batch with the sidecar cluster (CPE-1471/1472) but different file
(trust.rs vs supervisor.rs) — same worker is fine.
