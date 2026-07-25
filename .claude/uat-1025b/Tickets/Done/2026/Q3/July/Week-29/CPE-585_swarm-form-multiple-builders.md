---
id: CPE-585
title: "AI Console: Run swarm form accepts multiple tasks → one builder each (parallel coordination)"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 1h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The "Run swarm" form only accepted a single task and staffed one builder, so it couldn't exercise
real multi-agent coordination. Let it accept **multiple tasks (one per line)**, staffing **one builder
per task** so they run concurrently (the coordinator serializes any whose scopes overlap, CPE-517).

## Changes
- `launcher.html` — the Swarm-task `<input>` becomes a `<textarea>` (one task per line); added a
  `textarea` style. `runSwarmFromForm` builds N tasks and sets the builder role `count` to N.
- New `parseSwarmTasks(text)` helper: one task per line, ignoring blanks, with an optional
  `glob1,glob2 :: task` scope (disjoint scopes ⇒ parallel; omitted ⇒ `**`).
- No backend change: the coordinator/driver already staff multiple builders + tasks (`team(2)` tests).

## Acceptance Criteria
- [x] Form accepts multiple task lines; two lines → two tasks + builder `count: 2`.
- [x] Optional `glob :: task` scoping parsed (comma-separated globs; default `**`).
- [x] Single-line input still works (backward compatible).
- [x] jsdom launcher tests cover the above; full frontend suite + `npm run check` green.

## Notes
Enables the CPE-582 smoke to show genuine parallel coordination. For true parallelism give each task a
disjoint scope (`src/** :: …` / `docs/** :: …`); same-scope tasks run in turn by design.
