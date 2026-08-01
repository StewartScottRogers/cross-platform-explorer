---
id: CPE-1201
title: "Tauri commands + specta bindings for the image-similarity scan"
type: feature
component: Backend
priority: medium
status: Doing
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
