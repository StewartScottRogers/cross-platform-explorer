---
id: CPE-983
title: Semantic index pipeline — chunk → embed → document search
type: feature
component: Backend
priority: high
tags: ready
status: Done
created: 2026-07-24
epic: CPE-976
estimate: 2-3h
---

## Summary
Third child of AI semantic search (CPE-976). Ties CPE-981 (`vector_index`) and CPE-982 (`Embedder`) into a
document-level pipeline: chunk a document's text, embed each chunk, index it, and search **per document**
(best chunk wins) — so a query returns the right *files*, not raw chunks. Pure, tested with the
`FakeEmbedder`; no model decision needed.

## Design (pure)
- `chunk_text(text, max_words, overlap) -> Vec<String>` — word-window chunking with overlap, so a long doc
  becomes several embeddable passages and a phrase near a boundary isn't lost.
- `SemanticIndex { VectorIndex, Box<dyn Embedder>, docs: BTreeMap<doc_id, chunk_count> }`:
  - `upsert_document(doc_id, text)` — remove any prior chunks, chunk + `embed_batch` + add as
    `doc_id\0{i}`; records the chunk count for precise removal (incremental re-index).
  - `remove_document(doc_id)` — drop all of a doc's chunks.
  - `search(query, k) -> Vec<DocHit{doc_id, score}>` — embed the query, score all chunks, **group to the best
    chunk per document**, return the top-k docs best-first (deterministic tiebreak by doc_id).
- Reuses `vector_index`'s cosine + persistence; the text *extraction* (doc_text/content_search) and the
  change-driven re-index (CPE-833 signals) are wiring the caller supplies.

## Acceptance Criteria
- [x] `chunk_text` windows words with overlap; short text → one chunk; empty → none.
- [x] `upsert_document` indexes chunks; re-upsert replaces (no stale chunks); `remove_document` clears a doc.
- [x] `search` returns per-document hits (best chunk per doc), top-k, deterministic; end-to-end retrieval
      via `FakeEmbedder` asserts the right document ranks first.
- [x] Cargo-tested; clippy clean both modes; no new deps (pure std).

## Notes
- CPE-984 blends these semantic `DocHit`s with lexical hits (`index_query`); CPE-985 is the UI (attended).
  The real embedder backend remains the deferred big-design call.

## Work Log
- 2026-07-24 (dayshift) — Building the document pipeline on the vector core + embedder seam.
