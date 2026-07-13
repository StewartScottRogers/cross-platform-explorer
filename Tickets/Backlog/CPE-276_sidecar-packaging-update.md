---
id: CPE-276
title: Sidecar packaging, signing & independent update/rollback
type: Feature
status: Open
priority: Medium
component: Packaging
estimate: 4h+
created: 2026-07-13
---

## Summary

How sidecar binaries ship, are **code-signed**, and update/rollback on their **own
cadence** — the point of "keep up with the market without ricochet." A sidecar can
be bundled with the app or fetched/updated independently of the explorer's release,
without breaking the delete-test or the explorer's own updater pipeline.

## Acceptance Criteria

- [ ] Defined layout for bundled sidecars + an install location for
      fetched-at-runtime sidecars.
- [ ] Sidecar binaries are **code-signed** (Windows/macOS) so they don't trip
      SmartScreen/Gatekeeper; signature verified before launch (ties to [[CPE-002]]
      code-signing and [[CPE-295]] trust).
- [ ] A sidecar can **update** to a new compatible-contract version without an
      explorer release; incompatible versions refused cleanly ([[CPE-263]]).
- [ ] **Rollback**: pin/restore a previous sidecar version; a failed update reverts
      to last-known-good rather than leaving a broken tenant.
- [ ] Contract-**major** bumps are coordinated (host advertises supported range;
      sidecars that need the new host are held until the host ships).
- [ ] Uninstall removes the binary, its storage ([[CPE-269]]), secrets scope, and
      registry entry.
- [ ] Does not disturb the explorer's own updater/latest.json flow.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-264]], [[CPE-265]], [[CPE-263]]. **Phase:** P5.
**Epic:** [[CPE-260]]. Related: [[CPE-300]] (migration), [[CPE-308]] (catalog updates).

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Hardened: added binary code-signing, rollback / last-known-good, and
coordinated contract-major upgrades.
