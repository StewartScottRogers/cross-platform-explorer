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
- [ ] Dormant until a catalog signing key is configured (same gate as CPE-308) - scaffolding + guarded stub land now.
- [ ] Documented cadence + how to run it manually.

## Notes
needs-prereq: shares the CPE-308 signing-key gate (see agent-catalog-auto-update memory). Build guarded, activate when a key exists.
