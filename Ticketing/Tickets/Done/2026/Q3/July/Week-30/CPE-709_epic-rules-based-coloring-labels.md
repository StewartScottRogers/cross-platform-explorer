---
id: CPE-709
title: "EPIC: Rules-based file coloring & labels"
type: Task
status: Done
priority: Low
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-21
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

## Work Log
2026-07-20 (nightshift, 00:20 MST) — Activated. Open questions resolved (autonomous best-guess): **first-
match precedence** (the first enabled matching rule supplies the row's style; simplest + predictable);
rule styling sits *under* selection/hover (theme-var tint, like the existing tag label accent); **global
rule set** persisted in settings (per-folder deferred). Reuse note: smartFolders is tag-based, not a
file-criteria predicate matcher, so the engine defines its own small condition model (reusing `matchesGlob`
for name patterns) rather than forcing a fit.

## Child tickets
1. **CPE-774** — Pure rule-evaluation engine (`src/lib/colorRules.ts`): a condition model (extension /
   name-glob / size / age / is-dir) + `evaluateRules(entry, rules, now)` → first-match `{color?, label?}`.
   Unit-tested. **Foundation, headless.**
2. **CPE-775** — Apply the resolved style in the FileList renderer: row tint + label chip from the engine,
   reusing the CPE-638 label rendering; theme-variable driven (light/dark parity); zero per-row cost when
   no rules. **GUI.** *(prereq: 774)*
3. **CPE-776** — Rules editor UI (ordered rules, enable/disable, live preview) + persistence in settings.
   **GUI.** *(prereq: 774, 775)*

## Resolution (closed 2026-07-21)
All child tickets are **Done** — the epic's Definition of Done is delivered by the rule-evaluation engine (CPE-774), themed renderer hooks (775), the rules editor + persistence (776), and hardened condition parsing (808). Closed as part of the
epic-queue tidy-up: every planned child shipped, no remaining scope. Feature verification lives in each
child's Resolution.
