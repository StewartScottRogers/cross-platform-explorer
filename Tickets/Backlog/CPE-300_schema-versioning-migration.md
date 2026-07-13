---
id: CPE-300
title: Schema versioning & migration (manifests, state, profiles)
type: Task
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
---

## Summary

The contract isn't the only thing that evolves. Sidecar manifests, agent manifests,
stored sidecar state, and credential-profile records all change shape over years.
Without a migration discipline, a future release silently breaks old data. Define a
versioned-schema + forward-migration policy for **all** persisted formats.

## Acceptance Criteria

- [ ] Every persisted/loaded schema carries a `schemaVersion`.
- [ ] A loader runs ordered migrations old → current; unknown-future versions are
      refused with a clear message rather than mis-parsed.
- [ ] Documented policy: additive vs breaking, and how a new app version migrates
      existing user data on first run.
- [ ] Round-trip + migration tests for each schema (agent manifest, sidecar
      manifest, stored state, credential profile).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]], [[CPE-264]]. **Phase:** P1. **Epic:** [[CPE-260]].
Complements the contract semver in [[CPE-263]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
