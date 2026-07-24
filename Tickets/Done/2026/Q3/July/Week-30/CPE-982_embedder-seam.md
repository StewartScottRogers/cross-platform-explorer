---
id: CPE-982
title: Embedder seam + deterministic FakeEmbedder
type: feature
component: Backend
priority: high
tags: ready
status: Done
created: 2026-07-24
epic: CPE-976
estimate: 2h
---

## Summary
Second child of AI semantic search (CPE-976). The **pluggable embedding seam** — `trait Embedder` — plus a
dependency-free **`FakeEmbedder`** for tests/dev, so the rest of the feature (chunk→embed pipeline CPE-983,
query blend CPE-984) can be built and tested **end-to-end today**, before any real model is chosen. This is
the headless half the user greenlit ("build the seam now, decide later"); a **real** backend (bundled-local
vs external endpoint) stays the deferred big-design call.

## Design (pure, std-only, no deps)
- `trait Embedder { fn dim(&self) -> usize; fn embed(&self, text: &str) -> Vec<f32>; fn embed_batch(...) }`
  — object-safe (so a pipeline can hold `Box<dyn Embedder>`); `embed_batch` has a default impl over `embed`.
- `FakeEmbedder { dim }` — a deterministic **feature-hashed bag-of-words**: lowercase, tokenise on
  non-alphanumerics, hash each token (stable FNV-1a — **not** `DefaultHasher`, which isn't cross-run stable)
  into one of `dim` buckets, count. Same text → same vector; **shared tokens → non-zero cosine similarity**,
  so it's a genuinely useful stand-in for the vector index + pipeline (a query retrieves the most
  token-overlapping doc). Empty/tokenless text → a zero vector (which the index correctly never matches).
- Not feature-gated: pure std, zero deps. The *real* embedder (a model runtime / HTTP endpoint) is the
  feature-gated part, decided later.

## Acceptance Criteria
- [x] `Embedder` trait (object-safe) + `embed_batch` default; `FakeEmbedder` implementing it.
- [x] Deterministic (same text → identical vector), correct `dim`, stable across runs (FNV-1a, not
      `DefaultHasher`).
- [x] **End-to-end proof**: `FakeEmbedder` + `vector_index::VectorIndex` retrieves the most token-overlapping
      document for a query (the semantic-search happy path, headless).
- [x] Empty/tokenless text → zero vector; `embed_batch` == per-item `embed`.
- [x] Cargo-tested; clippy `--all-targets -D warnings` clean both modes; no new deps.

## Notes
- Real backend = the deferred big-design call (CPE-976 open question). CPE-983 (pipeline) + CPE-984 (query
  blend) build on this seam with `FakeEmbedder` in tests. Feeds [`vector_index`] (CPE-981).

## Work Log
- 2026-07-24 (dayshift) — Picked up per the user's "build the seam now" steer; building the trait + fake.
