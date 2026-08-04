---
id: CPE-1299
title: "Wire streaming scan commands (ipc::Channel) + bindings"
type: feature
component: app
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
Integration pass for the streaming scan walkers (CPE-1294/1295/1296): add `*_stream` `ipc::Channel` commands
in the app adapter that drive each walker's flush callback over a Channel, with generation-token supersede +
cancel, mirroring the existing `list_dir_stream`. Kept as ONE ticket so the shared `src-tauri/src/lib.rs`
handler list + bindings are touched once.

## Build
- For each streaming walker, add a `find_type_mismatches_stream` / `find_dangling_links_stream` /
  `find_orphan_sidecars_stream` `#[tauri::command]` in `src-tauri/src/lib.rs` that takes an
  `ipc::Channel<Batch>` (+ a generation token) and calls the `cpe-server` walker with a `flush` that emits
  each batch over the Channel, honoring supersede/cancel like `list_dir_stream` / `find_duplicates_stream`.
- Register in both `generate_handler![]` lists; regen `src/lib/bindings.gen.ts` (drift guard); capability
  entries if needed (likely none — core commands).
- A smoke test that each stream command drives its walker and emits batches.

## Acceptance criteria
- The three `*_stream` commands stream batches over an `ipc::Channel`; bindings regenerated zero-drift;
  `cargo test` + `npm run check` green; clippy clean both feature modes.

## Notes
Prereq: CPE-1294/1295/1296 merged. `ipc::Channel` stays in the app adapter (per CLAUDE.md). Epic CPE-1002;
streaming-liveness convention (STREAMING.md).

## Work Log
- 2026-08-03 — 3 stream commands + 3 cancel commands + bindings merged (#599). Reviewer APPROVE with rigor: wrote throwaway test proving cancel BREAKS the walk (scanned 50/250, registry cleaned no leak), re-ran export_bindings + git diff --exit-code = ZERO drift, all 6 cmds in BOTH generate_handler+collect_commands, empty-collection tail convention, real Channel smoke tests, both feature modes build+clippy+test+npm check green. Non-blocking: no cancel-specific unit test (small follow-up).
