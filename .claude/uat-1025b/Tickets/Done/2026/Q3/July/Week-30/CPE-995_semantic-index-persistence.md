---
id: CPE-995
title: Persist a SemanticIndex so it survives a restart without re-embedding
type: feature
status: Done
priority: high
component: Backend
tags: ready
created: 2026-07-24
closed: 2026-07-24
epic: CPE-976
---

## Summary
Give `SemanticIndex` (crates/server, epic CPE-976) durable `save`/`load` so an index built by embedding a
corpus survives an app restart without re-embedding every document. The embedder itself is not serialisable
(`Box<dyn Embedder>`), so it is supplied on `load`; the loaded vectors are only valid for the *same* embedding
model, so `load` verifies the embedder's `dim()` matches the persisted vector index and signals a rebuild on
mismatch.

## Design
- Hand-rolled little-endian binary, zero new deps, mirroring `VectorIndex`'s on-disk discipline (magic +
  format version, atomic temp-sibling + rename, bounds-checked `Reader`).
- File layout: `SEMIDX` magic (8 bytes) + `u32` format version + `u32 max_words` + `u32 overlap` +
  `u32 doc_count` + for each doc `{ u32 id_len, utf8 id, u32 chunk_count }` + the underlying
  `VectorIndex::to_bytes()` appended verbatim (its own magic/version guards its region).
- `save(&self, path)` serialises the above via temp+rename.
- `load(path, embedder)` parses the header, restores `docs` + chunking params, rebuilds the `VectorIndex`
  via `from_bytes`, then checks `embedder.dim() == index.dim()`.
- Error enum `SemanticIndexError { Io(String), Stale }`, matching the crate's `VectorIndexError` style:
  wrong magic/version OR embedder-dim mismatch ⇒ `Stale` (transparent rebuild signal); truncated/garbled
  body ⇒ `Io` (never a panic — every read is bounds-checked). `VectorIndexError` is folded into these
  (`Stale`→`Stale`, `Io`→`Io`).

## Acceptance Criteria
- [x] `SemanticIndex::save(path)` writes a versioned binary (magic+version, chunking params, docs map, vector
      bytes) atomically via temp+rename.
- [x] `SemanticIndex::load(path, embedder)` round-trips: `document_count`, chunking params, and search results
      preserved; a fresh embedder box is accepted.
- [x] Embedder-dim mismatch ⇒ `Stale` (rebuild), not a panic; bad magic ⇒ `Stale`; truncated body ⇒ `Io`.
- [x] `cargo test --lib semantic_index`, `cargo clippy --all-targets -D warnings`, and
      `cargo clippy --all-targets --features index -D warnings` all green.

## Work Log
- 2026-07-24: Read `semantic_index.rs`, `vector_index.rs`, `embedder.rs`. Implemented `SemanticIndexError`,
  `to_bytes`/`from_bytes`/`save`/`load` on `SemanticIndex`, and 5 tests (round-trip, wrong-dim⇒Stale,
  bad-magic⇒Stale, truncated⇒Io, chunking-params preserved). Verified with cargo test + both clippy modes.

## Resolution
Extended `crates/server/src/semantic_index.rs` only. Added `SemanticIndexError { Io(String), Stale }`
(Display + Error + `From<VectorIndexError>`), and `SemanticIndex::{to_bytes, from_bytes, save, load}`. The
format is `SEMIDX\0\0` magic + `u32` version + `u32 max_words` + `u32 overlap` + `u32 doc_count` + per-doc
`{len, utf8 id, chunk_count}` + the appended `VectorIndex::to_bytes()`. `load` takes the `Box<dyn Embedder>`
(unserialisable) and returns `Stale` when its `dim()` disagrees with the persisted index — a different model
means the saved vectors are meaningless. All reads go through a bounds-checked cursor, so a truncated file is
`Io`, never a panic. 5 new tests cover round-trip search parity + param preservation + the three failure
modes. No other file touched (headless).
