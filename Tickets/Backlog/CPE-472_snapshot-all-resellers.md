---
id: CPE-472
title: "Fold every reseller's models into the signed model-catalog snapshot"
type: Feature
status: Open
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 1h
created: 2026-07-15
epic: CPE-467
---

## Summary
Extend the model-snapshot pipeline (CPE-450) so the signed `models-index.json` covers every reseller
descriptor, not just the current set. Keep it signed + anti-rollback (CPE-451).

## Acceptance Criteria
- [ ] The snapshot builder iterates all reseller manifests and normalizes each into the catalog.
- [ ] `MIN_MODELS` guard scales; the workflow still publishes signed to the `model-catalog` release.
- [ ] Picker shows each reseller’s models from the snapshot, live fallback intact.
