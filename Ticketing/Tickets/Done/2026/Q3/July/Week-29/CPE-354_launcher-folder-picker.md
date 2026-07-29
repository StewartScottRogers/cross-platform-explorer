---
id: CPE-354
title: "AI Console launcher: native folder picker for the Working folder box"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Multiple
created: 2026-07-13
---

## Summary

The launcher's "Working folder" is a bare text box. Add a **Browse…** button that opens a
native OS folder dialog and fills the path. The launcher is a sandboxed webview with no Tauri
API, so it routes through the host: launcher → sidecar `/api/pick-folder` → broker
`host.pick_folder` → the host servicing loop opens the Tauri folder dialog (on the main
thread) and returns the chosen path.

## Scope
- `broker_client.rs`: `HostDialogs` trait + `BrokerDialogs` (calls `host.pick_folder`) +
  `NoopDialogs` (dev fallback → cancelled).
- `console.rs`: `dialogs` on `ConsoleState`; `POST /api/pick-folder` → `{ path }`.
- `main.rs`: wire `BrokerDialogs`.
- `src-tauri`: the servicing loop intercepts the `host.pick_folder` method (not a broker
  capability) and opens `app.dialog().file().pick_folder(..)`, returning the path or null.
- `launcher.html`: Browse button next to the cwd input.

## Acceptance
- Clicking Browse opens the OS folder dialog; choosing a folder fills the Working folder box;
  cancel leaves it unchanged. `cargo test`/`clippy` clean.

## Work Log
2026-07-13 — User asked for a folder-pick dialog instead of a plain text box. Building.

2026-07-13 — Implemented on branch `CPE-354-folder-picker`.
- `broker_client.rs`: `HostDialogs` trait + `BrokerDialogs` (`host.pick_folder`, 5-min wait
  via new `request_timeout`) + `NoopDialogs` dev fallback.
- `console.rs`: `dialogs` on `ConsoleState`; `POST /api/pick-folder` → `{ path }`.
- `main.rs`: wires `BrokerDialogs`.
- `src-tauri`: the servicing loop intercepts `host.pick_folder` and calls
  `app.dialog().file().pick_folder(..)` on the main thread, blocking the servicing thread on a
  channel until the user chooses; returns the path (or null on cancel).
- `launcher.html`: a **Browse…** button beside the Working-folder input.

ai-console `cargo test` 106 lib + 7 integration, `clippy` clean; src-tauri builds/tests
(`--features sidecar-platform`) 61, clippy clean. The dialog interaction needs a GUI eyeball —
the plumbing is compiled/tested end-to-end.
