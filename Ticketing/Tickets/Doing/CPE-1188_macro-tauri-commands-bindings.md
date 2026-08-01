---
id: CPE-1188
title: "Macro Tauri commands + specta bindings (save/list/load/delete/import/export/plan/run)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [ ] Compiles + all commands registered; a Rust integration test via `HeadlessCtx` round-trips
      save→list→load and runs a simple macro (with undo restoring state).
- [ ] `cargo test` (crates/server + src-tauri incl. the bindings-drift guard) green; `cargo clippy
      --all-targets -D warnings` clean (both modes); `npm run check` green against regenerated bindings.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-739). Depends on CPE-1187; same worker, sequential.
