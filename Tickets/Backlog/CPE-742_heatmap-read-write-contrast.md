---
id: CPE-742
title: Agent Watch — read-vs-write contrast in the folder heat-map
type: feature
component: Frontend
priority: low
status: Open
tags: ready
created: 2026-07-19
epic: CPE-726
estimate: 2-3h
---

## Summary
Child of CPE-726. The folder heat-map (CPE-402) lights up a folder row when the agent is changing files in
its subtree, but `folderHasActivity` / `folderHasActivityNorm` take only paths — they ignore the activity
**kind**, so a folder the agent has only *read* inside looks identical to one it is *writing* in. Add
read-vs-write contrast so a consulted-only subtree reads as cooler/distinct from a mutated one, matching the
CPE-405 principle that a read is the weakest signal.

## Scope
- Extend the folder-activity check to carry kind (or add a kind-aware variant) so a folder can be classed
  write-touched vs read-only-touched in its subtree — write wins when both are present.
- Apply a distinct, cooler/dimmer heat treatment for read-only-touched folder rows (theme-variable driven,
  light/dark parity), reusing the CPE-405 read accent vocabulary.
- Keep it pure + headless-testable; preserve the CPE-698 normalize-once optimization (don't re-normalize the
  activity set per folder row).

## Acceptance
- [ ] A folder whose subtree the agent only *read* renders a distinct (cooler) heat than a *written* one.
- [ ] Write outranks read when a subtree has both; existing write heat-map behaviour is unchanged.
- [ ] The per-row check stays O(1) over a pre-normalized activity set (no CPE-698 regression).
- [ ] Headless tests cover read-only vs write vs mixed subtrees; no cost when not watching.

## Notes
Builds on the shipped CPE-405 read pipeline; frontend-only.
