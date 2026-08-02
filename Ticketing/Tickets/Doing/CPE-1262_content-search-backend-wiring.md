---
id: CPE-1262
title: "Wire file-content (semantic) search to Tauri commands: index-build + search + persist"
type: feature
component: cpe-server
priority: medium
status: Doing
tags: ready
created: 2026-08-02
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
