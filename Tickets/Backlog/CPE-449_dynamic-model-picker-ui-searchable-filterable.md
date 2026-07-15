---
id: CPE-449
title: "Dynamic model picker UI (searchable/filterable)"
type: Feature
status: Open
priority: High
component: Frontend
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-15
epic: CPE-444
---

## Summary
The AI Console launcher's model selector becomes a searchable, filterable list over the full dynamic catalog (300+), with metadata (context, price, modalities) and a reseller filter. Depends on CPE-445/446/447 + a running fetch.

## Acceptance Criteria
- [ ] Model selector is a type-to-filter searchable list, not a short static dropdown; handles 300+ entries smoothly.
- [ ] Each row shows display name, reseller, context length, and price; filter by reseller/modality.
- [ ] Falls back to the last cached catalog offline; a manual Refresh re-fetches.
- [ ] jsdom launcher tests for filtering/rendering; end-to-end needs a GUI smoke-test (noted).

## Notes
needs-prereq: CPE-445/446/447 (+ the fetch path). Fundamentally GUI — verify final behaviour in a real run.
