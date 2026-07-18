---
id: CPE-599
title: "CLI surface for window geometry via tauri-plugin-cli (--x/--y/--width/--height + presets)"
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
The command-line front door: define the geometry flags with **tauri-plugin-cli** (schema-driven, auto
`--help`, capability-gated), parse them, and feed the resolver ([[CPE-598]]).

## Decisions (from activation)
- **Parser:** `tauri-plugin-cli` (schema + `--help` + validation; add the dep + a capability entry).
- **Bad input:** non-numeric / zero / negative → clear message + **non-zero exit**; out-of-range handled
  by the resolver's clamp+warn.

## Acceptance Criteria
- [ ] `tauri-plugin-cli` added; flags declared in the config schema with help text: core `--x --y
      --width --height`, plus `--position <preset>`, `--monitor <n>`, `--maximized`, `--fullscreen`,
      `--physical`.
- [ ] A capability entry authorises the CLI plugin (`src-tauri/capabilities/default.json`).
- [ ] Parsed args map into the resolver's input type; `--help` lists every flag with its meaning + the
      pixel-unit contract.
- [ ] Non-numeric/zero/negative geometry → a clear stderr message + non-zero exit (never a mangled
      window); tests for the parse→resolver mapping.
- [ ] `cargo check`/`clippy` green.

## Notes
Keep parsing thin — the logic is all in the resolver ([[CPE-598]]). Apply step is [[CPE-600]].
