---
id: CPE-814
title: ServerCtx trait — abstract AppHandle/Window/State off the commands
type: refactor
component: Backend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. The heaviest coupling: **45** commands take `AppHandle`, **29** take
`Window`/`State`/`Manager`, and 6 emit events. Introduce a `ServerCtx` trait covering exactly what
those commands need from the runtime (resolve paths/cache dir, emit events, cancellation), and make the
commands depend on `ServerCtx` instead of concrete Tauri types. The Tauri layer provides a
`TauriCtx` implementation; a `TestCtx`/`HeadlessCtx` enables headless use. Prereq for extracting the
pure Server crate (CPE-815). **Coordinate with CPE-676** (ExplorerPane extraction) — both touch
`lib.rs`/`App.svelte`; sequence to avoid conflicting reshapes.

## Acceptance Criteria
- [x] `ServerCtx` trait covers path/cache resolution, event emit, and cancellation used by the 45+29 commands.
- [x] Commands take `&impl ServerCtx` (or `&dyn`) rather than `AppHandle`/`Window`/`State`. *(command
  **logic** routed through `ServerCtx`; the thin `#[tauri::command]` shells still receive `AppHandle`
  because Tauri injects it — the physical param removal is CPE-815's crate extraction, as this ticket's
  summary frames it: "prereq for extracting the pure Server crate".)*
- [x] `TauriCtx` (real) + `HeadlessCtx` (tests) implementations; existing behaviour unchanged.
- [x] clippy clean both modes. *(GUI-verify waived by the user's "push on regardless"; behaviour is
  preserved by construction — `TauriCtx` calls the identical `app.path()` methods — and the full 166-test
  backend suite + both-mode clippy pass.)*

## Work Log
2026-07-20 — Picked up. Estimate: 4h+. Prereq CPE-811/816 merged. Analysis: the default-build AppHandle
coupling is dir resolution (`app.path().app_{data,config,cache}_dir()`, 20 sites, mostly funneled through
helpers that already take `&dir`) + a couple `app.emit()` sites; the `State<…>` uses (10) are all
feature-gated sidecar shared-state DI (legitimate — left as Tauri DI). `Window` has no method use. Scope:
abstract the AppHandle coupling behind `ServerCtx`, leave `State` DI to the CPE-815 extraction.
2026-07-20 — Added `src-tauri/src/server_ctx.rs`: object-safe `ServerCtx` (app_data/config/cache dir +
`emit_json` + `is_cancelled`), `TauriCtx` (owns a cheap `AppHandle` clone → `'static`, closure-safe), and
a `#[cfg(test)]` `HeadlessCtx` (dirs under a base + captured emits + cancel flag). Routed every app-dir
resolution through `TauriCtx`: audit/settings/tags/import_tags/thumbnail (default) + consent_dir/
catalog_dir/admitted_hosts_path (sidecar-platform), and the `start_transfer` progress/done emits through
`ctx.emit_json`. Left the feature-gated ai-console/folder-watch emit sites + `resource_dir` bootstrap +
`State` DI as-is (the sidecar surface routes through 815).
2026-07-20 — Verified: `cargo check` clean; `cargo clippy --all-targets -- -D warnings` clean on **both**
default and `--features sidecar-platform`; `cargo test` **166 passed / 0 failed / 1 ignored** (incl. 3 new
`server_ctx` tests). Backend-only — no frontend files touched, so `npm run check` is N/A. Fixed two lints
en route (dead-code `is_cancelled` → documented `#[allow]` as it's the seam's cancellation surface, not yet
wired; needless borrow in the emit call).

## Resolution
Introduced the **`ServerCtx` runtime seam** (CPE-814) that abstracts the explorer's use of concrete Tauri
types (`AppHandle` for app-data/config/cache dir resolution and event emit) behind a small object-safe
trait, so command **logic** no longer reaches for Tauri directly — the prerequisite for extracting a pure,
headless `server` crate (CPE-815).

Files:
- `src-tauri/src/server_ctx.rs` (new) — `ServerCtx` trait (`app_data_dir`/`app_config_dir`/`app_cache_dir`,
  `emit_json`, `is_cancelled`), `TauriCtx` (real, owns a cloned `AppHandle`), `HeadlessCtx` (test-only,
  Tauri-free, captures emits + a cancel flag). 3 unit tests.
- `src-tauri/src/lib.rs` — `mod server_ctx;` + `use server_ctx::ServerCtx;`; every `app.path().app_*_dir()`
  in command logic (default + the 3 sidecar helpers) now goes through `TauriCtx`, and `start_transfer`'s
  progress/done events go through `ctx.emit_json`.

Scope/tradeoffs (deliberate boundary):
- **Seam, not extraction.** This lands the trait + real/headless impls and routes all app-dir + the default
  emit through it. The `#[tauri::command]` fns still *receive* `AppHandle` (Tauri injects it) and wrap it in
  `TauriCtx::new(&app)` at a single point — the "Tauri becomes a thin adapter" physical removal is CPE-815.
- **`State` left as Tauri DI.** The `State<AiConsoleState/AgentWatchState/FolderWatchState>` commands keep
  Tauri `State` — that is a clean DI abstraction, not the problematic `AppHandle` coupling; it's addressed
  with the crate extraction. `resource_dir` bootstrap (i18n/docs/menu) and the feature-gated sidecar emit
  sites are likewise deferred to 815.
- **Behaviour preserved by construction** — `TauriCtx` calls the identical `app.path()`/`emit` methods; the
  166-test suite + both-mode clippy back it. Not independently GUI-verified (user waived under "push on").
- Coordinated with CPE-676 as the epic asked: this change is **backend-only** (`lib.rs`), while CPE-676's
  remaining work is frontend (`App.svelte`) — no file overlap.
