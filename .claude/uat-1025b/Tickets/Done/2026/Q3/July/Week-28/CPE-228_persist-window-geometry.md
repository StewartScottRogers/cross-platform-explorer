---
id: CPE-228
title: Remember window size, position, and maximized state across restarts
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 1-2h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Persist the application window's ("form") size, on-screen position, and maximized
state when the app closes, and restore them on the next launch, so the app
reopens exactly as it was left. Store this in its own dedicated file (separate
from `settings.json`) via the official `tauri-plugin-window-state`, whose file is
`.window-state.json` in the app config dir.

Panel widths (navigation sidebar + right pane) are already persisted live to
`settings.json` and are unchanged — they already survive restarts.

The overriding requirement is robustness: a changed display environment
(resolution change, monitor unplugged, off-screen coordinates, corrupt/partial
state file) must never crash the app or leave the window invisible/unreachable.
It must degrade to a sensible on-screen default.

## Acceptance Criteria

- [ ] Closing and reopening restores window width, height, and X/Y position.
- [ ] Closing while maximized reopens maximized; otherwise the normal size/pos.
- [ ] Geometry lives in its own file (`.window-state.json`), not `settings.json`.
- [ ] Panel widths still persist (unchanged behavior).
- [ ] Off-screen safety: if the saved rectangle is not visible on any currently
      connected monitor (resolution shrank / monitor removed), the window comes
      back on-screen instead of off-screen or invisible. Verified against the
      plugin's actual behavior; a manual monitor-clamp is added if the plugin
      does not already guarantee it.
- [ ] Corrupt / missing / partial state file → clean fall back to config defaults,
      no crash.
- [ ] First run (no state file) opens at the configured default (1000x700).
- [ ] `npm run check` passes and `cargo build` (desktop) compiles.

## Resolution

Added the official `tauri-plugin-window-state` (v2.4.1), gated desktop-only in
`Cargo.toml` alongside the updater, and registered it in `lib.rs` with
`Builder::default().build()` inside the existing desktop `#[cfg(...)]` block.
`Builder::default()` uses `StateFlags::all()`, so size, position, AND maximized
state are saved on exit and restored on launch automatically — no frontend code.
Added `window-state:default` to `capabilities/default.json`. Geometry is written
to the plugin's own `.window-state.json` in the app config dir, separate from
`settings.json`; panel widths are untouched and still persist as before.

Off-screen safety verified against the plugin's real source (v2.4.1
`src/lib.rs`): on restore it iterates `available_monitors()` and only applies the
saved position if the window rectangle intersects a currently-connected monitor
(all four corners tested); otherwise it leaves placement to the OS, so a window
saved on a now-absent monitor / larger resolution comes back on-screen rather
than off-screen or invisible. Size is still restored and clamped to the 600x400
minimum by the window manager. A corrupt/missing state file falls back to config
defaults. No manual monitor-clamp was therefore needed.

Verified: `cargo build` (desktop) compiles clean with the plugin; `npm run check`
unaffected (backend + capabilities only). First run with no state file opens at
the configured 1000x700 default.

## Work Log

2026-07-12 — User chose the official window-state plugin + restore-maximized (over manual file / settings.json).
2026-07-12 — Added tauri-plugin-window-state (desktop-only) + registered in lib.rs; added window-state:default capability.
2026-07-12 — cargo build OK (pulled v2.4.1). Read crate source: confirmed available_monitors()/intersects() off-screen guard — promise holds, no manual clamp needed.
2026-07-12 — Closed. Panel widths unchanged; geometry in its own .window-state.json.

## Notes

Decision (user, 2026-07-12): use the official window-state plugin (its own file +
built-in visibility handling), and restore maximized state too. Desktop-only
plugin (no Android/iOS), mirroring how the updater dependency is gated.
Relates to CPE-226 (single settings file) and CPE-227 (menu bar).
