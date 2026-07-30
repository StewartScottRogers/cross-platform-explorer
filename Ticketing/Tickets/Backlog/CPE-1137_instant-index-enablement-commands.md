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
- [ ] `index_build` crawls a root into a resident `Index`, persists it to `app_data/index/<volume_id>.idx`, and
      streams progress; a subsequent `index_search` returns ranked cross-folder hits from it.
- [ ] `index_search` parses `ext:`/`path:`/name terms (via `index_query`) and streams ranked `IndexHit`s;
      a collect-to-vec variant exists for tests and returns the same results.
- [ ] `index_status`/`index_drop` behave; dropping frees the volume and (for drop) removes its file.
- [ ] **Off-means-off:** no crawl/load happens without an explicit build/search; a test asserts the service is
      empty and does no disk work at construction.
- [ ] Persistence round-trips: build → save → drop-from-memory → search auto-loads from disk → same hits.
- [ ] `bindings.gen.ts` regenerated + committed; `npm run check` green.
- [ ] `crates/server` tests + `cargo clippy --all-targets -- -D warnings` green (both feature modes);
      `src-tauri` `cargo check` green (the new commands compile + are in `generate_handler!`).

## Notes
- Foundation for CPE-1138 (live `notify` watcher → `apply_*`) and CPE-1139 (frontend global-search overlay).
- Honour the epic tiebreaker: fast/small/**zero cost when off**. Reuse `name_search` semantics already in
  `index_query`. Full "auto-crawl every mounted volume" can be a thin follow-up if not trivial here — building
  a given root + searching all resident volumes satisfies this ticket.
