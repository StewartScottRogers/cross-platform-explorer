---
id: CPE-539
title: "Languages — populate translation catalogs for the offered languages (293 keys each)"
type: Task
status: Open
priority: Medium
component: Frontend
tags: [needs-decision]
epic: CPE-533
estimate: 4h+
created: 2026-07-16
---

## Summary
Wave 2 of [[CPE-533]]: fill in the **actual translations**. The picker now offers 31 languages
([[CPE-538]]), but only en/es/de/fr have full catalogs — the rest fall back to English. Populate the
**293-key** message catalogs for the remaining offered languages.

## Why this is its own ticket (honest scope)
293 keys × ~27 languages ≈ **8,000 translations**. Producing these to a shippable quality is a
**content-sourcing** project, not a code change — accurate translation of every UI string can't be
fabricated by the agent without risking errors across the whole interface.

## Acceptance Criteria
- [ ] A decided **translation source** (professional/community translators, or a vetted machine-
      translation service with human review — **needs-decision**).
- [ ] Complete 293-key catalogs added for the agreed languages, added to `messages`.
- [ ] A coverage indicator so partially-translated languages are honest about their state.
- [ ] The CPE-481 coverage gate extended to the newly-completed locales.

## Notes
**needs-decision:** translation source + quality bar. Deliver incrementally — each completed language
is shippable on its own thanks to the CPE-538 fallback. Do NOT ship machine-guessed translations as
"complete" without a review step.
