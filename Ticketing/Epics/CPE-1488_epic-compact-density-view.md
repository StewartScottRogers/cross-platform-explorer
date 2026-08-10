---
id: CPE-1488
title: "EPIC: Compact / dense view mode — TUI-grade information density"
type: Task
status: In Progress
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, competitive-landscape pass — TUI survey).** Dormant brief — decompose on
> `/ticketing-epic activate CPE-1488`.

## Why (the other real TUI strength CPE has no answer for)
TUIs pack far more rows per screen than CPE's fixed row pitch + toolbar chrome. "Minimal chrome / information
density" was a clear TUI win with **no CPE equivalent** (no density/compact/row-height toggle exists). Power
users who want to "see more, scan faster" — the reason people love a terminal listing — have nothing today.
This directly serves PURPOSE.md's fast/small/predictable tiebreaker (it's a *density* feature, not bloat).

## Goal
A **Compact** density toggle: tighter row pitch, smaller/optional icons, collapsible toolbar labels, thinner
tab strip — maximizing rows-per-screen — switchable on a dime, persisted in settings. (Comfortable stays the
default.)

## Rough slices (JIT)
- A density setting (`comfortable` | `compact`, maybe `cozy`) in the settings model.
- A compact row-height variant in `FileList.svelte` (virtualization from CPE-688 already means only visible
  rows render, so this is cheap — no perf cost).
- Chrome density: collapsible/short toolbar, thinner tabbar/sidebar, honoring the TABS/MENUS standards.
- Instant inline toggle (per [[prefer-inline-instant-controls]]) — a view-mode control, changeable on a dime,
  not a modal.
- Docs page per CPE-579.

## Notes
Pure frontend/CSS + a settings flag; no backend, no new deps. Small-medium. Reuse theme variables (light-theme
palette). The single most on-purpose candidate from the survey — it *is* fast/small/predictable made visible.

## Activated 2026-08-09 (sprint PM, lights-out bench refill)
Picked over CPE-1484/1487/1489/661/688 as the highest-impact cleanly-headless slice available this
sprint (see PM rationale in the sprint log / commit message). Decomposed into 4 independent-as-possible
tickets, sequenced by a single foundation ticket then two parallel consumers then the user-facing toggle:

## Child tickets
1. **CPE-1526** — Density setting: settings.ts model (`density: "comfortable"|"compact"`) + App.svelte
   wiring seam (prop threading only, no visible change). *(foundation, build first)*
2. **CPE-1527** — Compact row/tile pitch in `FileList.svelte` (details/icons/gallery), preserving the
   CPE-690/766 fixed-height virtualization invariant. *(prereq: 1526; parallel with 1528)*
3. **CPE-1528** — Chrome density: thinner `NavToolbar`/`TabBar`/`Sidebar` (honors TABS.md). *(prereq:
   1526; parallel with 1527)*
4. **CPE-1529** — Instant density toggle control (in `NavToolbar.svelte`) + docs update
   (`src/docs/03-explorer.md`). *(prereq: 1526; sequence after 1528 — both touch NavToolbar.svelte)*
