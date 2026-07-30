---
id: CPE-1137
title: "Instant index: expose the built engine via streamed build/search commands (headless foundation)"
type: feature
component: Backend
priority: high
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-703
---

## Summary
The instant-index engine is **fully built** in `crates/server/src/index.rs` (CPE-832/833: `Index::build`,
`save`/`load`/`to_bytes`/`from_bytes`, `search`, `apply_create`/`apply_remove`/`apply_rename`) and the query
grammar in `index_query.rs` (CPE-831: `parse`/`matches`/`score`/`rank`). But it is wired to **zero commands** —
no Tauri command, no frontend binding. This ticket is the headless foundation of epic CPE-703: hold a live
index in app state and expose **streamed** build + search commands so the frontend overlay (CPE-1139) and the
live watcher (CPE-1138) can build on it. **No GUI in this ticket.**

## Design (thin commands into cpe-server; mirror existing patterns)
- **Index service (state).** Add an `IndexService` to `cpe-server` that owns the loaded indices —
  `Mutex<HashMap<u64 /*volume_id*/, Index>>` plus helpers (`build_root`, `search_all`, `save_all`,
  `load_from_disk`, `drop_volume`). Keep the domain logic in `cpe-server` (per SERVER-ARCHITECTURE.md); the
  app holds it in Tauri managed state. Mirror the existing `AgentWatchState`/`AiConsoleState` pattern
  (`src-tauri/src/lib.rs`: `Mutex<HashMap<..>>` state, registered via `.manage(...)` at ≈ line 6541-6545).
- **Persistence.** Per-volume file at `app_data_dir()/index/<volume_id>.idx` (reuse `TauriCtx::app_data_dir()`
  exactly like `audit_dir`/`checkpoints_base`). `index_build` persists after a crawl; `index_search` uses the
  in-memory index (loading from disk lazily if present and not yet resident).
- **Cancellation.** Reuse the existing cancel-token pattern (the `Mutex<HashMap<u64, Arc<AtomicBool>>>` used
  elsewhere in `lib.rs` for cancellable ops) so a re-issued build cancels the prior crawl. `Index::build`
  already takes `&AtomicBool cancel`.
