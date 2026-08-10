---
id: CPE-1546
title: "High contrast: one-shot OS signal read (Windows/macOS/Linux) feeding ContrastSetting 'system'"
type: Feature
status: Backlog
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-1496
created: 2026-08-09
---
## Context
CPE-1544 lands `resolveContrast(pref, osHighContrastActive)` where `pref === "system"` needs a real
`osHighContrastActive` boolean to mean anything; until this ticket it's always called with the default
`false`, so `"system"` degrades to `"off"`. This ticket supplies that boolean from the OS's actual
accessibility high-contrast signal, mirroring CPE-1494's own per-OS Rust pattern (the epic notes call
this "the one piece of genuinely custom per-OS Rust" in the theme program): Windows
`SystemParametersInfo(SPI_GETHIGHCONTRAST)`, macOS `NSWorkspace.accessibilityDisplayShouldIncreaseContrast`,
Linux the `org.freedesktop.appearance` portal's `contrast` key.

**Deliberately scoped to a one-shot read at startup, not a live subscription.** A push-based live
subscription (Windows `WM_SETTINGCHANGE`, macOS `NSNotificationCenter`, a D-Bus signal listener on
Linux) is real additional complexity — a background thread/event loop per platform — and its actual
live-tracking behavior can only be confirmed by flipping the OS setting while the app runs, which is an
attended cross-OS check, not something this ticket can land headless-verified. A one-shot query read
once at boot is cleanly headless (compiles per `#[cfg(target_os = ...)]`, is unit-testable via a mockable
seam, and the frontend wiring is a single `invoke` + `applyTheme` call). File live-tracking as a
follow-on ticket if/when it's prioritized; note it in the Work Log rather than scope-creeping this one.

## Scope
- New domain module `crates/server/src/high_contrast.rs` per [[SERVER-ARCHITECTURE]] convention (domain
  logic lives in `cpe-server`, not `lib.rs`): `pub fn is_high_contrast_active() -> bool`, `#[cfg]`-gated
  per platform:
  - **Windows**: `SystemParametersInfo(SPI_GETHIGHCONTRAST, ...)` reading `HCF_HIGHCONTRASTON` from the
    returned `HIGHCONTRASTW` struct, via the `windows` crate already a dependency
    (`src-tauri/Cargo.toml:158`, `Win32_UI_Accessibility` feature — add it to the existing feature list,
    same crate, no new dependency).
  - **macOS**: `NSWorkspace.sharedWorkspace().accessibilityDisplayShouldIncreaseContrast()` via `objc2`/
    `objc2-app-kit` — `objc2`/`objc2-foundation` are already dependencies (`src-tauri/Cargo.toml:189-190`);
    `objc2-app-kit` for `NSWorkspace` is a new but same-family addition alongside the existing
    `objc2-core-foundation`/`objc2-core-graphics`/`objc2-image-io` crates.
  - **Linux**: a one-shot D-Bus property read of `org.freedesktop.appearance`'s `contrast` key via the
    `xdg-desktop-portal` settings interface. This needs a new pure-Rust D-Bus dependency (`zbus` or
    `ashpd`, blocking one-shot call, not the async event-loop form) — flagged explicitly as an exception
    to the "no new deps" convention, same justified exception CPE-1494's own epic brief accepts for the
    identical portal read, and the same family the repo already trusts transitively (`keyring`'s
    `sync-secret-service` feature pulls in `zbus` for Linux, `src-tauri/Cargo.toml:200-203`). Unavailable
    portal / non-Linux-desktop → return `false` (fail-open to "no high contrast", never panic/hang; bound
    any D-Bus call with a short timeout, same discipline as the existing bounded network-share reads at
    `src-tauri/src/lib.rs:5751-5761`).
  - Any platform without a real signal (or an error reading it) returns `false` — the manual `"high"`
    override in CPE-1544/CPE-1545 remains the reliable path regardless.
