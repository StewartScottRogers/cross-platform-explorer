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

## Deferred 2026-08-01 (workshift) — needs basename-DIRECT matching, not position-filtering
PR #529 tried the "drop match positions before the last separator" approach (option 2). It removes
the scattered prefix highlights (the main complaint) BUT the spotlight gui-smoke caught a real wart:
because ranking still fuzzy-matches over the FULL path greedily, when the path prefix contains a
query character the matcher consumes it there — e.g. query "marker" against ".../Temp/CPE-1045-marker.txt"
matches the 'm' in "Temp", so the basename positions are only "arker", and the filtered highlight
renders "arker" (leading 'm' unhighlighted) — a partial, buggy-looking highlight. The smoke's
`markTexts.join("").to.include("marker")` assertion fails on this.

**Correct fix for next pickup:** compute the highlight positions by matching the query against the
BASENAME directly (a small greedy subsequence match in the frontend, or have the backend return
basename-relative positions), so the full query highlights cleanly within the basename regardless of
what the path prefix contains. Keep ranking/ordering over the full path unchanged. Then the spotlight
smoke passes as-is and the highlight is complete. PR #529 closed unmerged; its `basenamePositions`
filter can be a starting reference but is not the final approach.

Low priority (a nit-of-a-nit), so parked rather than ground on mid-shift.
