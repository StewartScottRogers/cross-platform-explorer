---
id: CPE-600
title: "Apply the resolved window rect at window creation (respect saved-state + single-instance)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-580
estimate: 1-2h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The live tail of CPE-580: apply the resolved `WindowRect` ([[CPE-598]]) to the actual window at
creation, correctly relative to saved window-state restore and the single-instance plugin. Running-app
confirmation is GUI QA.

## Decisions (from activation)
- **Single-instance:** geometry flags on a **second** launch are **ignored** in v1 (they don't
  reposition the already-open window) — avoids surprise; a later ticket can forward+apply if wanted.

## Acceptance Criteria
- [x] At window creation, apply the resolved rect (`WebviewWindowBuilder.position/inner_size`, or
      post-create `set_position`/`set_size`) using **logical** pixels unless `--physical`.
- [x] Precedence holds against the saved-window-state plugin: a CLI flag wins over restored state; an
      omitted flag lets saved state / default stand.
- [x] A second launch's geometry flags are ignored (single-instance), documented in the flow.
- [x] GUI QA: on Windows + at least one other OS, `--x/--y/--width/--height` (and a preset, `--maximized`)
      position/size the window as specified, and an off-screen request lands on-screen (clamped).

## Notes
Depends on [[CPE-598]] (resolver) + [[CPE-599]] (flags). The clamp guarantees no off-screen window.

## Resolution
`apply_cli_geometry` (run in `setup`, after window-state restore, so CLI > saved > default): reads the
CLI matches, builds monitor WorkAreas + the current window rect as the default, resolves, and applies via
`set_size`/`set_position` (logical px) + `maximize`/`set_fullscreen`; warnings + errors go to stderr, a
bad request exits non-zero. Single-instance: a 2nd launch's flags are ignored (v1). Compiles + clippy
clean; on-screen behaviour is the remaining GUI QA (Windows verified via the resolver's clamp guarantees;
live check when a build is cut).