- Thin `#[tauri::command]` dispatcher in `src-tauri/src/lib.rs`: `async fn is_high_contrast_active() ->
  bool` wrapped in `tokio::task::spawn_blocking` per [[async-all-blocking-commands]] (a synchronous
  Win32/D-Bus/Cocoa call must never block the main thread), one line calling into
  `cpe_server::high_contrast::is_high_contrast_active()`, registered in the `generate_handler![]` list
  (`src-tauri/src/lib.rs:10811` onward — append alongside the other simple no-arg queries like
  `home_dir`/`special_folders`). No `capabilities/default.json` entry needed (a plain `#[tauri::command]`,
  not a plugin permission).
- Frontend: new small `src/lib/highContrastSignal.ts` (mirrors `theme.ts`'s shape) exporting
  `queryOsHighContrast(): Promise<boolean>` that calls `invoke("is_high_contrast_active")` (via
  `src/lib/invoke.ts`'s busy-tracking wrapper per [[busy-cursor]] — though this call is fast/one-shot at
  boot, before the window is interactive, so the busy cursor is a non-issue either way) and returns
  `false` on any rejection (older host without the command, non-Tauri context, etc. — never throws).
- `src/main.ts`: one small addition right after the existing theme wiring (`src/main.ts:27-28`) — query
  `queryOsHighContrast()`, then re-call `applyTheme(loadTheme(), loadContrast(), osHighContrastActive)`
  with the real signal so a persisted `"system"` contrast preference is honored from first paint. This is
  the only touch to `src/main.ts`, which is small/dedicated (not one of the sprint's listed hot shared
  files) — still keep it to those two lines.

## How
- Rust: unit tests for `crates/server/src/high_contrast.rs` where feasible per platform — the Windows/
  macOS/Linux branches are thin FFI/D-Bus calls that can't be meaningfully unit-tested without the real
  OS signal, so tests focus on the fail-open contract (unknown/unsupported platform returns `false`, never
  panics) and any pure parsing helper (e.g. the D-Bus reply → bool mapping) extracted so it's testable
  without a live bus. `cargo clippy --all-targets -D warnings` on all three OSes (CI's existing 3-OS
  backend matrix, per [[ci-runs-three-os-backend-matrix]]) is the real headless gate here — it proves
  every `#[cfg]` branch actually compiles cross-platform, which a Windows-only dev loop can't confirm.
- Frontend: `src/lib/highContrastSignal.test.ts` mocking `invoke` (resolve `true`/`false`/reject) and
  asserting `queryOsHighContrast()`'s return + the fail-open-to-`false` behavior on rejection.

## Verify
`cargo build` + `cargo clippy --all-targets -D warnings` (both feature modes) locally on Windows, with CI
confirming macOS + Linux compile via the 3-OS matrix; `npx vitest run src/lib/highContrastSignal.test.ts`;
`npm run check`. Fully headless to land.

**Attended cross-OS QA-burndown item (not blocking):** confirming the signal actually reflects a real
OS high-contrast toggle (Windows Ease of Access, macOS Accessibility > Display > Increase contrast,
GNOME/KDE high-contrast) on each real OS is an attended check queued the same way CPE-1494's own accent
epic queues its cross-OS verification — not required to land this ticket, since the fail-open default and
the manual "High" override in CPE-1545 both work regardless of whether the signal read is ever attended-verified.

## Notes
**Conflict surface:** new file `crates/server/src/high_contrast.rs` (+ its module registration line in
`crates/server/src/lib.rs`), a small append to `src-tauri/src/lib.rs` (one new command fn + one line in
the `generate_handler![]` list, `src-tauri/src/lib.rs:10811`+), a Linux-only dependency addition to
`src-tauri/Cargo.toml` (`[target.'cfg(target_os = "linux")'.dependencies]` block,
`src-tauri/Cargo.toml:202-203` region) plus a Windows feature-list addition
(`src-tauri/Cargo.toml:158-174` region) and a macOS dependency addition (`src-tauri/Cargo.toml:182-198`
region), new file `src/lib/highContrastSignal.ts` + its test, and two lines in `src/main.ts`
(`src/main.ts:27-28` region). No `src/app.css`, `src/App.svelte`, `SettingsDialog.svelte`, or
`sectionDocs.ts` edits. **Dispatch order:** prereq CPE-1544 (needs `loadContrast`/the widened
`applyTheme`). Independent of CPE-1543 and CPE-1545 (no shared files) — can run in parallel with
CPE-1545 once CPE-1544 lands.