- **Commands (thin dispatchers; all async + spawn_blocking per the async-all-commands rule):**
  - `index_build(root, volume_id)` → crawl via `Index::build`, insert into the service, `save` to disk.
    **Stream progress** over an `ipc::Channel` in batches (files-seen counts / final `BuildStats`) per
    `docs/design/STREAMING.md` — the `ipc::Channel` stays in the app adapter; the walker/logic is in
    `cpe-server`.
  - `index_search(query, limit)` → `index_query::parse` the input, run `Index::search` across all resident
    volumes, merge+rank, and **stream ranked `IndexHit`s** in batches over an `ipc::Channel` (supersedable by
    generation token so a newer keystroke's query wins). Also provide a collect-to-vec variant for tests, per
    the STREAMING "one shared walker backs both" convention.
  - `index_status()` → resident volumes + entry counts + truncated flags (for the UI to show index state).
  - `index_drop(volume_id)` / `index_clear()` → free memory + optionally delete the on-disk file.
- **Off-means-off (HARD DoD).** Nothing crawls or loads at startup; the service is empty until `index_build`
  is explicitly invoked. No watcher here. With the mode unused, zero startup/memory cost (assert no index
  work happens without an explicit build/search call). Add any new plugin capability to
  `src-tauri/capabilities/default.json` if the `ipc::Channel` commands need it.
- **Bindings.** These are `specta`-exported commands → **regenerate `src/lib/bindings.gen.ts`** via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` and commit it (the CI
  "Typed-bindings drift guard" fails otherwise — see [[regen-specta-bindings-on-struct-change]]).

## Acceptance Criteria
- [x] `index_build` crawls a root into a resident `Index`, persists it to `app_data/index/<volume_id>.idx`, and
      streams progress; a subsequent `index_search` returns ranked cross-folder hits from it.
- [x] `index_search` parses `ext:`/`path:`/name terms (via `index_query`) and streams ranked `IndexHit`s;
      a collect-to-vec variant exists for tests and returns the same results.
- [x] `index_status`/`index_drop` behave; dropping frees the volume and (for drop) removes its file.
- [x] **Off-means-off:** no crawl/load happens without an explicit build/search; a test asserts the service is
      empty and does no disk work at construction.
- [x] Persistence round-trips: build → save → drop-from-memory → search auto-loads from disk → same hits.
- [x] `bindings.gen.ts` regenerated + committed; `npm run check` green.
- [x] `crates/server` tests + `cargo clippy --all-targets -- -D warnings` green (both feature modes);
      `src-tauri` `cargo check` green (the new commands compile + are in `generate_handler!`).

## Work Log
- **2026-07-29 — built the headless foundation.**
- **New service (`crates/server/src/index_service.rs`, feature `index`).** `IndexService` owns
  `Arc<Mutex<HashMap<u64, Index>>>` (cheaply cloneable so an async command can clone the handle out of
  managed state and move it into `spawn_blocking`). Helpers: `build_root` (crawl + persist + make resident,
  with a progress callback), `search_all` (parse via `index_query` → search every resident volume → merge +
  rank + truncate to `limit`), `stream_search` (chunks `search_all` into `SEARCH_BATCH`=32 batches — the
  *same* ranking backs both variants), `status`, `drop_volume`, `clear`, and a private `load_missing` that
  lazily loads any persisted-but-not-resident `*.idx` from the index dir on search.
- **Engine touch-up (`index.rs`).** Refactored `Index::build` to delegate to a new `build_with(…, progress:
  impl FnMut(BuildStats))` — one shared walker; plain `build` passes a no-op — so a build can stream live
  progress every 256 dirs (`PROGRESS_EVERY`) plus a final tick. Added `serde::{Serialize,Deserialize}` +
  gated `specta::Type` to `IndexHit` and `BuildStats` so they can be command returns / channel payloads.
  All existing engine tests unchanged and still pass.
- **Commands (thin dispatchers, `src-tauri/src/lib.rs`), all async + `spawn_blocking`:** `index_build(root,
  volume_id, on_progress: Channel<BuildStats>)` (crawl + persist to `<app_data>/index/<volume_id>.idx`,
  streams progress, re-issued build for the same volume cancels the prior crawl via an
  `INDEX_BUILD_CANCELS` registry mirroring `DIR_STREAM_CANCELS`); `index_search(query, limit, on_hit:
  Channel<Vec<IndexHit>>)` (streamed) + `index_search_collect(query, limit) -> Vec<IndexHit>` (collect-to-vec
  for tests); `index_status() -> Vec<VolumeStatus>`; `index_drop(volume_id) -> bool`; `index_clear()`. All
  registered in both `generate_handler!` and the `collect_commands!` (specta) blocks.
- **State / persistence.** `IndexService::default()` is `.manage(...)`-ed **unconditionally** (not behind
  `sidecar-platform`) — the index is a core-explorer feature. Persistence dir is `<app_data>/index` resolved
  via `TauriCtx::app_data_dir()`, exactly like `audit_dir`/`checkpoints_base`.
- **Off-means-off.** The service holds an empty map at startup and never touches disk until `index_build`
  (or a search, which only reads the index dir if it exists). `fresh_service_is_empty_and_touches_nothing`
  asserts a constructed service is empty, does no disk work, and doesn't create the index dir on a search.
- **Feature wiring.** `src-tauri/Cargo.toml` now enables `cpe-server`'s `index` feature (the crate default
  stays OFF, preserving its own delete-test). No new dependencies — the index is std-only. No capability
  change needed: custom `#[tauri::command]` + `ipc::Channel` are covered by `core:default`, same as the
  existing `list_dir_stream`.
- **Bindings regenerated + committed** (`cargo run --bin export_bindings --features "specta-bindings
  sidecar-platform"`): `indexBuild`/`indexSearch`/`indexSearchCollect`/`indexStatus`/`indexDrop`/`indexClear`
  + the `BuildStats`/`IndexHit`/`VolumeStatus` types now in `src/lib/bindings.gen.ts`.
- **Verify (Windows).** `crates/server`: `cargo test` 1066 passed; `cargo test --features index` 1093 passed
  (incl. 7 new `index_service` tests); `cargo clippy --all-targets -- -D warnings` green both plain and
  `--features "index specta"`. `src-tauri`: `cargo check` (plain) and `cargo check --features
  "specta-bindings sidecar-platform"` green; `cargo clippy` green both modes. `npm run check`: 0 errors,
  0 warnings.

## Notes
- Foundation for CPE-1138 (live `notify` watcher → `apply_*`) and CPE-1139 (frontend global-search overlay).
- Honour the epic tiebreaker: fast/small/**zero cost when off**. Reuse `name_search` semantics already in
  `index_query`. Full "auto-crawl every mounted volume" can be a thin follow-up if not trivial here — building
  a given root + searching all resident volumes satisfies this ticket.
