---
id: CPE-1100
title: "Agent Watch: conflict/activity-overlap radar panel"
type: feature
component: Frontend
priority: low
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-396
depends-on: CPE-1099, CPE-1101
---

## Summary
GUI #3, conflict-radar slice B (the panel). Over the multi-session, actor-tagged activity stream from
CPE-1099, render a **radar** that surfaces paths touched by two different actors within a short window — the
classic "two agents (or agent vs. user) editing the same file" signal. Frontend only. Context:
`.claude/research-library/entries/agent-watch-dashboards-substrate.md`.

## Design (buildable)
1. **Overlap fold** — a store/derivation over the actor-tagged activity: group writes by path; when the same
   path is written by ≥2 distinct actors (or an agent write races a "user" op) within a window (e.g. a few
   seconds), emit an overlap entry `{ path, actors[], lastAt }`. Conceptually similar to the sidecar's
   `swarm_locks.rs::claims_overlap` (not reusable as-is — that's glob-claim-shaped, sidecar-internal).
2. **Radar panel** — a tab/section in the Agent Watch dashboard drawer (reuse the tabbed host from CPE-1094;
   TABS.md). List overlaps most-recent first: the path, the actors involved (as reflowing pills — tick-tacks),
   and a timestamp; clicking navigates to the path. Theme vars only.
3. **Hedge the language until attribution is trustworthy** — if actor tagging is coarse (many "unknown
   actor"), label the panel **"activity overlap"** rather than "conflict" so it doesn't over-claim. Show a
   small note when overlaps involve an "unknown" actor.

## ⚠ Notes / guardrails
- Pure store logic + a component. No new deps. Theme vars only; pills reflow; NaN/empty-safe. Listener only
  inside the watch `if (cwd)` gate; removed on stop/destroy (off-means-off; no leak).
- **Do not over-claim**: a "conflict" that's really just "two things touched this path" (including the user's
  own actions) is misleading — hedge to "activity overlap" until CPE-1099's actor tags are trustworthy.

## Acceptance Criteria
- [ ] When two distinct actors touch the same path within the window, the radar shows an overlap entry with
      the path + involved actors + time; clicking navigates there.
- [ ] Language hedges to "activity overlap" when actors include "unknown"; empty state is clean; zero cost when
      not watching.
- [ ] Colours from theme vars; pills reflow; `npm run check` clean; vitest green (overlap-fold + window +
      empty/degenerate cases tested); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed as GUI #3 conflict-radar panel on top of CPE-1099's multi-session +
actor-tag enablement. From the Library substrate brief. Ships LAST of the GUI #3 slices; honesty-hedged until
actor attribution is solid.
