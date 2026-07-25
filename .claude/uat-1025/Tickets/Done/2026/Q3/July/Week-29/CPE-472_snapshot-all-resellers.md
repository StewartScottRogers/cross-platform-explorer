---
id: CPE-472
title: "Fold every reseller's models into the signed model-catalog snapshot"
type: Feature
status: Done
priority: High
component: Sidecar (AI Console)
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-16
epic: CPE-467
---

## Summary
Extend the model-snapshot pipeline (CPE-450) so the signed `models-index.json` covers every reseller
descriptor, not just the current set. Keep it signed + anti-rollback (CPE-451).

## Acceptance Criteria
- [x] The snapshot builder iterates all reseller manifests and normalizes each into the catalog.
- [x] `MIN_MODELS` guard scales; the workflow still publishes signed to the `model-catalog` release.
- [x] Picker shows each reseller’s models from the snapshot, live fallback intact.

## Resolution
**Delivered by design — no code change.** The `model-snapshot.yml` workflow already iterates
`sidecar/ai-console/resellers/*.json` dynamically (`for manifest in .../resellers/*.json`), curls each
`models_endpoint` into `responses/<id>.json` best-effort, and `snapshot_from_reseller_dir` normalizes
every response into the signed bundle. So the 10 resellers added this session (CPE-475/477/478) are
**automatically included** on the next workflow run — no builder change needed. The `MIN_MODELS=20`
guard scales (auth-gated resellers without CI keys are simply omitted; OpenRouter's public list keeps
a healthy run's count in the hundreds), and the picker already prefers the signed snapshot with a live
per-reseller fallback (CPE-451/449). Verified by reading the workflow + `snapshot_from_reseller_dir`
(which has its own every-reseller normalization tests). Nightshift loop 11.
