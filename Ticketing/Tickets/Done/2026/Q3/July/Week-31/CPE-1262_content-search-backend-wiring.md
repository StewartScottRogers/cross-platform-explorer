---
id: CPE-1262
title: "Wire file-content (semantic) search to Tauri commands: index-build + search + persist"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-02
closed: 2026-08-02
epic: CPE-976
---

## Summary
Epic CPE-976's engine stack is BUILT but wired to ZERO commands: `SemanticIndex` (upsert_document/search/save/load,
`crates/server/src/semantic_index.rs`), `VectorIndex` (CPE-981), `Embedder` trait + local dependency-free
`FakeEmbedder` (CPE-982, `embedder.rs`), chunk→embed pipeline (CPE-983). This slice wires them into Tauri commands so
the user can search files **by their contents**, using the **local embedder — NO API key**. Frame honestly as
file-CONTENT search, embedder-pluggable (a better model can drop in later behind the existing seam); do not oversell
"semantic".

## Build
- **Store/persistence:** persist a `SemanticIndex` to the app data dir (e.g. `<appdata>/content-index/<hash(root)>.idx`)
  via the index's existing `save`/`load`. Mirror an existing on-disk store pattern (thumb_cache / audit_journal / vault store).
- **`content_index_build(root)` command (streamed):** walk `root` with the existing shared walker; for each **text-like**
  file (reuse the text-detection/read used by `content_search.rs` or a small helper — skip binaries/oversized files),
  read its text and `index.upsert_document(path, text)` with chunking (`SemanticIndex::with_chunking`). Persist at the end.
  Stream progress in batches over an `ipc::Channel` per STREAMING.md (files-indexed count / current path); provide a
  collect variant if the shared-walker convention needs it. Use the `FakeEmbedder` (pick the dim `vector_index` expects;
  the embedder used at `load` MUST match the dim the index was saved with — document this).
- **`content_search(root, query, k)` command:** load the persisted index for `root`, `index.search(query, k)`, map each
  `DocHit` → `{ path, score, snippet }` (read `DocHit`'s fields; snippet = the matched chunk text or a slice around it).
  Return ranked hits. Guard: no index yet → clear empty/`needs-build` result, not an error crash.
- Register both commands in `generate_handler!` (both lists) + regen `bindings.gen.ts` (specta structs change → REQUIRED).
- Keep it async + `spawn_blocking` for the walk/index IO (per the async-all-blocking-commands rule).

## Acceptance criteria
- `content_index_build` indexes a folder's text files (streamed progress) + persists; re-run updates.
- `content_search` returns ranked path+score+snippet hits from a real query over indexed content.
- `cargo build`/`test`/`clippy --all-targets -D warnings` clean; new cargo tests cover build→search round-trip over a temp
  folder of text files (deterministic with FakeEmbedder). Bindings regenerated (Typed-bindings drift guard passes).
- No new dep; local embedder only (no network/key).

## Notes
Frontend UI is CPE-1263 (depends on this). A better embedder / incremental-update-on-FS-change / richer text extraction
(pdf/docx) are later slices. Persisted-index embedder-dim compatibility is the main correctness trap — test it.

## Work Log
- 2026-08-02 — Worker (sonnet, worktree) built `crates/server/src/content_index.rs`: the walk +
  persistence layer over the already-built `SemanticIndex`/`Embedder` engine. `CONTENT_INDEX_DIM = 1024`
  fixes the `FakeEmbedder` dimension for every content index this module builds/loads. `build_index_with`
  walks a root (explicit stack, skips dot-dirs/symlinked dirs/unreadable entries, NUL-sniffs to skip
  binaries, 2 MiB per-file cap, 20k-file safety cap) and `upsert_document`s each text file, ticking a
  `ContentIndexProgress { files_indexed, files_skipped, current_path }` every 10 files plus a final tick.
  `save_index`/`load_index` persist to `<dir>/<fnv1a64(root)>.cix`; `load_index` returns `Ok(None)` for a
  never-built root (clean "needs build" signal) vs `Err` for a file that exists but fails to parse
  (stale format or a dim mismatch — `SemanticIndex::load`'s own never-panic discipline). `search_index`
  returns `ContentSearchOutcome { hits: Vec<ContentHit{path,score,snippet}>, index_exists }` — the
  `index_exists` flag is a **deviation from the ticket's literal `-> Vec<ContentHit>` signature**: `DocHit`
  carries no chunk/snippet text (only doc_id + score — the vector layer stores embeddings, not the
  original text), so the snippet is re-read from the file and centred on the query's first literal token
  match; and CPE-1263's own "prompt to build if no index" acceptance criterion needs a distinct
  never-built-vs-zero-hits signal that a bare empty `Vec` can't carry. Wired two Tauri commands in
  `src-tauri/src/lib.rs`: `content_index_build` (async + `spawn_blocking`, per-root cancel registry
  mirroring `INDEX_BUILD_CANCELS`, persists under `<app_data>/content-index/`) and `content_search` (async
  + `spawn_blocking`, loads fresh per call — no managed state, since a per-folder index is small enough
  that residency buys nothing unlike the whole-machine `index_service`). Registered both in both
  `generate_handler!`/`collect_commands!` lists. Regenerated `src/lib/bindings.gen.ts` via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (53 lines added:
  `ContentHit`/`ContentIndexBuildStats`/`ContentIndexProgress`/`ContentSearchOutcome` + the two command
  wrappers); a second run produced an identical diff (deterministic). No new Cargo dependency — reuses
  `semantic_index`/`embedder`/`vector_index`, already unconditionally compiled (no feature gate). No
  capabilities.json change needed (custom `#[tauri::command]`s aren't capability-gated, only plugin
  permissions are).
  Verify (from `crates/server`): `cargo build` clean; `cargo test --lib content_index` → 10/10 new tests
  pass; `cargo test --lib` → 1272/1272 pass; `cargo clippy --all-targets -- -D warnings` clean (default,
  `--features index`, and `--features pdf-thumb,video-thumb`); full `cargo test` (integration suites)
  clean. From `src-tauri`: `cargo check` clean; `cargo clippy --all-targets -- -D warnings` clean;
  `cargo test --lib` → 108/108 pass (incl. `typed_bindings_are_committed_and_routed_through_busy_cursor`).
  `npm run check` (svelte-check, after `npm install` to restore a stale `node_modules`) → 0 errors.
  Frontend wiring (CPE-1263) is next; left in Backlog, ready.
