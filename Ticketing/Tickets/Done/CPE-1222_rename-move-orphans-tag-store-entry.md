---
id: CPE-1222
title: "Bug (pre-existing): rename/move leaves an orphaned tag-store entry at the old path"
type: Bug
status: Done
priority: Low
component: cpe-server
tags: [ready]
estimate: 1h
created: 2026-08-01
closed: 2026-08-01
---

## Problem
Surfaced (not caused) during the CPE-1194 review. `rename_entry_impl` / `move_exact_impl` never
migrate a path's tag-store entries when the file moves, so tags recorded against the OLD path are
orphaned after a rename/move — e.g. a "Tag then Rename" macro that is never undone leaves a stale
tag entry keyed on the pre-rename path, and the renamed file loses its tags. Pre-dates CPE-1194 and
is out of that ticket's scope.

## Acceptance criteria
- [x] Renaming or moving a tagged file/folder migrates its tag-store entries to the new path (single-file
  and directory-subtree cases).
- [x] A rename/move followed by reload shows the tags on the new path and none orphaned at the old path.
- [x] Regression test covering rename + move, including a tagged file inside a renamed directory.

## Notes
Consider doing the migration in the same `cpe-server` layer that owns the tag store, invoked from the
rename/move commands. Check whether other path-keyed stores (favourites, frecency, snapshots) have the
same orphaning issue and note them here if so.

## Resolution
Migrated the tag-store re-key from a fragile, frontend-only, single-path, best-effort call into the
backend rename/move primitives themselves, so it happens atomically as part of the filesystem op for
every caller (drag-and-drop, inline rename, undo, macros, and watched-folder automation), and now
covers the directory-subtree case.

