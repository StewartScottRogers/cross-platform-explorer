---
id: CPE-763
title: Documents dialog — expandable left-pane sections + per-section contextual help from anywhere
type: feature
component: Frontend
tags: ready
created: 2026-07-19
closed: 2026-07-19
status: Done
priority: medium
estimate: 3-4h
---

## Summary
Prepare the in-app Documents library (CPE-536/537) for much more extensive documentation and make every
section's docs reachable directly from where that section lives:

1. **Expandable sections in the left pane.** The flat TOC now groups doc pages into **collapsible
   categories** (Getting started / Explorer / AI Console / Development), so the list scales to many more
   pages without becoming one long scroll. Searching force-expands every group with a match.
2. **Open into any section from anywhere.** Deep-linking already resolved a section → doc slug
   (CPE-595/596); now opening on a slug also **expands that doc's category and scrolls it into view**, so
   "open into any section" actually lands you on the page.
3. **Per-section direct access, not just the toolbar/menu.** A reusable `HelpButton` (the book glyph)
   sits in each section's own header and opens the docs viewer straight to that section's page.

## Scope / changes
- `src/lib/docs.ts` — `Doc` gains `category` + `categoryOrder` (frontmatter, defaults General/999); new
  pure `groupDocs()` builds ordered `DocCategory[]`. Unit-tested.
- `src/docs/*.md` — every page tagged with its `category`/`categoryOrder`; **new `11-disk-usage.md`**
  (the Space analyzer page) as the worked example of a brand-new documented section.
- `src/lib/sectionDocs.ts` — new `disk-usage` section → `11-disk-usage` (guard test auto-covers it).
- `src/lib/components/DocsView.svelte` — collapsible category groups (chevron header + count), default
  open, search-expands-all, deep-link scroll-into-view.
- `src/lib/components/HelpButton.svelte` — **new** reusable "?" affordance; `section` prop, dispatches
  `help` with the section id; parent forwards to `openDocs()`.
- `HelpButton` wired into `DiskSpaceView` (disk-usage), `WorkbenchView` (workbench), `BoardView`
  (agent-board) headers; `App.svelte` forwards each `on:help` to `openDocs(e.detail)`.
- Tests: `docs.test.ts` updated for the new `parseDoc` shape + `groupDocs` coverage.

## Follow-up (same one-line pattern)
The sidecar-side surfaces (AI Console, Agent Grid, Repositories, Swarms) live in the separate AI Console
UI; each just needs `<HelpButton section=… on:help />` dropped into its header + an `on:help` forward,
exactly as done here. Noted, not in this ticket.

## Acceptance
- [x] Left pane groups docs into expandable/collapsible categories
- [x] Categories default open; search force-expands groups with matches
- [x] Opening on a section deep-link expands its category + scrolls the item into view
- [x] Reusable `HelpButton` opens the docs viewer to a given section from that section's header
- [x] Wired into Disk-usage / Workbench / Agent-board headers (+ new disk-usage section & doc page)
- [x] `sectionDocs` guard test covers the new section; `npm run check` 0/0; `npx vitest run` green (742)

## Resolution (closed 2026-07-19)
Added a category layer to the docs model (`groupDocs`, pure + tested), tagged all pages with categories,
made `DocsView`'s left pane a set of collapsible sections with deep-link scroll, and introduced a
reusable `HelpButton` now living in the Disk-usage, Workbench, and Agent-board headers (each forwarding
to `openDocs`). Shipped a new `disk-usage` section + `11-disk-usage.md` page as the exemplar. Verified:
svelte-check 0/0, 742 frontend tests pass. Landed on `main`.

## Work Log
- 2026-07-19 — Picked up (user request). Estimate 3-4h. Surveyed the docs system (docs.ts / DocsView /
  sectionDocs) and the section-view headers (NavToolbar/Workbench/Board/DiskSpace).
- 2026-07-19 — Built grouping + collapsible UI + HelpButton + new disk-usage doc/section. check 0/0,
  742 tests pass. Closed and merged to main.
