---
id: CPE-675
title: Collapsible core sidebar sections
type: feature
component: Frontend
priority: medium
status: In Progress
tags: ready
created: 2026-07-18
epic: CPE-660
estimate: 3-4h
---

## Summary
The whole of CPE-660. Give the three core sidebar groups — pinned nav ("Explore"), quick-access
`places` ("Quick access"), and `drives` ("Drives") — a header row with a collapse twisty matching the
existing Favorites/Agents pattern, and persist every section's open/closed state via a new shared
`src/lib/sidebarSections.ts` store (localStorage). Convert the existing transient twisties (Favorites,
Agents, Tags, Smart) to the same store so all sections behave + persist identically. Default all-expanded.

## Acceptance Criteria
- [ ] The three core groups each have a header + working collapse twisty; dividers kept.
- [ ] `sidebarSections.ts` persists per-section open state (default open); pure reducer unit-tested.
- [ ] Existing Favorites/Agents/Tags/Smart twisties use the same persisted store (consistent + persist).
- [ ] Section labels added to all 12 locales; headers are keyboard-focusable with `aria-expanded`, themed.
- [ ] Per-node drive/folder tree expansion still works independently; `npm run check` + suite green.

## Work Log
2026-07-18 (nightshift) — Picked up as the sole CPE-660 child. No questions; best-guess. Estimate 3-4h.
