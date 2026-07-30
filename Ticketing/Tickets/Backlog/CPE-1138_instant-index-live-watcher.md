---
id: CPE-1138
title: "Instant index: live notify watcher keeps the index current without rescan"
type: feature
component: Backend
priority: high
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-703
blocked-by: CPE-1137
---

## Summary
CPE-833 built the pure incremental primitives on `Index` (`apply_create`/`apply_remove`/`apply_rename`). This
ticket lands the **OS watch adapter**: a `notify`-backed source (the crate is already an optional dep in
`src-tauri`, line 77) that watches each indexed root and feeds live filesystem events into the resident
`IndexService` (CPE-1137) so the index stays current without a full rescan — the epic's "stays current as
files are created/renamed/deleted" DoD.

## Design
- **Watch source in the app adapter** (`src-tauri`, behind the existing optional `notify` feature). When an
  index is built for a root (CPE-1137), start a `notify` recommended watcher on that root; on teardown
  (`index_drop`/disable) stop it. Keep the watcher handles in the index state (mirror `FolderWatchState`).
- **Map events → primitives.** Translate `notify` events to `apply_create(path, is_dir)` /
  `apply_remove(path)` / `apply_rename(from, to)` on the resident `Index` (lock the service Mutex). Handle
  `notify`'s rename pairing (from/to may arrive as two events on some platforms — coalesce by cookie where
  available, else treat as remove+create).
- **Debounce/batch.** Coalesce a burst of events (e.g. a short debounce window) and apply as a batch under one
  lock acquisition, so a `git checkout` storm doesn't thrash. Persist (`save`) opportunistically (debounced),
  not per event.
- **Pure, testable core.** Put the **event→primitive mapping** in a pure function in `cpe-server`
  (e.g. `index_watch::plan_from_event(...) -> Vec<IndexMutation>`) that is unit-tested headlessly with
  synthetic event sequences (created / removed / rename-pair / rename-split). The `notify` subscription itself
  (the OS half) is the thin adapter and is GUI/attended-verified with the running app.
- **Off-means-off.** No watcher thread exists unless an index is actively built + the mode is on; disabling or
  dropping the index stops the watcher and releases the handle. Zero cost when off.
- Regenerate `bindings.gen.ts` only if the command surface changes (likely none — this hooks into
  `index_build`/`index_drop`).

## Acceptance Criteria
- [ ] Building an index for a root starts a `notify` watcher on it; dropping/disabling stops it (no leaked
      threads/handles).
- [ ] The pure `index_watch` mapping converts created/removed/renamed events (incl. rename pairs and
      rename-as-remove+create) into the correct `apply_*` mutations — covered by headless unit tests with
      synthetic events.
- [ ] After a watched create/rename/remove, `index_search` reflects the change **without** a rebuild
      (integration test drives the mapping + `apply_*` against a temp `Index` and asserts search results
      update).
- [ ] Event bursts are debounced/batched (no per-event save; one lock acquisition per batch) — asserted by a
      test feeding a burst.
- [ ] **Off-means-off:** no watcher runs without an active index; teardown verified.
- [ ] `crates/server` tests + clippy (both modes) green; `src-tauri` `cargo check` green with the `notify`
      feature enabled.

## Notes
- Depends on CPE-1137 (the `IndexService` + build/drop lifecycle to hook into).
- The full live-FS behaviour is verified in the epic's end-of-run build→deploy→run GUI pass (with CPE-1139).
- NTFS USN-journal fast-path stays out of scope (a documented later Windows-only optimisation); `notify` is
  the always-available portable baseline per the CPE-703 activation decision.
