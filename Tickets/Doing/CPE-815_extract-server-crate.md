---
id: CPE-815
title: Extract a pure `server` crate (Tauri-free domain logic)
type: refactor
component: Backend
priority: medium
status: In Progress
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 4h+
---

## Summary
Child of CPE-810. Move the domain logic (the ~9k-line command bodies) out of the Tauri app into a
standalone `server` crate that depends only on `ServerCtx` (CPE-814) and the `contract` envelope
(CPE-811) — no Tauri. The Tauri app becomes a **thin adapter**: register handlers, build `TauriCtx`,
dispatch envelopes to the server. This is what makes the Server runnable headless and, later, remote.
Prereqs: CPE-814, CPE-811.

## Acceptance Criteria
- [ ] `server` crate holds the command logic; depends on `contract` + `ServerCtx`, not `tauri`.
- [ ] Tauri app is a thin shim dispatching to the server crate; local behaviour byte-for-byte unchanged.
- [ ] The server crate builds and its logic is unit-testable headless (no Tauri runtime).
- [ ] Local explorer no slower (spot-check vs benchmark); clippy clean both modes; GUI-verified.

## Work Log
2026-07-20 — Picked up (attended; user available for GUI checkpoints). Estimate: 4h+. Prereqs CPE-814
(ServerCtx seam) + CPE-811 (contract) merged. This is a multi-slice extraction; landing slice 1 green
before migrating command bodies.
2026-07-20 — **Slice 1 (foundation):** created `crates/server` (pkg `cpe-server`, Tauri-free, depends on
`cpe-contract` + serde). Moved the runtime seam and the filesystem-domain core out of the app:
  - `ServerCtx` **trait** + `HeadlessCtx` → `cpe-server/src/ctx.rs` (the app keeps only `TauriCtx`, now
    `impl cpe_server::ctx::ServerCtx`).
  - `location.rs` + `provider.rs` (already pure, Tauri-free) → `cpe-server/src/{location,provider}.rs`.
  - `cpe-server` re-exports `cpe_contract as contract`, establishing the GUI/adapter → Server → contract
    dependency direction.
  `src-tauri` now depends on `cpe-server` (path dep; pulls only already-present serde/serde_json/contract,
  so zero weight added to the plain explorer) and imports the `ServerCtx` trait from it. CI `Server crates`
  job extended to lint+test `crates/server` on all 3 OSes.
  Verified: `cpe-server` 12 tests green, clippy clean; app `cargo test` **154 passed / 0 failed**, clippy
  `-D warnings` clean on **both** default and `--features sidecar-platform`. Behaviour-preserving (moved
  modules were runtime-unreferenced; `TauriCtx` unchanged). **Remaining (later slices): migrate the ~117
  command bodies' domain logic into `cpe-server` behind `ServerCtx`, leaving the `#[tauri::command]` fns as
  thin dispatchers.** Ticket stays In Progress until that bulk lands + GUI-verified.
2026-07-20 — **Slice 2 (first real command-body migration):** moved the tag-store domain (CPE-635/614)
into `cpe-server/src/tags.rs` — the `TagEntry`/`TagStore` model + all pure helpers + `read/write` +
new `ServerCtx`-based entry points (`load`/`set`/`counts`/`rename_tag`/`delete_tag`/`retag`/`import`).
The 7 Tauri tag commands (`load_tags`/`set_tags`/`tag_counts`/`rename_tag`/`delete_tag`/`retag_path`/
`import_tags`) are now **one-line dispatchers** (`cpe_server::tags::…(&TauriCtx::new(&app))`). Moved the
6 model tests + added a ctx-entry-point test (over `HeadlessCtx`). Proves the "app is a thin shim
dispatching to the server crate" AC for a real, GUI-facing command group.
  Verified: `cpe-server` **19 tests** green; app `cargo test` **148 passed / 0 failed**; clippy
  `-D warnings` clean on **both** modes. Behaviour-preserving (identical helpers/JSON; `ServerCtx`
  resolves the same config dir). **Awaiting user GUI smoke-test of Tags** (assign/label/filter/import)
  before merge, then the remaining command groups migrate the same way.
2026-07-20 — **Slice 3:** moved the **settings** domain (CPE-226) into `cpe-server/src/settings.rs`
(pure helpers + `load`/`save` ctx entry points; `read_settings`/`write_settings` now dispatchers), and
relocated the two already-pure standalone modules **`geometry`** (CPE-598) and **`audit_journal`**
(CPE-800) into `cpe-server`, re-exported into the app via `use cpe_server::{audit_journal, geometry};`
so existing call sites resolve unchanged. Verified: `cpe-server` **39 tests** green; app `cargo test`
**129 passed / 0 failed**; clippy `-D warnings` clean on **both** modes. Behaviour-preserving.
2026-07-20 — **Slice 4:** relocated the **`ticket_board`** module (Agent Board backend, CPE-520) into
`cpe-server`, re-exported via `use cpe_server::{audit_journal, geometry, ticket_board};` (all 19 board
call sites resolve unchanged). Added `tempfile` as a `cpe-server` dev-dep (moved with the module's
tests). Verified: `cpe-server` **50 tests** green; app `cargo test` **118 passed / 0 failed**; clippy
clean **both** modes. Behaviour-preserving.
2026-07-20 — **Slice 5 (shared-helper untangle):** extracted the shared FS utils into
`cpe-server/src/fsutil.rs` — `to_epoch_ms` (7 call sites) + `sha256_file` (5 call sites), re-exported
into the app via `use cpe_server::fsutil::{sha256_file, to_epoch_ms};` so every existing call resolves
unchanged (dropped the now-unused `UNIX_EPOCH` import). Then moved the two self-contained domains that
depended on them: `text_stats` (CPE-414) → `cpe_server::text_stats` and file/folder `checksum` (CPE-412/
791) → `cpe_server::checksum`; their 3 Tauri commands (`text_stats`/`hash_file`/`checksum_folder`) are
now dispatchers. Moved/ported the epoch-ms, text-stats, checksum, and hash_file tests. Added `sha2` to
`cpe-server` deps. Verified: `cpe-server` **57 tests** green; app `cargo test` **113 passed / 0 failed**;
clippy `-D warnings` clean **both** modes. Behaviour-preserving (identical helpers/JSON). `cpe-server`
now holds 12 modules.
2026-07-20 — **Slice 6:** moved the shared `entry_is_symlink` helper (7 call sites) into
`cpe_server::fsutil` (re-exported), then extracted `folder_stats` (CPE-649) → `cpe_server::folder_stats`
and byte-identical `files_identical` (CPE-418) → `cpe_server::compare`; both commands are now
dispatchers. Tests moved. Verified: `cpe-server` **59 tests** green; app `cargo test` **111 passed /
0 failed**; clippy clean **both** modes. `cpe-server` now holds **14 modules**.
