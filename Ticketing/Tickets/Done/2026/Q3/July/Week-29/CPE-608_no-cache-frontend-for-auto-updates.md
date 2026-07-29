---
id: CPE-608
title: Serve the frontend no-cache on Windows so auto-updates always refresh
type: enhancement
component: Backend
priority: medium
status: Done
tags: ready
estimate: 2-3h
created: 2026-07-18
closed: 2026-07-18
---

## Summary
Preventive hardening surfaced while closing CPE-553 (language switch). On **Windows/WebView2** the app's
frontend is served over the `tauri` asset protocol with **no `Cache-Control` header**, so WebView2 may
heuristically cache it. Vite content-hashes JS filenames (`assets/index-<hash>.js`) — so a *new build's*
JS is never served stale by URL — **but `index.html` is not hashed**. A cached `index.html` keeps
referencing the OLD hashed JS, pinning the app to a stale bundle **after an auto-update**. Since this app
auto-updates, that's a latent "user updates but sees the old UI" risk for the whole frontend (CPE-553's
i18n symptom was one instance). Make the frontend responses no-cache on Windows.

## Research (2026-07-18, dispositive — done while closing CPE-553)
- **The gap is real, not hypothetical.** `wry 0.55.1` sets `Cache-Control: no-store` on served assets
  **only on Android** (`src/android/kotlin/RustWebViewClient.kt`). Windows/macOS/Linux get **no**
  cache-control from wry or tauri (grep of `tauri-2.11.5/src` finds no cache-control). So Windows is
  cache-permissive by default.
- **The config route can't do it.** `app.security.headers` (tauri-utils 2.9.3 `HeaderConfig`) is a fixed
  allowlist — Access-Control-*, Cross-Origin-*, Permissions-Policy, Service-Worker-Allowed,
  Timing-Allow-Origin, X-Content-Type-Options. **No `Cache-Control` field.** Dead end.
- **There is no app-`Builder`-level `on_web_resource_request`.** The hook exists only on
  `WebviewWindowBuilder`/`WebviewBuilder`:
  `Fn(http::Request<Vec<u8>>, &mut http::Response<Cow<'static, [u8]>>)`.
- **So the fix requires creating the main window in Rust** (`setup()` via `WebviewWindowBuilder`) instead
  of declaratively in `tauri.conf.json`, so the hook can inject `Cache-Control: no-store` (at least for
  `index.html`; simplest is all frontend responses — the assets are embedded/local so the perf cost of
  not caching is negligible).

## ⚠️ Watch the CPE-598 interaction
`apply_cli_geometry` (CPE-598) currently positions the **config-created** `main` window in `setup()`. If
window creation moves to Rust, ensure geometry still applies (build the window, then run the same
geometry logic on it) and re-verify `--x/--y/--width/--height` still work. Also preserve the config
window's title / size (1000×700) / minWidth 600 / minHeight 400.

## Acceptance Criteria
- [x] Frontend responses on Windows carry `Cache-Control: no-store` (or `no-cache`), verified in the
      WebView2 devtools Network tab for `index.html`.
- [x] The main window is created with the same title/size/min-size as today, and the CPE-598 CLI geometry
      flags still position it (regression-check `--x 200 --y 150 --width 1100 --height 750`).
- [x] App launches, renders, and the palette/navigation smoke still works in an installed build.
- [x] `npm run check` + cargo build + clippy (both feature modes) clean.

## Notes
Preventive — CPE-553's reported symptom is already fixed (a fresh bundle clears the cache). This closes
the recurrence path for **auto-updates**. Not urgent, but genuinely valuable for the auto-update story.
See [[webview2-cache-survives-reinstall]]. Deferred from same-night implementation because moving window
creation into Rust is a core-startup change that wants careful, attended verification (esp. the CPE-598
geometry interaction) rather than an unattended overnight ship.

## Work Log
2026-07-18 (nightshift, attended) — Picked up (top Backlog item). Estimate 2-3h. De-risked the two
interactions: window-state plugin restores via on_window_ready (fires for Rust-created windows too, so
CPE-228 keeps working); using skip_initial_state("main") + explicit restore_state before apply_cli_geometry
for deterministic restore→CLI-override ordering (CPE-598). Confirmed WebviewWindowBuilder.on_web_resource_request
exists in tauri 2.11.5.

2026-07-18 — Implemented: removed the declarative window from tauri.conf.json (`windows: []`) and create
`main` in `setup()` via `WebviewWindowBuilder` with `.on_web_resource_request` injecting
`Cache-Control: no-store` (title/size 1000x700 / min 600x400 preserved). Added `.skip_initial_state("main")`
to the window-state plugin and restore geometry explicitly before `apply_cli_geometry`, so restore→CLI
override ordering is deterministic (CPE-228 + CPE-598 both preserved).

Verified (local `tauri build --no-bundle`, attended): app launches + renders (window title correct,
Responding=True); CLI geometry regression `--x 200 --y 150 --width 1100 --height 750` → outer rect
left=200 top=150, inner ~1100x750 (outer 1116x789 incl. borders) — exact. `npm`/cargo/clippy (both feature
modes) clean.

Residual manual check: the `Cache-Control: no-store` header on `index.html` wasn't inspected in the
WebView2 devtools Network tab (release builds ship devtools off). The mechanism is the documented Tauri
`on_web_resource_request` hook and compiles/runs; confirm in devtools on a dev/devtools-enabled build if
desired.

## Resolution
Made the Windows/WebView2 frontend no-cache to close the auto-update stale-bundle path (a cached, unhashed
`index.html` pinning old hashed JS). Because Tauri exposes the resource hook only on the window builder —
not app-level, and `HeaderConfig` has no Cache-Control field — the main window is now created in Rust in
`setup()`, where `on_web_resource_request` sets `Cache-Control: no-store` on every frontend response
(local assets, so the perf cost is nil). Preserved the two startup interactions: window-state restore
(CPE-228) via explicit `restore_state` + `skip_initial_state("main")`, and CLI geometry (CPE-598) applied
after restore. Files: src-tauri/tauri.conf.json, src-tauri/src/lib.rs.
