---
id: CPE-375
title: "Catalog remote fetch + runtime reload (CPE-308 part 2, slice 2b)"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [needs-decision]
estimate: 2-3h
created: 2026-07-14
---

## Summary

Wrap the tested apply core (CPE-373) with the network + runtime pieces: download a catalog bundle
from a configured source into a staging dir, run `apply_bundle`, then make the running sidecar pick
up the new manifests without a restart.

## Acceptance Criteria

- [ ] Host-mediated fetch (src-tauri `ureq`, reuse `keyverify::resolve_proxy` + `CPE_OFFLINE`,
      CPE-369): the **host** holds the configured source URL — no general fetch exposed (no SSRF).
- [ ] Download → staging → `sidecar_host::catalog::apply_bundle` → persist version map.
- [ ] Runtime reload: the sidecar re-runs `load_signed_source` and swaps its registry (needs
      `ConsoleState.registry` behind a lock) + a `/api/catalog/reload` trigger.
- [ ] Offline/air-gapped: `CPE_OFFLINE` disables the fetch; the persisted catalog still loads.

## Notes — why `needs-decision`
Depends on [[CPE-373]]/[[CPE-372]]. The **default first-party source URL is unresolved** (who hosts
it) — the *enterprise "point at your own signed catalog"* path is buildable against a configured URL
now, but the first-party auto-update flow needs that decision. The actual network round-trip is
runtime-only-verifiable (like `host.verify_key`). Runtime reload also requires making the sidecar
registry mutable — a small `ConsoleState` change with its own regression surface.

## Work Log
2026-07-14 — Filed: split the network + reload out of CPE-373 once the apply core landed tested.
