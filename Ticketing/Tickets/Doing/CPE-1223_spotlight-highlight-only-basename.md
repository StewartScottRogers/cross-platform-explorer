---
id: CPE-1223
title: "Spotlight: fuzzy match highlights stray chars in the path prefix (consider basename-scoped highlight)"
type: Task
priority: Low
component: frontend
tags: [ready]
estimate: 45m
created: 2026-08-01
closed:
---

## Context
Spotlight fuzzy-matches + highlights over the FULL path (`fileSource` uses `h.path`), so the greedy
subsequence matcher can highlight a stray character in the path prefix before the real filename match
— e.g. querying "marker" faintly highlights the "m" in ".../Temp/..." ahead of the "marker" run in
the filename. Surfaced by the epic-704 / CPE-1220 Visual Critic pass (now slightly more visible since
CPE-1220 made highlights bolder). Cosmetic, pre-existing, not a CPE-1220 regression.

## Options to weigh
- Score/highlight against the basename (or rank by basename, show path dimmed + unhighlighted).
- Keep full-path matching but only render highlight runs that fall within the basename.
- Leave as-is (fuzzy launchers commonly highlight across the whole string).

## Acceptance criteria
- A typed query no longer scatters lone highlighted characters across the path prefix; the highlight
  reads as intentional (matched run in the meaningful part of the entry).
- Re-capture the spotlight gui-smoke screenshot; no regression to multi-run highlighting.

## Work Log
- Chose option 2 ("keep full-path matching but only render highlight runs that fall within the
  basename") — least invasive, doesn't touch ranking. Added `basenamePositions(text, positions)` to
  `src/lib/spotlightSources.ts`: finds the last path separator (`/` or `\`) in `text` and drops any
  match position before it, leaving positions at/after the basename start untouched. A no-op for text
  with no separator (action labels). `Spotlight.svelte` now calls
  `highlightByPositions(row.text, basenamePositions(row.text, row.positions))` when rendering each row's
  `<mark>` runs — matching/ranking (`spotlight_search`) is untouched; only which characters get
  highlighted changes.
- Added unit tests: `spotlightSources.test.ts` (`basenamePositions` — drops prefix hits, keeps
  in-basename runs, no-op for no separator/empty positions, handles both `/` and `\`) and
  `Spotlight.test.ts` (full-row render: a stray path-prefix hit is suppressed while the real in-filename
  run still highlights; a genuine non-contiguous multi-run match within the filename — mirrors
  spotlight.rs's own "rme" → "readme.md" doc example — still highlights both runs).
- `npm run check`: 0 errors. `npm test`: 148 files / 1652 tests passed. `gui-smoke && npm run
  typecheck`: clean (after `npm install` in gui-smoke, which had no node_modules in this worktree).
