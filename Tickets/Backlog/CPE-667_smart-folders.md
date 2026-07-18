---
id: CPE-667
title: Smart folders — a saved, auto-updating tag/attribute query
type: feature
component: Multiple
priority: medium
status: Open
tags: ready
created: 2026-07-18
epic: CPE-614
estimate: 3-4h
---

## Summary
Completes the unmet CPE-614 DoD gate: "at least one smart-folder query type works end-to-end and updates
as files change." The tags half of CPE-614 shipped (15 children), but no **smart folder** — a saved query
surfaced as a virtual folder that re-evaluates live — was ever built. Add one query type end-to-end:
a saved **tag query** (e.g. "everything tagged `invoice`") that appears in the sidebar and lists matching
files across the tagged set, updating as tags change.

## Acceptance Criteria
- [ ] A saved smart folder (name + tag query) persists across restart (reuses the tag store / persist layer).
- [ ] Selecting it lists all files matching the query, re-evaluating as tags are added/removed.
- [ ] Surfaced in the sidebar alongside Tags; create/rename/delete via the standard menu convention.
- [ ] Core query/matching logic is headlessly unit-tested; zero weight when no smart folder is open.
- [ ] `npm run check` + suite green; docs updated (03-explorer.md + sectionDocs if a new section).

## Notes
Uncovered during nightshift epic housekeeping (2026-07-18): CPE-614 was In Progress with all 15 tag
children Done but this DoD gate open. Filing it keeps the epic honest; CPE-614 closes when this lands.
Design open question for whoever works it: scope v1 to tag-only queries, or generalise to attribute
predicates (size/type/date)? Recommend tag-only for v1, generalise later.
