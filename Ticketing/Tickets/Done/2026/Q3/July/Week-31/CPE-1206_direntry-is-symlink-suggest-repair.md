---
id: CPE-1206
title: "Backend: is_symlink on DirEntry (no extra syscall) + suggest_repair command"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (foundation). Link CREATE (symlink/hardlink) + `link_status` already ship. The listing doesn't
flag links, and `links::suggest_repair` (broken-link basename search) is a pure fn with no command. Add both.

## Build
- Add `is_symlink: bool` to `DirEntry` (`crates/server/src/model.rs`), sourced from the `file_type()` ALREADY
  read during `list_dir` — **no extra syscall per entry** (critical for the "no measurable listing cost when a
  folder has no links" DoD). Regen `bindings.gen.ts`.
- Add a thin `suggest_repair` Tauri command (dispatcher over `links::suggest_repair`) + binding.
- Target resolution stays LAZY (per CPE-1208, on badge render) — do NOT add link-target resolution to the hot
  `list_dir` path.

## Acceptance Criteria
- [x] `cargo test -p cpe-server`: a listed symlinked entry has `is_symlink=true`, a plain file false (Windows-
      unprivileged skip pattern); `suggest_repair` returns the found path. Async + spawn_blocking.
- [x] clippy clean both modes; bindings regenerated (drift guard green); `npm run check` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Foundation; land first (1208/1209 depend on it).
- 2026-08-01 — Implemented (Worker). **`is_symlink` on `DirEntry`** (`crates/server/src/model.rs`): new
  required `bool` field, doc-commented that it's sourced without an extra syscall. Two construction sites:
  - `crates/server/src/listing.rs::dir_entry_from` (the hot `list_dir`/`list_dir_stream` walker) — sourced
    from `meta.file_type().is_symlink()` off the SAME `entry.metadata()` call already made per entry
    (`fs::DirEntry::metadata()` does not follow symlinks, unlike `fs::metadata()`), so listing cost is
    unchanged whether or not a folder contains links.
  - `src-tauri/src/lib.rs::entry_for_path` (the arbitrary-path smart-folder stat, not the hot walk) — this
    one already calls the *following* `fs::metadata()`, so `is_symlink` needed a separate
    `fs::symlink_metadata()` call; documented inline that the extra syscall is scoped to this per-row
    smart-folder path, not the listing hot path the DoD targets.
  - New backend test `list_dir_flags_symlinks_and_leaves_plain_files_unflagged` (`crates/server/src/listing.rs`):
    plain file → `is_symlink=false` always asserted; symlink → `is_symlink=true` asserted only when creation
    succeeds (Windows-unprivileged skip pattern, matching `links.rs`).
  - **`suggest_repair` command** (`src-tauri/src/lib.rs`): thin async `spawn_blocking` dispatcher over the
    already-tested `cpe_server::links::suggest_repair` (5 existing tests cover it exhaustively — basename
    match, root ordering, no-match, non-symlink input, unreadable root — so no redundant command-level test
    was added; there's no existing pattern in `lib.rs` for testing async Tauri commands directly, e.g. no
    `#[tokio::test]` usage anywhere in the file). Registered in both `generate_handler!` and the
    `collect_commands!` specta builder.
  - Regenerated `src/lib/bindings.gen.ts` (`cargo run --bin export_bindings --features "specta-bindings
    sidecar-platform"`) — adds `is_symlink: boolean` to the `DirEntry`/`LinkStatus` TS types and
    `suggestRepair(brokenLink, searchRoots)` to the typed client.
  - **Frontend fallout** from the new required field: fixed 4 `DirEntry` construction sites in `src/App.svelte`
    (synthesized folder/drive/home-row entries) and 1 in `src/lib/replayOverlay.ts` (`toDirEntries`), all set
    to `is_symlink: false` (none of these paths carry real link info). Fixed 17 test files whose `DirEntry`
    fixtures/expectations were missing the new field (helper factories + two `toEqual` literals in
    `src/lib/replayOverlay.test.ts`).
  - Verify: `cargo test -p cpe-server` green (1158 tests, incl. the new one). `cargo test` from `src-tauri`
    green (80/80, excluding the pre-existing unrelated `find_similar_images_collect_groups_a_fixture` failure
    — reproduced identically on unmodified `main` via `git stash`, confirmed not caused by this change),
    including the bindings-drift guard. `cargo clippy --all-targets -- -D warnings` clean on `cpe-server`
    (default and `--features index`) and on `src-tauri` (default and `--features sidecar-platform`).
    `npm run check`: 0 errors. `npx vitest run`: 141/141 files, 1569/1569 tests green.
