---
id: CPE-1215
title: "Spotlight global hotkey (tauri-plugin-global-shortcut) + Settings control"
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-704
---

## Summary
Part of CPE-704. A global OS hotkey that opens spotlight even when the window is hidden. OS-gated.

## Build
- Add `tauri-plugin-global-shortcut` dep + init; add the capability entry to `src-tauri/capabilities/default.json`
  (`global-shortcut:allow-register`/`allow-unregister`) or the register is denied at runtime. Register a default
  chord that emits a `spotlight:open` event. **Enable/disable + chord live in Settings — NEVER a launch-time
  modal** ([[avoid-modal-permission-popups]]). Unregister cleanly on disable (no background cost when off).

## Acceptance Criteria
- [x] Builds; clippy clean both modes; capability present; the setting persists + toggling register/unregisters.
- [x] **OS-gated:** the hotkey firing while the window is hidden is attended-verified (flagged, not headless).

## Work Log
- 2026-08-01 — Filed by Foreman (sprint, epic CPE-704). Depends on CPE-1214's event/open. OS-gated.
- 2026-08-01 — Implemented: `tauri-plugin-global-shortcut` added (desktop-only target dep, alongside
  updater/window-state); plugin initialized in `run()` with zero shortcuts claimed at startup (no
  background cost by default). `global-shortcut:allow-register`/`allow-unregister` added to
  `src-tauri/capabilities/default.json`. Two new commands `register_spotlight_hotkey`/
  `unregister_spotlight_hotkey(app, chord)` — idempotent, desktop-only (`cfg(not(any(android, ios)))`) —
  wrap `GlobalShortcutExt::on_shortcut`/`unregister`; the handler emits `spotlight:open` on
  `ShortcutState::Pressed`, which the CPE-1216 overlay will listen for. Registered in both
  `generate_handler!` and the specta `collect_commands!` list; `bindings.gen.ts` regenerated via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (drift guard clean).
  Settings: `spotlightHotkeyEnabled` (default OFF) + `spotlightHotkeyChord` (default
  `"CommandOrControl+Shift+Space"`) added to `src/lib/settings.ts`, with `initSettings()` best-effort
  re-registering on startup only when the flag was left on from a prior session (never blocks startup on
  failure). New self-contained `src/lib/components/SpotlightHotkeySettings.svelte` (mirrors
  `ShellIntegration.svelte`'s pattern) renders the toggle + editable chord field in `SettingsDialog.svelte`,
  calling register/unregister live and reverting the field on error. No launch-time consent modal —
  [[avoid-modal-permission-popups]] honored; the control lives only in Settings.
  Did NOT touch `App.svelte` or create `Spotlight.svelte` (owned by the CPE-1216 worker).
  **Verification:** `cargo build`, `cargo test` (85 passed), and `cargo clippy --all-targets -- -D
  warnings` all green in both the default feature set and `--features "sidecar-platform
  specta-bindings"`. `npm run check` — 0 errors. `npm test` — 143 files / 1590 tests passed, including a
  new `spotlightHotkeyEnabled`/`spotlightHotkeyChord` describe block in `settings.test.ts` (defaults +
  round-trip).
  **OS-gated / deferred, per the ticket:** actually pressing the chord while the main window is hidden
  and observing `spotlight:open` fire is NOT headless-verifiable — no test harness here can simulate a
  real OS-level global-hotkey keypress. What WAS verified headlessly: the plugin compiles/links, the
  Settings toggle persists and round-trips, and the register/unregister commands are correctly wired end
  to end (capability present, commands registered, chord passed through). Attended desktop verification
  (press the chord with the window minimized/hidden, confirm Spotlight opens once CPE-1216 lands) is
  deferred to a human/attended pass, as instructed.
