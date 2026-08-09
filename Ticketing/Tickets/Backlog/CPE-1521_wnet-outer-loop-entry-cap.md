---
id: CPE-1521
title: "Harden WNet discovery: cap the wnet_enum_level outer pagination loop (unbounded-growth on a pathological provider)"
type: Task
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-1517
created: 2026-08-09
---
## Why (opus adversarial review of PR #737 / CPE-1519, 2026-08-09)
The Windows WNet discovery walk (`src-tauri/src/lib.rs`, `wnet_enum_level`) hardens recursion **depth**
(`WNET_MAX_DEPTH=6`) and **buffer grows** (`WNET_MAX_BUFFER_GROWS=6`) against a pathological/hostile provider,
but the **outer pagination loop (~lib.rs:6008-6065) has no cumulative-entry or iteration cap.** A WNet provider
that repeatedly returns `ERROR_SUCCESS` with `count > 0` **without advancing its cursor** would spin this loop
forever and grow the `out` Vec unboundedly.

**Impact is bounded in practice** (why this is Low / was not a merge blocker): `discover_network_windows` is
`async` + `spawn_blocking` + `recv_timeout(DISCOVERY_TIMEOUT = 6s)`, so the **foreground stays responsive** — it
abandons the worker after 6s. Only the *detached* background thread would keep allocating. And the real trigger
is a **malicious/broken LOCAL WNet provider** (the local network-provider chain, not the remote NAS), which is a
high bar (attacker already has local code). Still, it's a consistency gap with the other two caps and a real (if
remote) memory-DoS vector, so close it.

## Scope
- In `wnet_enum_level`'s outer loop, track a **running total of entries emitted across outer iterations** and
  **`break` when it exceeds a sane cap** (e.g. a few thousand shares — far above any real network neighborhood),
  with the same **skip-on-error / partial-results** semantics as the depth and buffer-grows caps (return what
  was gathered, don't error the whole walk).
- Optionally also cap the raw outer-iteration count as a belt-and-suspenders guard.
- Add a unit test for the cap if the loop can be exercised with a fake enumerator seam; otherwise document why
  it's only reachable via the real FFI and covered by inspection.

## Verify
- `cargo build` + `cargo clippy --all-targets -- -D warnings` (both feature modes) clean in `src-tauri`.
- `cargo test --lib -p cpe-server` green (the pure mapping tests unaffected).

## Notes
Tiny, self-contained backend hardening. Follow-up to the merged CPE-1519 backend slice; same epic (CPE-1517,
LAN discovery). Matches the crew's tuned default: bound untrusted-input-driven loops rather than trust the
provider to terminate. Reviewer: opus adversarial pass on PR #737.
