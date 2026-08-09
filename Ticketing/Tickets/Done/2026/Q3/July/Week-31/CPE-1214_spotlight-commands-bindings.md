---
id: CPE-1214
title: "Expose spotlight search + frecency over Tauri commands + specta bindings"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-704
---

## Summary
Backbone for CPE-704. The fuzzy/ranking/frecency cores exist + are tested (`spotlight.rs`,
`spotlight_results.rs`, `spotlight_frecency.rs`) but are UNEXPOSED (no command, no binding). Wire them to the
frontend.

## Build
- Thin `#[tauri::command] spotlight_search(query, sources: Vec<(ResultKind, Vec<String>)>, per_kind_cap)`
  dispatching into `spotlight_results::aggregate`; `spotlight_frecent(visits, now_s, limit)` into
  `spotlight_frecency::rank_frecent`. One-line dispatchers (SERVER-ARCHITECTURE); register in `generate_handler!`
  + `collect_commands!`. Regenerate `bindings.gen.ts` (`SpotResult`/`SpotSection`/`Visit` first cross the
  boundary — drift guard).

## Acceptance Criteria
- [x] `cargo test -p cpe-server` green; a Rust integration test invokes the command path → grouped/highlighted
      output; clippy both modes clean; `npm run check` green; bindings-drift guard green.

## Work Log
- 2026-08-01 — Filed by Foreman (sprint, epic CPE-704). Backbone; build first. 1216-1218 depend on the shape.
- 2026-08-01 — Done. Two thin `#[tauri::command]` dispatchers added to `src-tauri/src/lib.rs`, following the
  existing `_impl`-fn + `spawn_blocking` pattern (same as `board_cards`/`text_stats`):

  - `spotlight_search(query: String, sources: Vec<(ResultKind, Vec<String>)>, per_kind_cap: usize) -> Vec<SpotSection>`
    — dispatches into `cpe_server::spotlight_results::aggregate(&query, &sources, per_kind_cap)`. Infallible.
  - `spotlight_frecent(visits: Vec<Visit>, now_s: u64, limit: usize) -> Vec<String>`
    — dispatches into `cpe_server::spotlight_frecency::rank_frecent(&visits, now_s, limit)`. Infallible.

  TS client (camelCase, via `spotlightSearch`/`spotlightFrecent` in `bindings.gen.ts`):
  - `spotlightSearch(query: string, sources: ([ResultKind, string[]])[], perKindCap: number): Promise<SpotSection[]>`
  - `spotlightFrecent(visits: Visit[], nowS: number, limit: number): Promise<string[]>`

  `ResultKind` (`crates/server/src/spotlight_results.rs`) needed a `serde::Deserialize` derive added
  alongside its existing `Serialize` — it now crosses the IPC boundary as a command **input** (each
  `sources` entry is `(ResultKind, Vec<String>)`), not just an output. `SpotResult`/`SpotSection`/`Visit`
  already derived `specta::Type` under the crate's `specta` feature; no other type changes needed.

  Both commands registered in `generate_handler!` and the `export_bindings` `collect_commands!` list (right
  after `text_stats`/`inspect_file`). `bindings.gen.ts` regenerated via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`.

  Tests added in `src-tauri/src/lib.rs`'s `mod tests` (calling the `_impl` fns directly — no async runtime
  needed, same pattern as `board_cards_impl`): `spotlight_search_impl_groups_caps_and_highlights` (asserts
  section ordering by kind, per-kind cap, and non-empty highlight `positions` on the winning result),
  `spotlight_search_impl_empty_on_no_match`, `spotlight_frecent_impl_ranks_recent_and_frequent_first`,
  `spotlight_frecent_impl_caps_at_limit`. Also added `spotlightSearch`/`spotlightFrecent` to the existing
  bindings-drift guard's command-name assertion list.

  Verification (all synchronous, one at a time):
  - `cargo test` in `crates/server`: 1160 passed, 0 failed.
  - `cargo test` in `src-tauri` (default features): 85 passed, 0 failed (incl. the 4 new spotlight tests +
    the bindings-drift guard, now green against the regenerated file).
  - `cargo clippy --all-targets -- -D warnings` in `crates/server`: clean (default + `--features specta`).
  - `cargo clippy --all-targets -- -D warnings` in `src-tauri`: clean (default + `--features
    "specta-bindings sidecar-platform"`).
  - `npm run check`: 0 errors, 0 warnings.

  No frontend UI wired yet — this ticket is the backbone only, per the epic. CPE-1216/1217/1218 build the
  overlay against this exact shape.
