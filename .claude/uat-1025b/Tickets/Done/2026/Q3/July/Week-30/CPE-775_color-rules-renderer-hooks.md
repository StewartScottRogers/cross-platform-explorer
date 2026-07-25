---
id: CPE-775
title: Apply rule-based row tint + label chip in the FileList renderer
type: feature
status: Done
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-709
estimate: 2-3h
---

## Summary
Wire the CPE-774 rule engine into the listing (epic CPE-709): each row computes its `{color?, label?}` via
`evaluateRules` and renders a themed row tint + optional label chip, reusing the CPE-638 label rendering.
Theme-variable driven (identical light/dark); zero per-row cost when no rules are defined.

## Acceptance Criteria
- [x] Rows matching a rule get the rule's tint (under selection/hover) and label chip; light/dark parity.
- [x] No measurable per-row cost when the rule set is empty (early-out); works within the virtualized list.
- [x] `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-774. Attended GUI. Reuse the existing `.row.tagged` / label-accent styling from CPE-638.

## Work Log
- 2026-07-20 — Picked up. Estimate 2-3h. Grepped first: the rule engine (`evaluateRules`, CPE-774) was
  already **partially** wired into `FileList.svelte` by the CPE-776 editor work — a matched row already
  recolored the **filename text** + rendered a `.rule-label` chip. The gap vs this ticket's AC1 was the
  **row tint** (a background wash on the whole row, layered under selection/hover), which didn't exist.
- 2026-07-20 — Added the row tint mirroring the CPE-638 `.tagged` pattern: the row gets
  `class:rule-tinted` + a `--rule-color` custom prop from `ruleStyle.color`; CSS paints
  `background: color-mix(in srgb, var(--rule-color) 14%, transparent)` **only** on a resting row
  (`:not(.selected):not(:hover):not(.agent-active):not(.agent-inside)`) so selection/hover/Agent-Watch keep
  their own opaque background, plus a thin left accent bar (`box-shadow inset 3px`) that stays visible even
  while selected — yielding to a tag label bar and to Agent Watch. The wash is `color-mix` with transparent,
  so it's identical in light/dark. Early-out preserved (`colorRules.length ? evaluateRules(...) : {}` →
  `class:rule-tinted={!!ruleStyle.color}`), so an empty rule set is byte-for-byte the old row.
- 2026-07-20 — 2 component tests (`FileList.test.ts`): a matching `.png` row is the only `.rule-tinted`,
  carries `--rule-color`, and shows its label chip; an empty rule set tints nothing. `npm run check` clean;
  frontend suite **886 green**.

## Resolution
Wired the CPE-774 rule engine's tint into the `FileList` row renderer (the label chip + text color were
already wired by CPE-776). A matched row gets `class:rule-tinted` + a `--rule-color` var; new CSS washes the
resting row with `color-mix(var(--rule-color) 14%, transparent)` and adds a thin left accent bar, both
layered **under** selection/hover (they paint opaque over the wash) and never masking an Agent-Watch row.
Theme-independent by construction (translucent wash → light/dark parity); zero cost when no rules match.
Files: `src/lib/components/FileList.svelte` (row class/var + CSS), `src/lib/components/FileList.test.ts`
(+2 tests).

**GUI-verified in the sidecar dev build (CDP):** seeded a rule (ext `md` → `#e0483b`, label "Doc"), navigated
to the repo folder → all **5 `.md` rows** rendered `rule-tinted` with computed
`background: color(srgb 0.878 0.282 0.231 / 0.14)` (the `color-mix` actually painted), a left accent bar, and
a "Doc" chip; the other 25 rows stayed plain. Flipping `data-theme=dark` kept the identical translucent tint
(parity), and marking a row `.selected` computed to the **opaque selection colour** — confirming the tint
sits under selection. All three ACs met. CPE-775 → Done.
