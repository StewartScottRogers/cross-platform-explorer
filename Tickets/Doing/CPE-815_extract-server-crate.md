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
2026-07-20 — **Slice 7:** extracted the disk-usage domain — `dir_size` + `dir_children_sizes` +
`dir_size_walk` + `ChildSize` (CPE-749/754) → `cpe_server::disk_usage` (moved the `rayon` dep with it,
and **dropped `rayon` as a direct app dependency** since nothing else in the app used it). Both commands
are now dispatchers. Tests moved (the symlink-cycle regression test kept in the app, repointed to
`cpe_server::disk_usage::dir_size`). Verified: `cpe-server` **61 tests** green; app `cargo test` **108
passed / 0 failed**; clippy clean **both** modes. `cpe-server` now holds **15 modules**.
2026-07-20 — **Slice 8:** extracted the folder-tree `scan_tree` domain (TreeNode + scan_children,
CPE-779, compare view) into `cpe_server::compare`; the `scan_tree` command dispatches. Tests moved.
Verified: `cpe-server` **63 tests** green; app `cargo test` **106 passed / 0 failed**; clippy clean
**both** modes. **8 slices landed** — the standalone modules + the light/self-contained command domains
(tags, settings, checksum, text-stats, folder-stats, compare, disk-usage) are now in `cpe-server`. The
remaining `lib.rs` bulk is the heavy-dependency command groups (previews via image/office/pdf, archives
via zip/tar/7z, DB via rusqlite/parquet, content/name search, and the 3 `ipc::Channel` streamers — the
streamers wait on CPE-819's network-streaming equivalent).
2026-07-20 — **Slice 9:** extracted the duplicate-finder domain (DupGroup/DupResult + size-then-hash
scan, CPE-420) → `cpe_server::duplicates` (leans on the extracted `fsutil` `sha256_file`/
`entry_is_symlink`); the `find_duplicates` command dispatches. Dedicated test moved; the symlink-cycle
regression test kept in the app, repointed to `cpe_server::duplicates::find_duplicates` (exposed
`DupResult` fields `pub` for that cross-crate read). Verified: `cpe-server` **64 tests** green; app
`cargo test` **105 passed / 0 failed**; clippy clean **both** modes. `cpe-server` now holds **16 modules**.
2026-07-20 — **Slice 10 (streaming split):** extracted the filename-search domain (glob/`?`/`*` +
brace-group matching + the shared `walk_name_matches` walker + types, CPE-603/697/666) →
`cpe_server::name_search`. Key pattern: the walker takes a `flush(Vec<NameMatch>) -> ControlFlow`
callback, so the collect command dispatches to `find_files_by_name` while the **streaming** command
keeps its `tauri::ipc::Channel` in the app adapter and feeds the same extracted walker — the transport
stays in the app, the walk is pure. Ported the thorough brace-matching test suite into the crate.
Verified: `cpe-server` **70 tests** green; app `cargo test` **100 passed / 0 failed**; clippy clean
**both** modes. `cpe-server` now holds **17 modules**. (Proves the streaming-liveness convention survives
the extraction — the groundwork CPE-819 generalizes over the wire.)
2026-07-20 — **Slice 11:** extracted the content-search domain (ContentMatch/ContentSearchResult +
bounds + `looks_binary` + the recursive line search, CPE-416) → `cpe_server::content_search`; the
`search_file_contents` command dispatches. Dedicated test moved; the symlink-cycle test repointed. This
retired the last app user of `entry_is_symlink`, so dropped it from the app's fsutil re-export.
Verified: `cpe-server` **71 tests** green; app `cargo test` **99 passed / 0 failed**; clippy clean
**both** modes. `cpe-server` now holds **18 modules**. Both "search" domains (name + content) are now
extracted; remaining `lib.rs` bulk is the heavy-dep groups (previews/archives/DB), `list_dir` (core +
streaming), the backup engine, and properties/metadata.
2026-07-20 — **Slice 12 (shared model types):** extracted the core filesystem model — `DirEntry`,
`EntryInfo`, `Place`, `OpResult` (+ its `ok`/`err` constructors) and the `extension_of` / `is_hidden`
(platform-cfg) helpers — into `cpe_server::model`, re-exported into the app via
`use cpe_server::model::{…};` so all construction sites (fields made `pub`) + the 26 `OpResult::` call
sites resolve unchanged. This is the foundation the remaining file-op/`list_dir`/properties extractions
key off. Verified: `cpe-server` **74 tests** green; app `cargo test` **97 passed / 0 failed**; clippy
clean **both** modes (fixed a Windows-only unused-var in the ported `is_hidden` test). `cpe-server` now
holds **19 modules**.
2026-07-20 — **Slice 13 (core listing):** extracted `list_dir` — the app's beating heart — into
`cpe_server::listing`: the pure `dir_entry_from` mapper (uses the extracted `model` +
`fsutil::to_epoch_ms`), `LIST_DIR_BATCH`, the shared `stream_dir_entries` walker, and a `list_dir`
collect helper. Same **streaming split** as name-search: `list_dir` dispatches; the streaming
`list_dir_stream` command keeps its `ipc::Channel` **and** the `DIR_STREAM_CANCELS` cancel registry in
the app adapter and feeds the extracted walker. Walker tests moved; the `cancel_dir_stream` registry
test stays in the app. Verified: `cpe-server` **79 tests** green; app `cargo test` **92 passed / 0
failed**; clippy clean **both** modes. `cpe-server` now holds **20 modules**.
2026-07-20 — **Slice 14:** extracted the Properties `entry_info` into `cpe_server::model::entry_info`
(uses the extracted `EntryInfo`/`is_hidden` + `fsutil::to_epoch_ms`); the `entry_info` command
dispatches. Test moved. Verified: `cpe-server` **80 tests**; app `cargo test` **91 passed / 0 failed**;
clippy clean **both** modes. **This exhausts the clean, pure-domain extractions.** Remaining in
`lib.rs`: heavy-dependency **previews** (image/office/pdf/rusqlite/parquet/…) and **archives**
(zip/tar/7z) — pulling those into `cpe-server` is a deliberate design decision — plus OS-coupled bits
(file ops via `trash`, `special_folders`, `drive_type`, link forge, thumbnails) that arguably belong in
the Tauri adapter.
2026-07-20 — **Slice 15:** extracted the link forge (`create_symlink` + `create_hard_link`, CPE-802) into
`cpe_server::links` — pure `std::os` cfg branches, no new deps (OS-specific but filesystem *domain*
logic; the 3-OS CI compiles both branches). Both commands dispatch. Tests moved. Verified: `cpe-server`
**82 tests** green; app `cargo test` **90 passed / 0 failed**; clippy clean **both** modes. `cpe-server`
now holds **21 modules**.
2026-07-20 — **Slice 16 (first heavy-dep domain):** extracted the archive-listing domain (`ArchiveEntry`
+ zip/tar/gzip/7z/iso listers + the extension dispatcher, CPE-064/109/110/113) into `cpe_server::archive`,
pulling the pure-Rust `zip`/`tar`/`flate2`/`sevenz-rust`/`iso9660` crates into the Server (per the "keep
going + pull heavy crates in" decision). The `read_archive_entries` command dispatches; archive tests
repointed. **Slimmed the plain explorer:** dropped the now-app-unused `iso9660` dep from `src-tauri`.
Verified: `cpe-server` **84 tests** green; app `cargo test` **90 passed / 0 failed**; clippy clean **both**
modes (removed the now-unused app `use serde::Serialize`). `cpe-server` now holds **22 modules**.
