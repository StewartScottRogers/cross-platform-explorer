---
id: CPE-1558
title: "Trash backend: list_trash / restore_trash_items / empty_trash + TrashEntry DTO (Win+Linux)"
type: Task
status: Doing
priority: Medium
component: Backend
epic: CPE-1486
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1486 (browsable in-app Trash) slice 1. The `trash` crate v5 (`trash 5.2.6`) exposes
`os_limited::list/restore_all/purge_all` on **Windows + Linux only** (macOS structurally `cfg`-excluded).
Existing `src-tauri/src/lib.rs` already wraps the crate for undo-last-delete (`delete_to_trash`,
`can_restore_from_trash`, `restore_from_trash`) — extend that, don't reinvent. Per
`docs/design/SERVER-ARCHITECTURE.md`, trash commands **stay in the Tauri adapter** (`lib.rs`), not `cpe-server`;
only the DTO goes in `cpe-server::model`.

## Scope
- Add a serializable `TrashEntry` DTO to `crates/server/src/model.rs` (`id`, `name`, `original_path`,
  `time_deleted`, optional `size`) — plain struct + `Serialize` + `specta::Type`, same shape as `Place`/`OpResult`.
  A pure, unit-testable mapping fn from plain fields (NO `trash` crate dependency in `cpe-server`).
- In `src-tauri/src/lib.rs`, next to the existing trash commands, add three `#[cfg(any(target_os="windows", target_os="linux"))]` commands:
  - `list_trash` — `trash::os_limited::list()` → `Vec<TrashEntry>`, skip-on-map-error like `list_dir`; **stream in
    batches over an `ipc::Channel`** per STREAMING.md (chunk the already-materialized Vec) PLUS a collect-to-vec
    variant for tests.
  - `restore_trash_items(ids: Vec<String>)` — reuse `restore_all` but **loop item-by-item** so a
    `RestoreCollision`/`RestoreTwins` on one item surfaces as a distinguishable per-item `OpResult` error instead of
    aborting the whole batch (mirror `restore_from_trash_impl`).
  - `empty_trash(ids: Option<Vec<String>>)` — `None` = purge everything (`purge_all(list()?)`), `Some` = purge those ids.

## Acceptance criteria
- `list_trash` returns entries with correct `original_path`/`time_deleted`; per-item metadata failures are skipped, not fatal.
- `restore_trash_items` round-trips a probe file and reports a `RestoreCollision` case as a clear per-item error.
- `empty_trash` purges selected or all items.
- All three are `#[cfg(any(windows,linux))]` (a `cfg`, not a runtime `if` — the crate module doesn't exist off Win/Linux).
- Unit tests follow the existing `trash_roundtrip_available()` skip-don't-fail pattern for CI runners with no Recycle Bin (CPE-1268).
- `cargo build`, `cargo test`, `clippy --all-targets -D warnings` (both feature modes) green. NO new Cargo dependency.

## Notes
Bindings/handler registration is the NEXT slice (CPE-1559) — this slice adds the commands + DTO + tests; wiring
`generate_handler!` + regenerating bindings happens there. Solo-safe (isolated to trash-owning code + additive DTO).
Model: sonnet.
