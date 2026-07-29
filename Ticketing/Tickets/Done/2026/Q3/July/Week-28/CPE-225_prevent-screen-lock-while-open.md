---
id: CPE-225
title: Prevent the screen from locking or sleeping while the app is open
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

While the application is open, keep the display awake — inhibit the OS screen lock, screensaver, and
display sleep so the user can watch the explorer (e.g. during long-running Agent Watch sessions)
without the screen dimming or locking. The inhibition is released automatically when the app closes.

## Acceptance Criteria

- [x] While the app is running, the OS does not lock the screen, start the screensaver, or sleep the
      display
- [x] The inhibition is released cleanly when the app quits or crashes (no lingering keep-awake state)
- [x] Works on Windows, macOS, and Linux (per-platform mechanism as appropriate)
- [x] Any new Tauri plugin/permission the feature needs is declared in
      `src-tauri/capabilities/default.json` — N/A: implemented with a native Rust crate, not a Tauri
      plugin invoked from the frontend, so no capability/permission entry is required
- [x] The plain explorer stays fast, small, and predictable with the feature idle (per PURPOSE.md)

## Resolution

Implemented always-on keep-awake for the app's whole lifetime using the cross-platform `keepawake`
crate (v0.6). The guard is created once, on the main thread, in `run()` and owned by the `app.run(..)`
run-loop callback, so it is dropped — releasing the assertion — the moment the loop ends (app quit).
On a hard crash the OS releases the assertion on process death, so nothing lingers either way.

Under the hood `keepawake` maps to the native mechanism per platform: Windows
`SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED)`, macOS `IOPMAssertion`
(PreventUserIdleDisplaySleep), Linux `org.freedesktop.ScreenSaver` inhibit over zbus (pure-Rust, so
no libdbus dev headers needed in CI). Acquisition failure is logged, not fatal — the explorer still
runs, the screen just isn't held awake.

**Files changed:**
- `src-tauri/Cargo.toml` — added `keepawake = "0.6"` to the desktop-only target deps (gated
  `cfg(not(any(target_os = "android", target_os = "ios")))`, alongside the updater plugin).
- `src-tauri/src/lib.rs` — create the keep-awake guard in `run()`; switched the tail from the
  `.run(context)` shortcut to `.build(context)?` + `app.run(move |_, _| { … })` so the run loop owns
  the guard for the app's lifetime.

**Design choice:** always-on (no UI toggle), matching the request as written and keeping zero
frontend/`invoke` surface — so no new Tauri capability/permission was needed and the idle explorer is
unchanged. A future toggle can hold the guard in `Option` state without disturbing this structure.

**Verification:** `cargo check`, `cargo clippy --all-targets`, and `cargo test` (37 passed) all clean
on Windows; `npm run check` reports 0 errors. A throwaway probe example confirmed the guard *acquires*
successfully at runtime on this Windows machine (`create()` → `Ok`). The OS-level DISPLAY request
itself could not be observed in-session because `powercfg /requests` requires an elevated prompt,
which wasn't available here; macOS/Linux compilation is covered by CI.

## Work Log

2026-07-12 — Picked up. Estimate: 2-3h. Plan: hold a cross-platform "keep display awake" assertion
for the app's whole lifetime via a Rust dependency, created in the Tauri `setup` hook. Always-on
(matches the request); no frontend/invoke surface, so no new capability needed.
2026-07-12 — Surveyed the codebase: backend is a single `src-tauri/src/lib.rs` with `run()` building
the Tauri app; no `setup` hook yet. Cargo toolchain confirmed working locally (1.97.0), so I can
`cargo check` the Windows target here; CI covers macOS/Linux compile.
2026-07-12 — Chose the `keepawake` crate (v0.6) over hand-rolled per-platform FFI: one dependency
covers Windows/macOS/Linux and, being pure-Rust zbus on Linux, needs no system dbus headers in CI.
Verified its API against docs.rs before wiring it in.
2026-07-12 — To avoid Send/Sync constraints on Tauri managed state, held the guard by restructuring
the tail of `run()` from `.run(context)` to `.build(context)?` + `app.run(move |_,_| …)`, letting the
run-loop callback own the guard (main-thread, dropped on quit).
2026-07-12 — `cargo check`, `cargo clippy --all-targets`, `cargo test` (37 passed) all clean on
Windows; `npm run check` 0 errors.
2026-07-12 — Ran a temporary `examples/keepawake_probe.rs`: guard `create()` returned Ok on this
machine, confirming the SetThreadExecutionState call succeeds at runtime. Could not observe the OS
DISPLAY request directly — `powercfg /requests` demands an elevated prompt (not available here).
Removed the probe afterward.

## Notes

Open question for the owner: should keep-awake be **always on** while the app runs (simplest, matches
the request as written), or a **user-toggleable** setting that defaults on? Drafted here as always-on;
easy to add a toggle if preferred.

Per-platform reference: Windows `SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED)`;
macOS `IOPMAssertionCreateWithName` (kIOPMAssertionTypePreventUserIdleDisplaySleep) or `caffeinate`;
Linux `org.freedesktop.ScreenSaver` DBus inhibit or `systemd-inhibit`. A crate like `keepawake` may
cover all three.
