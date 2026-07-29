---
id: CPE-833
title: Live incremental index update (change-watching core)
type: feature
component: Backend
priority: medium
tags: ready
created: 2026-07-24
epic: CPE-703
estimate: 3-4h
status: Done
---

## Summary
Child of CPE-703, prereq CPE-832 (done). Keep the filename index ([`cpe_server::index::Index`]) current as
files are created / renamed / deleted **without a full rescan**. This ticket lands the **pure, headless
core** — incremental mutation primitives on `Index` — so a change source can apply single events in O(depth)
instead of re-crawling. The actual OS watch source (the `notify` crate: inotify / FSEvents /
ReadDirectoryChangesW, already an optional dep in `src-tauri`) is the **adapter** half and is wired + GUI-
verified separately (it needs the running app + live filesystem events).

Filed + worked in the CPE-703 dayshift waterfall (2026-07-24), continuing straight on from CPE-832.

## Design (pure core)
- **Absolute-path roots.** `Index::build` now stores the **absolute root path** on the root entry, so
  reconstructed hit paths are absolute + openable (a small CPE-832 refinement) and change events — which
  carry absolute paths — resolve against the same strings.
- **Path resolution in O(depth).** Two runtime aux maps, rebuilt on load next to the trigram map: a
  name→id interner and a `(parent_id, name_id) → entry_id` child map. `resolve(abs_path)` finds the root
  whose path prefixes the target, then descends component-by-component.
- **Tombstoning, not physical delete.** Entries are addressed by index (parents + trigram postings point at
  ids), so a remove **marks the entry (and its subtree) dead** rather than shifting the vec. `search`,
  `path_of`, and `resolve` skip dead entries. `to_bytes` **compacts** (drops dead entries + remaps ids), so
  tombstones never persist and the on-disk format stays v1 — a save is also the natural compaction point.
- **Primitives:** `apply_create(path, is_dir)`, `apply_remove(path)`, `apply_rename(from, to)` — each
  updates entries, the trigram postings, and the aux maps incrementally.

## Acceptance Criteria
- [x] `Index::build` stores absolute root paths; hit paths are absolute (CPE-832 test updated →
      `search_reconstructs_absolute_paths`, asserts `starts_with(root)`).
- [x] `apply_create` / `apply_remove` / `apply_rename` keep `search` results correct without a rebuild —
      proven by `incremental_mutations_match_a_fresh_rebuild` (apply the 3 ops, mirror on disk, assert
      identical search results across 6 queries).
- [x] Removal tombstones the whole subtree (`apply_remove_tombstones_the_whole_subtree`); `to_bytes`
      compacts dead entries away, format stays v1, reload == a fresh build of the survivors
      (`to_bytes_compacts_tombstones_away`). A moved directory carries its subtree via parent pointers
      (`moving_a_directory_carries_its_subtree`); a re-created path revives its tombstone.
- [x] Cargo-tested (18 total, 7 new); `clippy --all-targets -D warnings` clean both feature modes; plain
      build (feature OFF) still compiles zero indexer.

## Work Log
- **2026-07-24 ~05:2x USMST (dayshift):** built the pure incremental core. Key design calls (logged):
  - **Absolute-path roots** — the root entry now stores the absolute crawl-root path (trimmed of trailing
    separators), so hits are absolute/openable and change events resolve against the same strings. Small
    refinement to CPE-832's basename-rooted paths; the shipped 832 test still passed (absolute path
    contains the basename) and was strengthened to assert `starts_with(root)`.
  - **Single child-adjacency map** (`parent → [child ids]`) + a name interner as the runtime aux maps
    (rebuilt on load beside trigrams). `resolve(abs_path)` strips the matching root prefix and descends in
    O(depth·fanout).
  - **Reparent-on-move** — a rename/move just repoints the entry's `parent` (and fixes the two child
    vectors), so a moved directory's entire subtree follows automatically via parent pointers (O(1) +
    fixups), no subtree re-crawl. Verified.
  - **Add-only trigrams during mutation** — mutations only ever *add* postings; stale postings from an old
    name are harmless (every candidate is re-confirmed by `index_query::matches`) and are cleaned by
    compaction on save.

## Notes
- Follows CPE-832. CPE-834 (overlay UI, attended/GUI) consumes both. The `notify` → `apply_*` event bridge
  is the adapter slice, deferred to attended wiring (live OS events + running app).
- Cross-directory move requires the destination's parent dir to already be indexed; if not, the event is a
  no-op and the change source should rebuild the affected subtree (documented on `apply_rename`).
