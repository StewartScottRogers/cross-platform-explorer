---
id: CPE-300
title: Schema versioning & migration (manifests, state, profiles)
type: Task
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `migrate` in the `sidecar-host` crate: a generic forward-migration
framework (`Migrations::register(from, step).migrate_to_current(doc, current)`) that
runs ordered `v -> v+1` steps over a `serde_json::Value` carrying a `schema_version`.
Refuses documents newer than the build supports, errors on a missing step or a
non-advancing (buggy) step, and passes a current document through untouched. Plus
`read_schema_version` / `set_schema_version` helpers. This is the reusable policy the
manifest registry ([[CPE-264]]), stored state, and credential profiles ([[CPE-279]])
all adopt. 6 unit tests; clippy clean.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented + tested (6 tests) during dayshift. Done.
