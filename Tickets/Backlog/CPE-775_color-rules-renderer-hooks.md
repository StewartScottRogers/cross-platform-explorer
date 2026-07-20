---
id: CPE-775
title: Apply rule-based row tint + label chip in the FileList renderer
type: feature
status: Open
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-709
estimate: 2-3h
---

## Summary
Wire the CPE-774 rule engine into the listing (epic CPE-709): each row computes its `{color?, label?}` via
`evaluateRules` and renders a themed row tint + optional label chip, reusing the CPE-638 label rendering.
Theme-variable driven (identical light/dark); zero per-row cost when no rules are defined.

## Acceptance Criteria
- [ ] Rows matching a rule get the rule's tint (under selection/hover) and label chip; light/dark parity.
- [ ] No measurable per-row cost when the rule set is empty (early-out); works within the virtualized list.
- [ ] `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-774. Attended GUI. Reuse the existing `.row.tagged` / label-accent styling from CPE-638.
