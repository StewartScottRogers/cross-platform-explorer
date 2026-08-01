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
