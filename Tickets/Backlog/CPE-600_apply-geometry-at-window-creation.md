---
id: CPE-600
title: "Apply the resolved window rect at window creation (respect saved-state + single-instance)"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
epic: CPE-580
estimate: 1-2h
created: 2026-07-17
---

## Summary
The live tail of CPE-580: apply the resolved `WindowRect` ([[CPE-598]]) to the actual window at
creation, correctly relative to saved window-state restore and the single-instance plugin. Running-app
confirmation is GUI QA.

## Decisions (from activation)
- **Single-instance:** geometry flags on a **second** launch are **ignored** in v1 (they don't
  reposition the already-open window) — avoids surprise; a later ticket can forward+apply if wanted.

## Acceptance Criteria
- [ ] At window creation, apply the resolved rect (`WebviewWindowBuilder.position/inner_size`, or
      post-create `set_position`/`set_size`) using **logical** pixels unless `--physical`.
- [ ] Precedence holds against the saved-window-state plugin: a CLI flag wins over restored state; an
      omitted flag lets saved state / default stand.
- [ ] A second launch's geometry flags are ignored (single-instance), documented in the flow.
- [ ] GUI QA: on Windows + at least one other OS, `--x/--y/--width/--height` (and a preset, `--maximized`)
      position/size the window as specified, and an off-screen request lands on-screen (clamped).

## Notes
Depends on [[CPE-598]] (resolver) + [[CPE-599]] (flags). The clamp guarantees no off-screen window.
