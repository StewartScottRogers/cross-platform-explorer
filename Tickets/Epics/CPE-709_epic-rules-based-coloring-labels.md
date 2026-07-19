---
id: CPE-709
title: "EPIC: Rules-based file coloring & labels"
type: Task
status: Proposed
priority: Low
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
User-defined visual rules that tint rows/icons and attach coloured labels by criteria (extension, age,
size, read-only, name pattern, tag), so a folder is scannable at a glance.

## Why
Complements the existing tag system with automatic, condition-driven styling that Total Commander / Finder
power users rely on. Reuses the smart-folder predicate model rather than inventing a new matcher.

## Rough scope (areas, not child tickets)
- A rule-evaluation engine reusing the smart-folder predicate model (condition -> style).
- A rules editor UI (ordered rules, first-match or layered) with live preview.
- Themed styling hooks in the listing renderer (row tint, icon overlay, label chip) — light/dark parity.
- Persistence of rule sets in settings.

## Open questions (resolve at activation)
- Rule precedence model (first-match vs. cascade) and interaction with selection/hover styling.
- Performance with many rules across virtualized rows.
- Scope: global rules vs. per-folder rules.

## Definition of Done
- Users define rules that colour rows/icons and add labels by file criteria, with a live preview.
- Styling is theme-variable driven (identical light/dark) and applied within the virtualized list.
- Rules persist and add no measurable per-row cost when none are defined.
