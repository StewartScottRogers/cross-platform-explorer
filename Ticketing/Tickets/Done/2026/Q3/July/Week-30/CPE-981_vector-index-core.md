---
id: CPE-981
title: Pure vector index core (cosine top-k + persistent store)
type: feature
component: Backend
priority: high
tags: ready
status: Done
created: 2026-07-24
epic: CPE-976
estimate: 3h
---

## Summary
First slice of the AI semantic-search epic (CPE-976), activated 2026-07-24. The **backend/model-agnostic**
core the whole feature is built on — exactly as `index_query` was for CPE-703's lexical search. A pure
`cpe-server::vector_index`: store per-item embeddings (`Vec<f32>`), find the nearest by **cosine similarity**
(top-k), and persist the index to disk. It commits to **no embedding model** (vectors arrive already
computed), so it needs zero attended decision and is reused whatever embedder CPE-982 lands.

## Design (pure, std-only, zero deps)
- `VectorIndex { dim, ids: Vec<String>, vectors: Vec<f32> }` — a flat row-major `dim × n` matrix. Vectors are
  **L2-normalised on insert**, so cosine similarity is a plain dot product (fast, and scores land in
  `[-1, 1]`).
- `new(dim)`, `add(id, &[f32]) -> Result` (dim-checked; replaces an existing id in place), `remove(id)`,
  `len`/`is_empty`/`dim`.
- `search(query, k) -> Vec<SearchHit{ id, score }>` — normalise the query, dot against every row, return the
  top-k best-first with a deterministic tiebreak (score desc, then id asc). A zero/degenerate query or empty
  index yields no hits.
- `to_bytes`/`from_bytes` + `save`/`load` — a hand-rolled versioned binary store (magic + version + dim +
  count + ids + f32 rows), atomic temp+rename; `VectorIndexError::{Io, Stale}` with transparent-rebuild on a
  format mismatch (mirrors CPE-832's store discipline). No new deps.
- Not feature-gated: pure std, tiny, zero runtime cost unless an index is built (like `restore_plan`/
  `snapshot`). The heavy *embedder* is the feature-gated part, in CPE-982.

## Acceptance Criteria
- [x] `VectorIndex` with add/replace/remove, dim-checking, L2-normalised storage.
- [x] `search` returns cosine top-k, best-first, deterministic tiebreak; nearest-vector correctness asserted.
- [x] `save`/`load` round-trips (bytes) and still searches; bad magic/version → `Stale`, truncated → `Io`,
      never a panic.
- [x] Cargo-tested; clippy `--all-targets -D warnings` clean both feature modes; no new deps (pure std).

## Notes
- Model-agnostic on purpose. CPE-982 (embedder seam, **big-design attended**), CPE-983 (pipeline), CPE-984
  (query blend), CPE-985 (UI) build on this. Persisted format is versioned so an embedding-dim/model change
  is a transparent rebuild, not a hard error.

## Work Log
- 2026-07-24 (dayshift) — Picked up as the first CPE-976 child; building the pure core.
