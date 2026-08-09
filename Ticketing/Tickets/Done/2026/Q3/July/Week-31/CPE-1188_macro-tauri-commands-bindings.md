---
id: CPE-1188
title: "Macro Tauri commands + specta bindings (save/list/load/delete/import/export/plan/run)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Part of CPE-739. **Linchpin** — the macro engine is unreachable from the frontend (`grep macro` in
`src-tauri/src/lib.rs` = 0). Add thin `#[tauri::command]` dispatchers into `macro_store`
(save/list/load/delete/import/export), `action_macro::plan`, and the CPE-1187 executor (`macro_run`, which
bridges to the existing `rename_entry`/`move_exact`/`set_tags`/media-convert primitives), and regenerate
bindings. Everything frontend depends on this.

## Build
- Thin one-line dispatchers in `src-tauri/src/lib.rs` (mirror the `template_*` command pattern), registered in
  `generate_handler!` (+ `collect_commands!` for export). The RUN command must route through the CPE-1187
  resolve+scope-check and record the inverse for undo; imported macros never auto-run.
- **Regenerate `src/lib/bindings.gen.ts`** (specta) — the Typed-bindings drift guard requires it
  ([[regen-specta-bindings-on-struct-change]]).

## Acceptance Criteria
- [x] Compiles + all commands registered; a Rust integration test via `HeadlessCtx` round-trips
      save→list→load and runs a simple macro (with undo restoring state).
- [x] `cargo test` (crates/server + src-tauri incl. the bindings-drift guard) green; `cargo clippy
      --all-targets -D warnings` clean (both modes); `npm run check` green against regenerated bindings.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-739). Depends on CPE-1187; same worker, sequential.
- 2026-07-31 — Added 9 thin `#[tauri::command]` dispatchers in `src-tauri/src/lib.rs` (new "Action macros"
  section, right after the folder-template block, mirroring its `template_*` one-line-dispatcher pattern):
  `macro_save` / `macro_list` / `macro_load` / `macro_delete` / `macro_export` / `macro_import` into
  `cpe_server::macro_store`; `macro_plan` into `action_macro::{validate, plan}`; `macro_run` / `macro_undo`
  bridging the CPE-1187 `macro_run::resolve` plan to real disk work. All 9 registered in both
  `generate_handler!` and `collect_commands!`. `macro_import` only ever writes to the persisted catalog —
  running is always the separate, explicit `macro_run` call, so an imported macro can never auto-run.
  `macro_run` resolves + scope-checks (CPE-1187), then applies each op via `macro_apply_op`: `rename`/`move`
  reuse the existing `rename_entry_impl`/`move_exact_impl` primitives directly (deriving the concrete
  target from the resolved `to` path); `convert` reads the file, re-encodes via
  `cpe_server::batch_transform::apply_ops` (the same engine the Batch-Media dialog uses) and writes the
  result at the resolved path, removing the original (a macro's `Convert` step is in-place from the user's
  perspective, unlike Batch-Media's non-destructive default); `tag`/`untag` read the tag store, union/remove
  the label, and write back via `cpe_server::tags::set`. **All-or-nothing**: `macro_apply_run` rolls back
  every already-applied op (replaying its inverse) the instant one step fails, so a run is never left
  half-done — verified by a dedicated test that forces a mid-run failure (a rename target for a file that
  doesn't exist on disk) and asserts the first, successful step was undone. `macro_undo` replays a
  completed run's inverses in reverse. 4 new `HeadlessCtx`-based integration tests in `src-tauri`'s existing
  `mod tests` (a live `tauri::AppHandle` can't be built in a plain libtest binary here, per the file's own
  Windows-loader note, so — matching every other command test in this file — they exercise the same
  underlying calls the dispatchers make): save→list→load round trip; a tag macro run + undo restoring the
  tag store; a rename+move macro run + undo restoring the original path and file bytes on a real temp
  directory; the rollback-on-partial-failure case. Regenerated `src/lib/bindings.gen.ts` via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — `macroSave` /
  `macroList` / `macroLoad` / `macroDelete` / `macroExport` / `macroImport` / `macroPlan` / `macroRun` /
  `macroUndo` all present in the typed client. `cargo test` green: cpe-server 1131/1131, src-tauri 77/77
  (incl. the bindings-drift guard test). `cargo clippy --all-targets -D warnings` clean: cpe-server default
  + `--features index`; src-tauri default + `--features sidecar-platform`. `npm run check`: 0 errors,
  0 warnings. No new dependencies.
