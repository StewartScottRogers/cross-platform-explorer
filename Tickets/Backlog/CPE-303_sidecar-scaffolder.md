---
id: CPE-303
title: Sidecar scaffolder ("create-sidecar" generator)
type: Task
status: Open
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

To keep "adding sidecars" cheap over years, ship a generator that scaffolds a new
sidecar (crate + frontend stub + manifest + conformance test wiring) from a
template, so a new Mega-Feature starts compliant on day one. This is what makes the
platform a genuine platform, not a one-off.

## Acceptance Criteria

- [ ] One command scaffolds a working sidecar that handshakes, mounts an empty UI,
      and passes the conformance kit ([[CPE-301]]).
- [ ] Template documents where to add capabilities, UI, and logic.
- [ ] Referenced from the SDK docs ([[CPE-273]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-273]], [[CPE-301]]. **Phase:** P4/P5. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
