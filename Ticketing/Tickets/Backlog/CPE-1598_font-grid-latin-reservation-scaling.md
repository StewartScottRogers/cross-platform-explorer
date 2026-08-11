---
id: CPE-1598
title: "Font glyph grid: scale the Latin reservation instead of a flat 50% tax on CJK fonts"
type: Task
status: Backlog
priority: Low
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Follow-up measured by the independent UAT tester on CPE-1593 (PR #802). The fix for the Latin-dilution
problem works, but its policy is blunt: `RESERVED_LOW_BLOCK_SHARE` reserves a flat **50% of the 200-cell
cap** for Basic Latin + Latin-1, gated only on *whether the font covers those blocks at all* — not on how
much other coverage the font has.

Nearly every real-world CJK UI font fully covers ASCII + Latin-1 (they need it for mixed-script text), so
they pay the full 100-cell tax. Measured on `C:\Windows\Fonts\malgun.ttf` (Malgun Gothic, 27,133 codepoints
of coverage):

| | distinct 256-blocks shown | Latin cells | non-Latin cells |
|---|---|---|---|
| before the reservation | 134 | ~0 | 200 |
| after the reservation | 100 | 101 | 99 |

A 25% drop in distinct blocks, and roughly the first half of the grid is plain Latin before the user
reaches any Hangul. `seguisym.ttf` was barely affected (38 blocks either way — its coverage isn't
block-fragmented the way Hangul/CJK is). `arial.ttf` is the case the reservation exists to protect and is
correct as-is.

UAT judged this **not** a functional break — a user previewing Malgun still immediately sees it is a
Korean font with rich coverage, and it remains dramatically better than the pre-CPE-1593 all-Latin grid —
so it did not block the merge. But it is a real, measured loss of richness worth fixing properly.

## Fix
Scale the reservation by how much non-Latin coverage the font actually has, rather than applying a flat
50%. Shape to consider: reserve proportionally (e.g. Latin gets a share of the budget close to Latin's
share of the font's coverage, floored at enough cells to show the alphabet + digits, and capped so a
Latin-only font still fills the grid). The invariants to preserve, both currently guarded by tests:

- A font covering Basic Latin still shows the full `A–Z`, `a–z`, `0–9` run near the top (the CPE-1593
  acceptance criterion — do not regress Arial).
- A font with no Latin coverage pays nothing (already true).
- No control/whitespace cells (`isDisplayableCodepoint`, already true).

Add a test asserting a CJK-plus-ASCII font gets meaningfully more than half its grid spent on its own
script.

## Notes
Small and self-contained. Conflict surface: `src/lib/preview/font.ts` (`sampleCoverage`,
`RESERVED_LOW_BLOCK_SHARE`), `src/lib/preview/font.test.ts`, and the caption wording in
`src/lib/components/FontPreview.svelte` if the policy description changes. Model: sonnet.
