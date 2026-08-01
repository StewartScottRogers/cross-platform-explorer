---
id: CPE-1233
title: "QA: gui-smoke pin + Visual Critic screenshot for the Saved Searches flow (CPE-1229)"
type: Task
priority: Medium
component: gui-smoke
tags: [ready]
estimate: 1h
created: 2026-08-01
epic: CPE-978
closed:
---

## Context
CPE-1229 (PR #532) shipped the structured Saved Searches UI: a "Save search…" affordance in
SelectByDialog + palette, a new sidebar "Saved Searches" section, and an open-evaluator. It has
Reviewer + UAT sign-off but no gui-smoke screenshot for the Visual Critic yet (marquee epic-978 slice).

## Repro to drive (from the CPE-1229 worker's own notes)
Open a real folder → Ctrl+Shift+P → "Save search…" → pick a criterion (e.g. Extension = png) → type a
name → click Save search. The sidebar's "Saved Searches" section appears below Smart Folders; clicking
the entry shows the filtered recursive view.

## Acceptance criteria
- New `gui-smoke/specs/saved-search.smoke.ts` (mirror similar-images/near-duplicates specs) drives the
  real built app through the repro above against the seeded tmpDir (which already has CPE-1203 .png
  fixtures), asserts the "Saved Searches" sidebar section renders the saved entry, opens it, asserts a
  seeded .png shows and a non-matching file doesn't, and `snap("saved-search")`s the sidebar + result.
- Spec passes green + captures `saved-search.png`. Visual Critic judges the sidebar section + the
  SelectByDialog "Save search…" inline capture (section styling matches Smart Folders, menu conventions,
  dialog border, on-theme, no clipping).

## Notes
QA-Architect burndown item for the new surface; mirrors CPE-1221 (near-dup pin).
