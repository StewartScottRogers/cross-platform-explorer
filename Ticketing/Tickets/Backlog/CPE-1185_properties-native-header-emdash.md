---
id: CPE-1185
title: "Polish: symmetric em-dash spacing in PropertiesDialog native-metadata header"
type: chore
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-717
---

## Summary
Visual-Critic follow-up from CPE-1176 (epic CPE-717). The Properties "Native metadata" section header renders
"Native metadata**—** cpe.tags" with asymmetric em-dash spacing (space after, none before). Convention is a
space on both sides. One-line cosmetic fix.

## Build
- In `src/lib/components/PropertiesDialog.svelte`, the native-section header string: change `metadata—
  {name}` to `metadata — {name}` (space before AND after the em dash). Verify the whole header still fits.

## Acceptance Criteria
- [ ] Header shows "Native metadata — cpe.tags" with symmetric spacing; `npm run check` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift) from the CPE-717 Visual Critic non-blocking note. Pick up in a
  polish pass / batch with nearby frontend work.
