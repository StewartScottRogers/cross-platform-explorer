---
id: CPE-450
title: "Scheduled regeneration -> signed GitHub snapshot"
type: Feature
status: Open
priority: Medium
component: CI
tags: [needs-prereq]
estimate: 3-4h
created: 2026-07-15
epic: CPE-444
---

## Summary
A scheduled job regenerates a normalized, signed model-catalog snapshot from every reseller and publishes it to GitHub Releases, so clients load fast + offline. Reuses the agent-catalog signing/anti-rollback pipeline (CPE-308/376/371).

## Acceptance Criteria
- [ ] A workflow (scheduled + manual) fetches every reseller's model list, normalizes, and builds one catalog bundle.
- [ ] The bundle is ed25519-signed and content-hashed with a strictly-monotonic version (anti-rollback), published to Releases (reuse CPE-308/371 machinery).
- [ ] Signs with the EXISTING `CPE_CATALOG_SIGNING_KEY` (already configured — see below) via the `catalog-sign` tool; no new key needed.
- [ ] Documented cadence + how to run it manually.

## Notes
**Not key-gated.** Correction (2026-07-15): the catalog signing key already exists — `CPE_CATALOG_SIGNING_KEY`
is a set repo secret and its public key is embedded in `CATALOG_TRUSTED_KEYS` (CPE-380); release v0.13.0
already ships a signed *agent* catalog bundle. This ticket reuses that same key + the `catalog-sign`
machinery for the *model* snapshot — so it's a build task (the model-snapshot job), not a key-procurement
gate. Retag to `ready` when picked up. `needs-prereq` now only reflects CPE-445..448 (the model data),
which are DONE — so this is effectively actionable.
