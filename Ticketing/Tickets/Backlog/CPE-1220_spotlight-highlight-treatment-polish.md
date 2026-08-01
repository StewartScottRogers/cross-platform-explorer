---
id: CPE-1220
title: "Polish: make Spotlight matched-run highlight pop more than a thin underline"
type: Task
status: Open
priority: Low
component: frontend
tags: [ready]
estimate: 20m
created: 2026-08-01
closed:
---

## Context
Epic-704 Visual Critic (VISUAL PASS on the Spotlight overlay) raised one non-blocking nit: on the
active (accent-blue) result row, the matched-substring highlight is a thin underline
(`Spotlight.svelte`: `.sp-row.active :global(.sp-hl) { text-decoration: underline }`), which is
slightly less scannable at a glance than a bolder treatment (bold weight and/or a subtle background
tint that still reads on the accent fill).

## Acceptance criteria
- The matched run is more visually prominent on both the active and inactive rows, still legible on
  the accent-blue active background, still theme-var driven (no hard-coded colours).
- No regression to the non-active `.sp-hl` treatment; re-capture the spotlight gui-smoke screenshot.

## Notes
Pure visual polish; deferred out of epic-704 (which passed) as a standalone tweak.
