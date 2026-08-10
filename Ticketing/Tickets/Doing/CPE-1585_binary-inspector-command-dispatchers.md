---
id: CPE-1585
title: "Binary Inspector: Tauri command dispatchers + specta bindings for binary_info / disassembly"
type: Task
status: In Progress
priority: Medium
component: Backend
epic: CPE-1562
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1562 slices 1 (CPE-1572, `BinaryInfo` DTO/inspector) and 2 (CPE-1581, x86/x64 disassembly via
`iced-x86`) landed **inside `crates/server` only** — there is no `#[tauri::command]` dispatcher and no typed
frontend binding, so the frontend literally cannot call either. Slice 4 (the Binary Inspector tabbed preview
provider) is blocked on this wiring.

## Goal
Expose the existing `cpe-server` binary-inspection surface to the frontend as thin, typed, async commands.

## Scope
- Add `#[tauri::command]` dispatchers in `src-tauri/src/lib.rs` — **one-line thin dispatchers** into
  `cpe-server` per SERVER-ARCHITECTURE.md; register them in `generate_handler![]`.
  - `binary_info(path)` → the structured `BinaryInfo` DTO (overview/sections/imports/exports/symbols).
  - `binary_disasm(path, ...)` → the capped disassembly listing produced by the CPE-1581 work.
  - Match the existing signatures in `crates/server/src/binary_preview.rs` — do **not** redesign the DTOs.
- **Async + `spawn_blocking`** — these are filesystem/CPU-heavy; a sync command freezes the main thread
  ([[async-all-blocking-commands]]).
- **Regenerate `src/lib/bindings.gen.ts`** (specta) and confirm the CI Typed-bindings drift guard passes
  ([[regen-specta-bindings-on-struct-change]]).
- Any capability needed goes in `src-tauri/capabilities/default.json`.
- Tests: keep/extend the `cpe-server` fixture tests; add a smoke test that the commands are registered.

## Out of scope
The frontend Binary Inspector provider/tabbed view (epic slice 4 — separate ticket) and .NET metadata
(slice 3 — separate ticket).

## Acceptance criteria
- `binary_info` and `binary_disasm` are callable from the frontend via typed bindings.
- `cargo clippy --all-targets -D warnings` clean in **both** feature modes; `cargo test` green.
- `npm run check` green; `bindings.gen.ts` regenerated + committed (drift guard green).
- If a Cargo.lock changes, **both** lockfiles regenerated ([[multiple-independent-cargo-locks]]).

## Notes
Model: sonnet. Conflict surface: `src-tauri/src/lib.rs`, `src/lib/bindings.gen.ts`,
`src-tauri/capabilities/default.json`. Do not touch `src/lib/preview/*` or `App.svelte` (other workers).
