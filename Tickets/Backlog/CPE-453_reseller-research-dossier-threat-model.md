---
id: CPE-453
title: "Reseller research dossier + threat model"
type: Task
status: Open
priority: High
component: Docs
tags: [ready]
estimate: 1-2h
created: 2026-07-15
epic: CPE-444
---

## Summary
Capture the reseller research (endpoints, auth, shapes, sources) as a doc, and a threat model for the model-catalog: egress allow-list per reseller, untrusted model metadata, per-reseller key handling, and GitHub-snapshot signing/anti-rollback.

## Acceptance Criteria
- [ ] `docs/design/model-resellers.md`: table of each reseller (id, model-list endpoint, auth, shape, tier) with citations.
- [ ] Threat-model section (STRIDE, extends CPE-304/forge-threat-model): multi-host allow-listed egress, untrusted model metadata (advisory only), per-reseller keys at rest/in transit/logs, signed-snapshot integrity + anti-rollback.
- [ ] Records the invariants CPE-447/450/451/452 must satisfy.
- [ ] Sources cited.

## Notes
Do this early - it pins the endpoints CPE-446/448 encode and the egress invariants CPE-447 enforces.