**`crates/server/src/tags.rs`:**
- Added `tag_store_rename_subtree(store, from, to) -> bool` — the subtree-aware superset of the
  existing exact-path-only `tag_store_rename`. An exact match re-keys like before; any entry whose
  path sits under `from` (checked with both `/` and `\` separators) is re-keyed by swapping the
  `from` prefix for `to`, leaving the rest of the path untouched. Returns whether anything changed so
  callers can skip a write when nothing needed to move.
- `retag(ctx, from, to)` (the `retag_path` command's backing fn, CPE-650) now calls the subtree-aware
  function instead of the exact-only one — a free upgrade for its existing callers too.

**`src-tauri/src/lib.rs`:** threaded `ctx: &dyn ServerCtx` into the shared rename/move primitives and
call `cpe_server::tags::retag` after a successful filesystem op, best-effort (a tag-store write
failure never fails an otherwise-successful rename/move — there's nothing sane to roll back to):
- `rename_entry_impl` (backs the `rename_entry` command, the macro "rename" step, AND the
  watched-folder "rename" action — one fix, three callers).
- `move_exact_impl` (backs the `move_exact` command used by **undo** and the macro "move" step).
  Undo previously never migrated tags at all — a rename/move + undo round-trip silently dropped the
  tag on the way back; that's fixed now too.
- `do_move_into` (backs `move_entries_impl`, i.e. the everyday drag-and-drop / cut-paste move, AND
  the watched-folder "move" action). Migrates from the actually-written path — `do_move_into`
  auto-renames on collision, so the migration targets whatever path was actually written, not the
  naively-expected one.

The existing frontend `retagPath` calls in `App.svelte` (`commitRename`, `retagMoves`) are now
redundant no-ops (the backend has already re-keyed the store by the time they fire) but harmless, so
left in place to avoid frontend churn/risk.

**Tests (real regressions, not just the pure function):**
- `crates/server/src/tags.rs`: `tag_store_rename_subtree_carries_the_top_entry_and_every_descendant`,
  `tag_store_rename_subtree_handles_windows_backslash_paths_and_no_matches`,
  `retag_through_ctx_migrates_a_directorys_whole_tagged_subtree`.
- `src-tauri/src/lib.rs`: `rename_entry_impl_migrates_a_tagged_files_entry_to_the_new_path`,
  `rename_entry_impl_migrates_tags_for_every_file_inside_a_renamed_directory` (the directory-subtree
  case, nested two levels deep), `move_entries_impl_migrates_tags_for_a_file_and_a_moved_directorys_subtree`
  (drag-and-drop move of a file AND a directory with a nested tagged file), and
  `move_exact_impl_migrates_tags_to_the_restored_path` (undo).
- All pre-existing rename/move/watch-action/macro tests updated to the new `ctx`-carrying signatures
  and still pass unmodified in behavior.

**Other path-keyed stores checked, per the ticket's ask:**
- **Favourites** (`src/lib/settings.ts`, `KEYS.favorites` = `"cpe.favorites"`) — frontend-only
  `localStorage`, keyed by `path`, with **no backend at all**. Same class of bug: renaming/moving a
  favourited item leaves a dead-path favourite behind and doesn't carry it forward. **Not fixed here**
  (out of this ticket's backend scope) — worth its own small ticket (frontend `commitRename`/
  `retagMoves`-style hook, or a `re-key-by-path` helper in `settings.ts`).
- **Frecency** (`spotlight_frecency::Visit` in `crates/server/src/spotlight_frecency.rs`) — the
  backend only *ranks* a `visits: Vec<Visit>` passed in as an argument; it doesn't load/persist
  anything itself. The actual storage (`KEYS.spotlightFrecency` = `"cpe.spotlightFrecency"`) is
  frontend `localStorage`, same as favourites. Same orphaning risk, same "not fixed here."
- **Recents** (`KEYS.recents` / `KEYS.recentFolders`) — same frontend-`localStorage`-by-path pattern,
  same risk, not explicitly asked about but noted for completeness.
- **Snapshots** — `snapshot::BlobStore` is keyed by **content hash**, not path, so renames don't
  orphan it (that's the point — dedup survives renames). But
  `snapshot_schedule::Catalog` (`crates/server/src/snapshot_schedule.rs`) **is** path-keyed — a
  `BTreeMap<String, ScheduleRule>` keyed by each rule's `root` folder — and **is** backend-persisted
  via `ServerCtx`. Renaming/moving a folder that has a scheduled-snapshot rule attached orphans that
  rule (it keeps watching the old, now-nonexistent path) exactly like tags did. **Not fixed here**
  (separate module, separate command surface) — recommend a follow-up ticket mirroring this one's
  `retag`-in-the-rename/move-primitive approach for `snapshot_schedule`.

## Work Log
- 2026-08-01 — Picked up. Grepped the tag store (`crates/server/src/tags.rs`) and the rename/move
  primitives (`rename_entry_impl` / `move_exact_impl` / `do_move_into` / `move_entries_impl` /
  `run_watch_actions_impl` in `src-tauri/src/lib.rs`). Found CPE-650/652 had already built a
  `retag_path` command + frontend hook, but it was single-path (no subtree), best-effort/fire-and-forget,
  wired only into `commitRename`/`retagMoves`, and never called by undo (`moveExact`) at all —
  confirming the bug is real and broader than just "never migrated."
- 2026-08-01 — Added `tag_store_rename_subtree` (pure, subtree-aware) to `cpe-server::tags`, upgraded
  `retag()` to use it, and threaded a `ServerCtx` into the backend rename/move primitives so the
  migration happens atomically on every code path (command, undo, macro, watch automation) instead of
  relying on a frontend afterthought.
- 2026-08-01 — Added real regression tests at both layers (pure subtree function + the actual
  filesystem-touching `rename_entry_impl`/`move_exact_impl`/`move_entries_impl`), including the
  directory-subtree case with a nested tagged file two levels deep.
- 2026-08-01 — Verified: `cargo test -p cpe-server` (1176 passed), `cd src-tauri && cargo test` (92
  passed), `cargo clippy --all-targets -- -D warnings` clean for cpe-server (default, `specta`,
  `--all-features`) and for src-tauri (default, `specta-bindings`, `sidecar-platform` — the exact
  three modes CI's backend workflow runs). No `specta::Type` struct touched, so no bindings regen
  needed. Checked favourites/frecency/recents/snapshots for the same orphaning class per the ticket's
  ask; findings recorded above. Opened PR.
