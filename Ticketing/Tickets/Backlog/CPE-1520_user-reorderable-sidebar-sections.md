---
id: CPE-1520
title: "User-reorderable left-pane sections (drag to reorder Tags/Explore/Quick Access/Drives/Network/…)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-660
created: 2026-08-09
---
## Why (user, 2026-08-09)
The user wants the left-pane **sections** — Tags, Explore, Quick Access, Drives, Network, and the rest — to be
**reorderable by drag**, so they can arrange the sidebar to taste. Today section **order is fixed in code**: each
section is a literal block in a hardcoded sequence in `Sidebar.svelte`, and the persisted store
(`sidebarSections.ts`, CPE-675) tracks **only collapse state** (an id→boolean open map) — there is no order
concept at all.

## Current state (confirmed)
- Sections (ids): `agents`, `favorites`, `tags`, `smart`, `savedSearch`, `explore`, `places` (Quick Access),
  `drives`, `network`. Rendered top-to-bottom in that fixed markup order.
- `sidebarSections.ts` persists open/collapsed per id (`isOpen`/`toggleSection`), nothing about order.

## Scope
- **Add a persisted order** to the sidebar store (extend `sidebarSections.ts`, or a sibling `sidebarOrder`
  store): an array of section ids. **Default = the current fixed order** (zero behavioural change on upgrade;
  an unset/short/legacy value falls back to the default, and any section id missing from a persisted order is
  appended in default position — so adding a future section can't strand it off-list).
- **Render sections in the persisted order.** Pragmatic, low-churn approach: wrap the existing section blocks in
  a flex column and drive each block's CSS `order` from the persisted array (keep the inline blocks as-is,
  avoid a big extract-to-component refactor). Reconfirm keyboard/focus order still follows visual order
  ([[menu-design-standard]]-adjacent a11y).
- **Drag to reorder:** a drag affordance on each section **header** (a grip, or drag the header row) that
  reorders the array and persists. Match the app's existing dnd conventions (`dnd.ts`); don't conflict with the
  in-section item drag (favorites/quick-access rows already drag). Provide a clear drop indicator between
  section headers.
- **Reset order** control (small "Reset sections" affordance, e.g. in the sidebar's overflow or Settings) so a
  user can get back to default.
- Collapse state (CPE-675) is **orthogonal and preserved** — reordering must not disturb which sections are
  open.

## Verify
- Pure reducer unit tests (jsdom): default order when unset; reorder move (up/down/to-ends) produces the right
  array; a persisted order missing a new id appends it; malformed/legacy JSON falls back to default; collapse
  state survives a reorder.
- **Attended/visual (or gui-smoke Visual Critic):** drag a section header, order persists across restart, drop
  indicator reads clearly, reset works, no jank with a collapsed section mid-drag.
- Docs (`src/docs/03-explorer.md` sidebar section): document drag-to-reorder + reset.

## Notes
Frontend-only, testable without hardware. Pairs with **CPE-1516** (Network must first be a permanent section to
be reorderable — its default slot goes in this ticket's default order). Epic CPE-660 (sidebar). Relates to
[[prefer-inline-instant-controls]]. Consider whether the AI-console/board sidebars share the pattern (they may
not — scope to the main explorer sidebar unless trivially shared).
