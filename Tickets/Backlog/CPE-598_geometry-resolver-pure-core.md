---
id: CPE-598
title: "Pure window-geometry resolver: (args, monitors, defaults) → final rect (clamp/precedence/DPI)"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
epic: CPE-580
estimate: 2-3h
created: 2026-07-17
---

## Summary
The headlessly-testable brain of CPE-580: a pure function that turns parsed geometry args + the monitor
work-area list + defaults into a final `WindowRect`. All the foolproofing lives here — clamping, preset
resolution, precedence, DPI/unit handling — so it can be exhaustively unit-tested without a window.

## Decisions (from activation)
- **Precedence:** `CLI flag > saved window state > config default`.
- **Off-screen protection:** clamp the final rect against the actual monitor **work area** so a window
  can never open where it can't be seen/dragged.
- **Bad input:** non-numeric / zero / negative size → an error the caller turns into a clear message +
  non-zero exit; out-of-range/off-screen values → **clamp + warn** (never a stranded window).
- **Unit:** logical pixels are the contract; `--physical` opts out (conversion handled here).
- **Preset vs explicit:** an explicit `--x/--y/--width/--height` **wins** over a `--position <preset>`.
- **Presets/convenience** (`--position`, `--monitor`, `--maximized`, `--fullscreen`) resolve **down to
  the same four scalars**, not special cases.

## Acceptance Criteria
- [ ] `resolve_geometry(args, monitors, defaults) -> Result<WindowRect, GeometryError>` (or clamp-with-
      warnings return) implementing the decisions above; no Tauri/window deps (pure).
- [ ] Each of x/y/width/height is independently optional; omitted fields take the default; any subset
      composes.
- [ ] Exhaustive tests: clamping onto the work area, precedence, preset resolution, `--monitor` offset,
      DPI logical↔physical, and junk-input errors.
- [ ] `cargo test` + `cargo clippy --all-targets -D warnings` green.

## Notes
Mirrors the swarm pure-core/live-tail split; the apply step is [[CPE-600]].
