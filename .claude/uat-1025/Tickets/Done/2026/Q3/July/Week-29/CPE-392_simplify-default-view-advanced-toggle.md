---
id: CPE-392
title: "AI Console: simplify the default view — collapse optional fields under 'Advanced ▾'"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Reduce first-look overwhelm for newcomers. The default toolbar shows ~8 fields; most are optional.
Keep the essentials visible (Agent, Provider, Account, Model, Project folder, Launch) and move the
optional ones — **Fast model**, per-launch **API key**, and the **Setup** (save/select) controls —
into a collapsed **"Advanced ▾"** section, hidden by default.

## Acceptance Criteria
- [x] Default view = Agent, Provider, Account(when ≥2 keys), Model, Project folder + Keys…/Recent…/
      Manage agents ▾/? /Launch. No Fast model / API key / Setup visible.
- [x] An "Advanced ▾" toggle reveals Fast model + per-launch API key + Setup controls.
- [x] Wiring/ids unchanged; harness tests: fields hidden by default, toggle shows them.

## Work Log
2026-07-14 — Filed: inexperienced-user goal, part 1 (declutter).
