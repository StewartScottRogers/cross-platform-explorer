---
id: CPE-494
title: "Pill/chip rows ('tick-tacks') must reflow, not wrap-and-overflow"
type: Defect
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-16
closed: 2026-07-16
---

## Summary
When a pane is narrow, pill/chip/button rows ("tick-tacks") wrapped their **text inside the pill**,
overflowing the rounded background instead of reflowing. Reported with a screenshot of the
**ContextBar** ("Open on GitHub / Open package.json / …" action buttons wrapping + overflowing). The
row should grow taller and **reflow the pills**, each pill keeping its text on one line.

## Acceptance Criteria
- [x] The ContextBar chips + action buttons keep their text on one line and reflow onto new rows when
      narrow (no text overflowing a pill).
- [x] The same rule is applied to every pill/chip/badge row (correct it everywhere).
- [x] Documented as a standing rule (CLAUDE.md UI conventions + memory).

## Resolution
The rule: **container** `display:flex; flex-wrap:wrap; gap` + auto height; **each pill**
`white-space:nowrap; flex:0 0 auto`. Applied to:
- **`ContextBar`** — `.ctx-actions` gained `flex-wrap:wrap`; `.ctx-action` + `.ctx-chip` gained
  `white-space:nowrap; flex:0 0 auto` (the reported bug).
- **Global `.pills`/`.pill`** (`src/app.css`, used by HomeView's Recent/Favorites/Folders tabs) —
  `.pills` gained `flex-wrap:wrap`; `.pill` gained `nowrap; flex:0 0 auto`.
- **`SidecarManager`** `.cap` capability chips — `nowrap; flex:0 0 auto` (`.caps` already wraps).
- **`ConsentSheet`** — `.badge` `nowrap`; `.cap-label` `flex-wrap:wrap` so a label + its badges wrap.

The Agent Watch strip chips + session chips already prevented internal wrap (ellipsis / single char),
so they were left as-is. Documented in `CLAUDE.md` "UI conventions" and saved to memory
(`tick-tacks-reflow`). `npm run check` clean. Files: `src/lib/components/{ContextBar,SidecarManager,
ConsentSheet}.svelte`, `src/app.css`, `CLAUDE.md`.
