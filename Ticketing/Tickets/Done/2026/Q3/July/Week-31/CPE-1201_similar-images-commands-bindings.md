---
id: CPE-1201
title: "Tauri commands + specta bindings for the image-similarity scan"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Part of CPE-997. Expose CPE-1200's scan to the frontend, mirroring the exact-duplicate commands.

## Build
- Thin `spawn_blocking` dispatchers in `src-tauri/src/lib.rs`: `find_similar_images(root)` + streaming
  `find_similar_images_stream(root, on_group: ipc::Channel<Vec<SimGroup>>)`, modelled on
  `find_duplicates`/`find_duplicates_stream` (~lib.rs:3198/3211). Register in BOTH `generate_handler!` blocks.
- Add `specta::Type` to `SimGroup`/result structs; **regenerate `bindings.gen.ts`** (drift guard —
  [[regen-specta-bindings-on-struct-change]]). Async + spawn_blocking (never sync fs — [[async-all-blocking-commands]]).

## Acceptance Criteria
- [ ] `npm run check` clean; `bindings.gen.ts` contains the new commands; backend builds + clippy clean
      (both feature modes); a `HeadlessCtx`/unit exercise of the collect command returns groups for a fixture.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-997). Depends on CPE-1200; same worker, sequential.
  Serialize lib.rs/bindings with CPE-1204 if that runs concurrently.
- 2026-08-01 — Added two thin `spawn_blocking` dispatchers in `src-tauri/src/lib.rs` (next to
  `find_duplicates`/`find_duplicates_stream`): `find_similar_images(root)` and
  `find_similar_images_stream(root, on_group: ipc::Channel<Vec<SimGroup>>)`, both async + `spawn_blocking`
  (never sync fs). Registered in BOTH `generate_handler!` (the `.invoke_handler` block) and the
  `collect_commands!` block used by the `export_bindings` bin.
- Added `specta::Type` (via the crate's `feature = "specta"` gate) to `SimGroup` and `SimResult` in
  `image_similarity.rs`. Regenerated `src/lib/bindings.gen.ts` with
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` — +40 lines: the new
  `findSimilarImages` / `findSimilarImagesStream` clients + `SimGroup`/`SimResult` types (drift guard
  will pass; only additions).
- src-tauri unit test `find_similar_images_collect_groups_a_fixture` exercises the collect path on an
  embedded 81-byte gradient PNG (two identical copies group; a `.txt` is filtered; non-folder root is
  `Err`) — no `image` dev-dep added to src-tauri.
- Verified: `cargo test -p cpe-server` (1155 green); `src-tauri` `cargo test` incl. the new test + the
  bindings-drift guard test; `cargo clippy --all-targets -D warnings` clean on both crates in default +
  `--features index` (cpe-server) / `--features sidecar-platform` (src-tauri); `npm run check` 0/0.
