---
id: CPE-608
title: Serve the frontend no-cache on Windows so auto-updates always refresh
type: enhancement
component: Backend
priority: medium
status: Open
tags: ready
estimate: 2-3h
created: 2026-07-18
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
- [ ] Frontend responses on Windows carry `Cache-Control: no-store` (or `no-cache`), verified in the
      WebView2 devtools Network tab for `index.html`.
- [ ] The main window is created with the same title/size/min-size as today, and the CPE-598 CLI geometry
      flags still position it (regression-check `--x 200 --y 150 --width 1100 --height 750`).
- [ ] App launches, renders, and the palette/navigation smoke still works in an installed build.
- [ ] `npm run check` + cargo build + clippy (both feature modes) clean.

## Notes
Preventive — CPE-553's reported symptom is already fixed (a fresh bundle clears the cache). This closes
the recurrence path for **auto-updates**. Not urgent, but genuinely valuable for the auto-update story.
See [[webview2-cache-survives-reinstall]]. Deferred from same-night implementation because moving window
creation into Rust is a core-startup change that wants careful, attended verification (esp. the CPE-598
geometry interaction) rather than an unattended overnight ship.
