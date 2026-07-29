---
id: CPE-599
title: "CLI surface for window geometry via tauri-plugin-cli (--x/--y/--width/--height + presets)"
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
The command-line front door: define the geometry flags with **tauri-plugin-cli** (schema-driven, auto
`--help`, capability-gated), parse them, and feed the resolver ([[CPE-598]]).

## Decisions (from activation)
- **Parser:** `tauri-plugin-cli` (schema + `--help` + validation; add the dep + a capability entry).
- **Bad input:** non-numeric / zero / negative → clear message + **non-zero exit**; out-of-range handled
  by the resolver's clamp+warn.

## Acceptance Criteria
- [x] `tauri-plugin-cli` added; flags declared in the config schema with help text: core `--x --y
      --width --height`, plus `--position <preset>`, `--monitor <n>`, `--maximized`, `--fullscreen`,
      `--physical`.
- [x] A capability entry authorises the CLI plugin (`src-tauri/capabilities/default.json`).
- [x] Parsed args map into the resolver's input type; `--help` lists every flag with its meaning + the
      pixel-unit contract.
- [x] Non-numeric/zero/negative geometry → a clear stderr message + non-zero exit (never a mangled
      window); tests for the parse→resolver mapping.
- [x] `cargo check`/`clippy` green.

## Notes
Keep parsing thin — the logic is all in the resolver ([[CPE-598]]). Apply step is [[CPE-600]].

## Resolution
`tauri-plugin-cli` added (dep + `.plugin(tauri_plugin_cli::init())` + `cli:default` capability);
`tauri.conf.json` declares the flag schema (`--x/--y/--width/--height --position --monitor --maximized
--fullscreen --physical`) with help text + the logical-pixel contract. `geometry::parse_args` maps the
plugin's value map into `GeometryArgs`; a present-but-unparseable numeric (or bad preset) is a hard error
→ non-zero exit. `cargo check` + clippy clean.
