---
id: CPE-1138
title: "Instant index: live notify watcher keeps the index current without rescan"
type: feature
component: Backend
priority: high
status: Done
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
- [x] Building an index for a root starts a `notify` watcher on it; dropping/disabling stops it (no leaked
      threads/handles).
- [x] The pure `index_watch` mapping converts created/removed/renamed events (incl. rename pairs and
      rename-as-remove+create) into the correct `apply_*` mutations — covered by headless unit tests with
      synthetic events.
- [x] After a watched create/rename/remove, `index_search` reflects the change **without** a rebuild
      (integration test drives the mapping + `apply_*` against a temp `Index` and asserts search results
      update).
- [x] Event bursts are debounced/batched (no per-event save; one lock acquisition per batch) — asserted by a
      test feeding a burst.
- [x] **Off-means-off:** no watcher runs without an active index; teardown verified.
- [x] `crates/server` tests + clippy (both modes) green; `src-tauri` `cargo check` green with the `notify`
      feature enabled.

## Notes
- Depends on CPE-1137 (the `IndexService` + build/drop lifecycle to hook into).
- The full live-FS behaviour is verified in the epic's end-of-run build→deploy→run GUI pass (with CPE-1139).
- NTFS USN-journal fast-path stays out of scope (a documented later Windows-only optimisation); `notify` is
  the always-available portable baseline per the CPE-703 activation decision.

## Work Log (2026-07-29)

**Pure mapping (`crates/server/src/index_watch.rs`, feature `index`, zero `notify` dependency):**
- `WatchEvent` enum: `Created{path,is_dir}` / `Removed{path}` / `Modified{path}` (identity-preserving
  content change — ignored) / `Renamed{from,to}`.
- `IndexMutation` enum: `Create{path,is_dir}` / `Remove{path}` / `Rename{from,to}` — 1:1 with
  `Index::apply_create`/`apply_remove`/`apply_rename`.
- `plan_from_event(&WatchEvent) -> Vec<IndexMutation>` and `plan_from_events(&[WatchEvent]) -> Vec<IndexMutation>`
  (batch convenience, concatenates in order). `Modified` always maps to nothing. An unpaired rename half
  is represented by the adapter as a plain `Removed`/`Created` `WatchEvent` (not a special enum case) —
  the "rename-as-remove+create" fallback falls out of the existing variants.
- `IndexService::apply_mutations(&self, dir: &Path, volume_id: u64, mutations: &[IndexMutation]) -> Result<bool, String>`
  lives on `IndexService` itself (`index_service.rs`, needs the private resident-map field) but is
  conceptually part of this ticket's wiring. Locks the volume ONCE, applies every mutation in the slice,
  and — only if at least one mutation actually changed the index — persists (`save`) once. No-op
  (`Ok(false)`) on an empty batch or a non-resident volume (a benign race with drop/clear).
- 12 headless unit tests: pure-mapping cases (create/remove/modified/paired-rename/rename-as-remove+create/
  batch-order-and-skip-modified) plus integration-style cases driving `IndexService` end-to-end (watched
  create/remove/rename reflected in `search_all` without a rebuild, a 20-event burst applied as one batch,
  a dropped-volume no-op).

**`notify` adapter (`src-tauri/src/lib.rs`, behind `sidecar-platform`, mirrors `FolderWatchState`/`AgentWatchState`):**
- `IndexWatchState` (managed state): `HashMap<u64, IndexWatch>` keyed by `volume_id`; `arm`/`stop`/`stop_all`
  own the map lifecycle (kept `notify`/`AppHandle`-free so it's unit-testable — mirrors
  `AgentWatchState::arm`). `index_watch_start(app, volume_id, root)` builds the `notify::RecommendedWatcher`,
  spawns the pump thread, and calls `state.arm`; `index_watch_stop`/`index_watch_stop_all` call
  `state.stop`/`stop_all`. All four have a `#[cfg(not(feature = "sidecar-platform"))]` no-op twin so
  `index_build`/`index_drop`/`index_clear` can call them unconditionally.
