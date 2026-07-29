---
id: CPE-996
title: OCR → semantic ingest helper
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-980
---

# CPE-996 — OCR → semantic ingest helper

## Summary

The thin integration that makes epic CPE-980 (OCR) feed epic CPE-976 (semantic search): recognise an
image's text with an `OcrEngine`, then index that text in a `SemanticIndex`, so an image-only file (a
scanned page, a screenshot) becomes a first-class hit in semantic search.

New module `crates/server/src/semantic_ingest.rs` — pure glue, no I/O, no new dependencies. It owns no
state; the caller owns the `SemanticIndex`, the `OcrEngine`, and (for the cached variant) the
`OcrCache`.

## Design

Two free functions in `semantic_ingest.rs`, importing the types from `crate::ocr` and
`crate::semantic_index`:

- `pub fn index_image(index: &mut SemanticIndex, engine: &dyn OcrEngine, doc_id: &str,
  image_bytes: &[u8])` — `engine.recognize(image_bytes)` → `index.upsert_document(doc_id, &text)`.
- `pub fn index_image_cached(index: &mut SemanticIndex, engine: &dyn OcrEngine, cache: &mut OcrCache,
  doc_id: &str, image_bytes: &[u8])` — same, but the OCR text is resolved through
  `cache.recognize_cached(engine, image_bytes)` so re-indexing identical image bytes does not re-OCR.

`pub mod semantic_ingest;` added to `crates/server/src/lib.rs` in the AI-search cluster (right after
`ocr`), with a doc comment matching the neighbouring modules.

## Acceptance Criteria

- [x] `index_image` OCRs the bytes and upserts the text under `doc_id` (thin, pure, no I/O).
- [x] `index_image_cached` resolves OCR text through `OcrCache` so identical bytes are OCR'd once.
- [x] End-to-end test: two text-as-bytes "images" indexed via `index_image` + `FakeEmbedder`, a
  natural-language `search` retrieves the right doc by meaning/keywords (proves 980 → 976 composition).
- [x] Cached test: a call-counting `OcrEngine` proves indexing the same bytes twice OCRs once (cache
  hit) and the doc is still findable.
- [x] Zero new dependencies; pure glue importing `crate::ocr` / `crate::semantic_index`.
- [x] `pub mod semantic_ingest;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib semantic_ingest` passes.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `semantic_ingest.rs` end-to-end as the OCR → semantic-index glue.
  - **Assumptions logged:**
    - Kept it as **free functions**, not a struct, because the module holds no state — the caller owns
      the index, engine, and cache. This mirrors the "caller owns the pieces" seam style used across
      the AI-search cluster and keeps the glue as thin as the ticket asks.
    - `&dyn OcrEngine` (not a generic) so a caller can pass a `Box<dyn OcrEngine>` / `FakeOcr` /
      real-engine interchangeably without monomorphisation — the `OcrEngine` trait is object-safe by
      design (CPE-991).
    - No public re-exports of `ocr` / `semantic_index` types; callers already have them. Glue only.
    - Empty-OCR-result behaviour is inherited from `SemanticIndex::upsert_document` (tokenless doc is
      tracked but never matches) — not special-cased here.
    - Module placed after `ocr` in `lib.rs` (the AI/document-understanding cluster).
  - Verified (PowerShell, `crates/server`): `cargo test --lib semantic_ingest` → all passed; both
    clippy invocations (default + `--features index`) clean with `-D warnings`.
- 2026-07-24 — Status → Done; ACs checked; moved to `Tickets/Done/2026/Q3/July/Week-30/`.
