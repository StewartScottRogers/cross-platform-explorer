---
id: CPE-1528
title: "Compact density: thinner toolbar / tab strip / sidebar chrome"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1488
created: 2026-08-09
---
## Context
CPE-1488's density toggle isn't just the row list — it also covers "chrome": a collapsible/shorter
toolbar, thinner tabbar/sidebar, so the whole window gains screen real estate for content when compact
is on. This ticket is the chrome half, sibling to CPE-1527's file-list-row half; the two touch entirely
different files so they can be built in parallel.

## Scope
- **Toolbar** (`NavToolbar.svelte`): tighter control padding when compact; collapsible/hidden text
  labels on buttons that have icons (icon-only in compact, icon+label in comfortable) — buttons remain
  reachable/labelled via `title`/aria for accessibility.
- **Tab strip** (`TabBar.svelte`): a thinner tab strip in compact, **while still honoring the TABS.md
  standard** — the accent top-bar + content-surface active-tab treatment and recessed-chip inactive
  tabs stay intact at the smaller pitch; don't invent a new tab treatment for compact, just shrink the
  existing one. See [docs/design/TABS.md](../../docs/design/TABS.md).
- **Sidebar** (`Sidebar.svelte`): tighter row padding/spacing for places/pins/drives in compact.
- All variants driven by the `density` prop from CPE-1526 (already threaded in) plus theme CSS variables
  only — no hard-coded colors, no new deps, light-theme only (no dark theme exists in this repo yet).
- Comfortable (default) stays pixel-identical to today.

## How
- Consume the `density` prop CPE-1526 already threads into these three components — do not re-add
  settings plumbing here.
- Prefer a `density`-driven CSS class (e.g. `class:compact` on the root element of each component) over
  duplicating markup, so the two visual states share one DOM structure.

## Verify
`npm run check` + `npx vitest run` covering `src/lib/components/NavToolbar.test.ts`,
`src/lib/components/Sidebar.test.ts` (add a `TabBar.test.ts` case if that file has test coverage
already, otherwise a small new one) — assert the compact class/attribute is applied when
`density === "compact"` and absent otherwise. Fully headless. Visual sign-off (does it actually look
right at a glance) can be queued as a later gui-smoke screenshot pass — implementation + unit coverage
must not block on that.

## Notes
**Conflict surface:** `src/lib/components/NavToolbar.svelte`, `src/lib/components/TabBar.svelte`,
`src/lib/components/Sidebar.svelte` + their `.test.ts` files. **Prereq: CPE-1526.** Disjoint from
CPE-1527 (FileList.svelte) — safe to run in parallel with it. **Overlaps CPE-1529 on
`NavToolbar.svelte`** (CPE-1529 adds the density toggle control itself into that same file) — the
Foreman should sequence CPE-1529 *after* this one lands rather than dispatching them concurrently, to
avoid a merge collision on that file.
