---
id: CPE-580
title: "EPIC: Launch the GUI with command-line window-geometry options (x / y / width / height)"
type: Task
status: Proposed
priority: Medium
component: Backend
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-17
---

## Summary
Let the app be launched with command-line flags that set the **window geometry** — position (`--x` /
`--y`) and size (`--width` / `--height`) — so a person or a script can open the explorer exactly where
and how big they want it. The design goal the user set is **foolproof and orthogonal**: each dimension
is an independent, optional knob; any subset composes cleanly; and no combination of flags (or bad
input) can produce a window you can't see or grab.

## Goal
`cpe --x 100 --y 100 --width 1200 --height 800` opens a 1200×800 window at (100,100). Omit any flag and
that dimension falls back to the normal default (saved state / config). Every flag is independently
optional and independently valid; junk input fails loudly, never silently, and never strands the window
off-screen.

## Design principles (the user's two asks)

**Orthogonal.** The window is four independent scalars — `x`, `y`, `width`, `height`. Each has its own
flag, each is optional, and none depends on another. Supply any subset; unspecified fields keep their
default. Convenience layers (named presets, `--maximized`, `--fullscreen`, `--monitor`) are a *separate*
orthogonal tier that **resolve down to those same four scalars** rather than being special cases — so
the core stays a plain rectangle.

**Foolproof.** The footguns are known; design them out up front:
- **Off-screen protection** — validate/clamp the final rect against the actual monitor **work area** so
  a window can never open where it can't be seen or dragged (the #1 CLI-geometry footgun).
- **DPI / logical-vs-physical** — Tauri distinguishes `LogicalSize`/`PhysicalSize`; pick **logical
  pixels** as the contract (stable across DPI scaling), documented, with `--physical` as an explicit
  opt-out. Never leave the unit ambiguous.
- **Bad input fails fast** — non-numeric, zero/negative size, absurd values → a clear message + non-zero
  exit (or documented clamp), never a silently mangled window.
- **Defined precedence** — `CLI flag > saved window state > config default`, documented in one place.
- **Multi-monitor** — coordinates live in the virtual-desktop space; `--monitor <n>` positions relative
  to a chosen display as an orthogonal selector.

## Rough scope (NOT decomposed)
- **CLI surface** — decide `tauri-plugin-cli` (schema + help + validation, needs a capability entry) vs.
  hand-parsing `std::env::args()`. Flag set: `--x --y --width --height` (core) + likely `--position
  <preset>`, `--monitor <n>`, `--maximized`, `--fullscreen`, `--physical`.
- **Pure geometry resolver** — `(parsed args, monitor list, defaults) -> final WindowRect`. This is the
  headlessly unit-testable brain (mirrors the swarm pure-core / live-tail split): clamping, preset
  resolution, precedence, DPI conversion all live here and get exhaustive tests.
- **Live wiring** — apply the resolved rect at window creation (`WebviewWindowBuilder.position/inner_size`
  or post-create `set_position`/`set_size`) in `src-tauri`; the running-app confirmation is GUI QA.
- **Interaction with existing behaviour** — saved window-state restore and the single-instance plugin
  (if a second launch forwards args to the running instance, do those args reposition it?).
- **Docs** — README "launch options" section + in-app docs library entry (per the docs-library rule).

## Open questions (resolve at activation)
- **CLI parser:** `tauri-plugin-cli` (richer, self-documenting, capability-gated) or a small hand-rolled
  parser (zero new dep, full control)?
- **Bad/out-of-range input:** hard-reject with non-zero exit, or clamp-with-warning so scripts never die?
  (Foolproof cuts both ways — pick one and make it consistent.)
- **Preset vs explicit conflict:** if both `--position center` and `--x` are given, does explicit win,
  or last-flag-wins?
- **Single-instance:** should geometry flags on a second launch move the already-open window, or be
  ignored?
- **Unit default:** confirm logical pixels as the contract with `--physical` opt-out.

## Definition of Done (epic-level)
- The four core flags work independently and in any combination; omitted flags fall back to defaults.
- No flag combination or input can open an off-screen / zero-size / ungrabbable window.
- Precedence and pixel-unit contract are documented and match behaviour.
- The geometry resolver is covered by headless unit tests (clamping, presets, precedence, DPI, junk
  input); the apply step is verified by GUI QA on Windows + at least one other OS.
- README + in-app docs updated.

## Notes
Fits the codebase's proven **pure-core + live-tail** shape: a fully-tested geometry resolver plus a thin
live apply layer that needs GUI QA. `big-design` — the design work is the foolproofing/precedence rules,
not the plumbing. Dormant brief until activated with `/ticketing-epic activate CPE-580`.

## Work Log
2026-07-17 — Filed as a dormant `Proposed` brief on request. Not decomposed; activate to plan.
