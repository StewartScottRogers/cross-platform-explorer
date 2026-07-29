---
id: CPE-1044
title: Fix --open delivery — inject via init script (CPE-1043 didn't navigate)
type: bug
component: Multiple
priority: high
status: Done
tags: ready
created: 2026-07-25
epic: CPE-616
estimate: 1h
---

## Summary
CPE-1043 shipped `--open <dir>` but it **didn't actually open the folder** in the real build: the app
landed on Home. Root cause: the frontend fetched the folder via a `startup_dir` **command** gated on
`"__TAURI_INTERNALS__" in window`, and that path didn't fire at launch (the gate was added only to stop
the extra startup `invoke` from perturbing the `App.features` render tests — it was masking, not solving).

**Fix — deliver the folder without a command or gate.** The backend already reads `--open` reliably in
**setup** (verified: the CLI match resolves to the absolute samples path). So resolve it there and inject
it as a synchronous global via `WebviewWindowBuilder::initialization_script`
(`window.__CPE_OPEN_DIR__ = "…"`), which runs before the app's own scripts. The frontend reads that global
**synchronously** at startup — no `invoke`, no Tauri-presence gate, no startup-timing perturbation. In a
plain browser / test env the global is simply absent, so tests are unaffected (the fragile gate and the
`startup_dir` command are removed).

Debugging note: local `debug`/`cargo build --release` runs load the **dev server** (`localhost:1420`), not
bundled assets, so `--open` can only be verified in a real `tauri build` bundle.

## Acceptance Criteria
- [ ] Launching a bundled build with `--open <dir>` opens the explorer at that folder (verified in a
      `tauri build` release, since debug loads localhost).
- [ ] `startup_dir` command + its bindings + the `__TAURI_INTERNALS__` gate are removed; the pure
      `launch::resolve_open_dir` + the `open` CLI arg remain.
- [ ] `cargo test -p cpe-server` + clippy clean both modes; `npm run check` + `npx vitest run` green
      (incl. App.features — now unaffected since there's no startup invoke).

## Work Log
2026-07-25 — Filed after the reinstall showed --open landing on Home. Backend read confirmed correct via a
setup-time diagnostic; switched delivery to an init-script global.

2026-07-25 (attended) — **DONE, merged PR #362.** `--open <dir>` now opens the explorer at the folder
(verified in a local `tauri build` bundle — breadcrumb `…\samples`, listing the sample tree). Fix: backend
resolves `--open` in setup and injects `window.__CPE_OPEN_DIR__` via `initialization_script`; frontend
reads it synchronously and calls `navigate()` (not `loadPath`, which left the view on Home). Removed the
`startup_dir` command + gate. 726 cpe-server tests + full vitest + clippy (all crates) green. Merged at
user direction; independent review was in flight.
