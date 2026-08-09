---
id: CPE-1321
title: File-Health mismatch subtitle clipped by the results horizontal scrollbar
type: bug
component: Frontend
priority: medium
tags: ready
created: 2026-08-05
epic: CPE-1002
estimate: 1h
---

## Summary
CPE-1319 fixed the mismatch-tab filename truncation by moving the reason to a subtitle line under the name,
but the Visual Critic (on a real build) found the subtitle now OVERLAPS the results list's horizontal
scrollbar and is vertically CLIPPED — only the top half of the subtitle glyphs render (e.g. "claims ing" /
"ave…able" instead of the full "claims jpg → looks like Windows executable/library"). The filename is legible;
the reason text is not, because the taller stacked row's bottom is cut off by the scrollbar.

## Acceptance Criteria
- [ ] The mismatch row's subtitle renders FULLY (not clipped) even when the results list shows a horizontal
      scrollbar. Investigate the CSS on the results/rows scroll container (where `overflow-x` is set) and the
      `.row-wide`/`.subtitle` height in `src/lib/components/FileHealthDialog.svelte`: likely fixes are giving
      the container `padding-bottom` so the last row clears the scrollbar, letting rows size to content
      (no clipped fixed height), and/or preventing the scrollbar from painting over row content. Verify the
      subtitle's full text is visible with a long reason AND when a horizontal scrollbar is present.
- [ ] No regression to the other tabs (dangling/orphan/empty) or to the compact single-line rows.
- [ ] `npm run check` clean + full `npm run test:unit` green (add/adjust a jsdom structural assertion if
      useful, though the clipping itself is visual — the Foreman re-screenshots + re-runs the Visual Critic).

## Work Log
2026-08-05 (sprint run 2) — Filed by the Foreman from the Visual Critic's re-judge of the CPE-1319 fix
(orphan pill = good; mismatch subtitle clipped by the horizontal scrollbar). Targeted CSS fix.