- `index_watch_pump` debounces on a 300ms window (mirrors `folder_watch_pump`'s 250ms / `fs_activity_pump`'s
  200ms): plain `Create`/`Remove` events and unresolved `Modify(Name(Any/Other))` renames go into a
  `touched: HashSet<String>`, resolved at flush by **re-stat**'ing each path (exists ⇒ `Created`, gone ⇒
  `Removed`) — this also self-corrects a same-window create-then-delete or delete-then-recreate rather than
  trusting the individual event's kind. `RenameMode::Both` pairs immediately; `From`/`To` correlate by
  `notify`'s tracker cookie (orphaned `From`s left at flush time fold into the re-stat set — the
  "rename-as-remove+create" fallback). Each flush calls `cpe_server::index_watch::plan_from_events` once
  then `IndexService::apply_mutations` once — one lock/save per debounce window, not per event.
- Hook points: `index_build` starts the watcher after a successful build **that actually became resident**
  (checks the build's `cancel` flag post-crawl so a superseded build never clobbers a newer build's
  watcher with a stale root — same guard `IndexService::build_root` uses for the resident map itself).
  `index_drop` and `index_clear` (now takes an `app: AppHandle` param) stop the relevant watcher(s)
  unconditionally, so a dropped/cleared volume never leaves a live watcher behind.
- 2 new headless tests: `IndexWatchState`'s arm/stop/stop_all map lifecycle (mirrors
  `arming_two_sessions_then_stop_all_leaves_zero_watchers` for `AgentWatchState`) using a real `notify`
  watcher over a scratch temp dir (receiver dropped — only the map lifecycle is asserted, not events).

**Bindings:** `index_clear` gained an `app: tauri::AppHandle` parameter, but `tauri-specta` excludes
`AppHandle`/`State` from the generated TS signature (same as every other command), so
`bindings.gen.ts` only picked up doc-comment text for `indexDrop`/`indexClear` — no signature change,
no frontend call sites touched. Regenerated via `cargo run --bin export_bindings --features
"specta-bindings sidecar-platform"` and committed per the CI drift guard.

**Verify:** `crates/server` — `cargo test --features index` (1107 passed) and
`cargo clippy --all-targets --features "index specta" -- -D warnings` (clean). `src-tauri` —
`cargo check --features "sidecar-platform"` and plain `cargo check` (off-means-off, no `notify` pulled)
both clean; `cargo clippy --features "sidecar-platform" -- -D warnings` clean;
`cargo test --features "sidecar-platform"` green (28/28 in the watcher-adjacent filter, including the 2
new tests, no warnings). `npm run check` — 0 errors/warnings.

**Assumptions / scope:** the pump's rename-cookie correlation logic is a fresh, purpose-built copy (not a
reuse of Agent Watch's `handle_rename_event`/`mark_renamed`) because Agent Watch's helpers collapse a
rename into an opaque `"renamed"` tag that doesn't preserve from/to direction — fine for a UI activity
label, not enough to decide `apply_create` vs `apply_remove` correctly for the index. The full live-FS
behaviour (watch a real folder, edit it in the running app, see search update) is left to the GUI/attended
pass noted in this ticket, alongside CPE-1139.

## Review fix (2026-07-29, Foreman-applied per opus reviewer CHANGES REQUESTED)
- **Nested-create ordering bug fixed:** the pump's flush drained `touched` (a `HashSet`) in arbitrary order,
  so a child file's `Create` could be applied before its parent dir's `Create`; `Index::apply_create`
  silently drops an entry whose parent isn't indexed yet → the child stayed missing until a rebuild (breaks
  AC#3 for same-window `mkdir a && touch a/f`, archive-extract, `git checkout`, `cp -r` — common on Windows).
- **Fix:** extracted the re-stat resolution into a pure, testable `index_watch::resolve_touched(touched, stat)`
  in `cpe-server` that **sorts paths ascending** so an ancestor (a path prefix) always precedes its
  descendants, then classifies each as `Created`(exists)/`Removed`(gone). The pump now calls it instead of
  draining the `HashSet` directly.
- **Tests added:** `resolve_touched_orders_ancestors_before_descendants` (pure: child-before-parent input →
  parent Create emitted first; gone path → Removed) and `nested_create_in_one_window_is_indexed_regardless_of_event_order`
  (end-to-end: build index, create `freshdir/` + `freshdir/nested_hit.rs`, feed the child event FIRST →
  resolve_touched → apply_mutations → assert `nested_hit.rs` is searchable). index_watch now 14 tests.
- No exported type/signature changed → no `bindings.gen.ts` regen. `crates/server` green; clippy clean
  (`index specta`); `src-tauri` clippy `--features sidecar-platform` clean.
