---
id: CPE-1488
title: "EPIC: Compact / dense view mode — TUI-grade information density"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (workshift PM, competitive-landscape pass — TUI survey).** Dormant brief — decompose on
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
