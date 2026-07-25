---
id: CPE-936
title: Rewrite swarm demos for usability + live console narration
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Rewrote all 7 swarm demos for **usability** (clearer, self-contained task descriptions with concrete
deliverables) and **liveness** (each builder narrates its terminal as a watchable show): a
`=== <file>: starting ===` banner, a `→ ` line for every step + what it found, and a
`✓ <file>: done — <result>` summary. A `{F}` placeholder is substituted with each task's own filename at
load time, so the narration names its own file. All demos remain safe (create files only).

Note: the live swarms run inside the Agent Deck using the user's configured agent — they can't be driven
from CI; instead all 7 demos are **structurally verified** (correct task counts, disjoint `.md` scopes,
`{F}` fully substituted, banner/step/summary markers present).

## Acceptance Criteria
- [x] Every demo has clearer tasks + a banner/steps/summary live-narration pattern naming its own file.
- [x] All 7 verified well-formed (counts, disjoint scopes, no leftover {F}). Full suite green (929).

## Work Log
- 2026-07-23 — LIVE template + {F} per-task substitution in loadSelectedDemo; browser-verified narration.
