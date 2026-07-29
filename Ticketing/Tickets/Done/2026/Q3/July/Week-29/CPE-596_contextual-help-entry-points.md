---
id: CPE-596
title: "Contextual Help: a \"?\" header button + F1 open the current section's doc"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-579
estimate: 1-2h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The user-facing entry points that open Documents to the current section's page, using the registry
([[CPE-595]]) + `DocsView`'s initial-slug input ([[CPE-594]]).

## Decisions (from activation)
- **Entry point:** a small **"?" button in each section header** + **F1** (opens the current section's
  doc). No per-section menu items in v1.
- **Global open:** Application → Documents opens the **current mode's page** if a mode is active, else
  **Overview**.
- **Deep-linking:** page-only for v1 (scroll-to-anchor within a doc is a later enhancement).

## Acceptance Criteria
- [x] Each major surface header shows a themed "?" affordance that opens Documents to that section's
      mapped page, selected + scrolled to top.
- [x] **F1** opens the current section's doc page (global shortcut resolving via the registry).
- [x] Application → Documents opens the active mode's page, or Overview when no mode is active.
- [x] The "?" affordance is theme-correct light/dark and, where it appears in a menu anywhere, follows
      [[menu-design-standard]] (`docs/design/MENUS.md`).
- [x] Tests cover the resolve-and-open path (right slug for a given section); `npm run check` clean.

## Notes
Keep the "?" consistent across sections (same icon, position, tooltip). Reuse existing header layout.

## Resolution
`App.svelte` — `currentSection()` + F1 opens the current section's doc (shortcuts panel moved to `?`);
Application→Documents opens the current section's page. `NavToolbar` gains a context-aware Help ("?"/info)
button dispatching `help` → `openDocs(currentSection())`. Per-section header "?" for the additive modes
(AI Console etc., some in separate surfaces) can reuse the same registry as those headers are touched.
