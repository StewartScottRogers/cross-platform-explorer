---
id: CPE-1287
title: "Wire safety-scan commands + regen bindings (integration)"
type: feature
component: app
priority: medium
status: Doing
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
Integration pass for the Shift-1 `cpe-server` scan adapters (CPE-1281/1282/1283/1284/1285): expose each as a
thin `#[tauri::command]` and regen the typed bindings. Kept as ONE ticket so the shared surfaces
(`src-tauri/src/lib.rs` handler list + `src/lib/bindings.gen.ts`) are touched once, avoiding merge conflicts
across the parallel module workers.

## Build
- For each landed scan adapter, add a thin async `#[tauri::command]` in `src-tauri/src/lib.rs` that
  `spawn_blocking`s into the `cpe-server` fn (per the async-all-blocking-commands rule) and register it in
  BOTH `generate_handler![]` lists (runtime + specta/test list).
- Regenerate `src/lib/bindings.gen.ts` (the specta drift guard fails otherwise).
- Add any needed capability entries in `src-tauri/capabilities/default.json` (these are read-only fs scans;
  likely none beyond existing fs perms — verify).
- A small smoke test that each command dispatches into its adapter.

## Acceptance criteria
- `analyze_archive_safety`, `find_empty_dirs`, `find_orphan_sidecars`, `find_dangling_links`, and
  `find_type_mismatches` are callable from the frontend via `commands.*`; bindings regenerated with zero
  drift; `cargo test` + `npm run check` green; clippy clean both feature modes.

## Notes
Prereq: CPE-1281–1285 merged. Feeds the frontend "File Health" model (CPE-1288 / T7). Epic CPE-1002.

## Work Log
