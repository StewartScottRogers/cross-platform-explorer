---
id: CPE-976
title: "EPIC: AI semantic search — find files by meaning"
type: Task
status: In Progress
priority: High
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-24
closed:
---

> **Activated 2026-07-24** (dayshift, user chose "Activate CPE-976"). Starting — like CPE-703 did with
> `index_query` — with the **backend/model-agnostic pure vector-index core** (CPE-981), which commits to no
> embedding model and needs no attended decision. The genuinely big-design pieces (the **embedding backend**:
> bundled-local vs opt-in-external vs both) are isolated in a later child (CPE-982) and held for the user's
> attended confirm before building, exactly as the index-engine backend was for CPE-832.

## Decisions (activated 2026-07-24)
- **First slice = the pure vector index (CPE-981)**, not the embedder. Cosine top-k over stored `Vec<f32>`
  embeddings + a persistable on-disk store — pure, cargo-tested, reused whatever embedder lands. Zero rework
  risk; no model/privacy decision required yet.
- **Embedding backend (the big-design call, deferred to CPE-982):** leaning toward a **pluggable `Embedder`
  seam with a `FakeEmbedder` for tests**, and an *opt-in* real backend (a small bundled local model and/or an
  external endpoint) behind a feature gate — so the plain build pulls in no ML stack (lean-core /
  fast-when-off). **To be confirmed attended** before wiring a real model.
- **Delete-test / fast-when-off:** the vector core is pure std (zero deps, zero cost unless an index is
  built); the heavy embedder is the feature-gated part — the epic's hard DoD.

## Child tickets
1. **CPE-981** — Pure vector index (`cpe-server::vector_index`): store `(id, Vec<f32>)`, cosine-similarity
   `search` top-k, add/replace/remove, persistable binary store. Backend/model-agnostic, cargo-tested.
   *Headless — buildable now.*
2. **CPE-982** — Embedding provider seam (`trait Embedder` + `FakeEmbedder`). **Big-design: the real backend
   (bundled-local vs external) is the attended confirm.** *(prereq: 981)*
3. **CPE-983** — Chunk + extract → embed pipeline (reuse `doc_text`/`content_search`), incremental via the
   CPE-833 change signals. *(prereq: 981, 982)*
4. **CPE-984** — Query pipeline: embed the NL query → top-k → blend/re-rank with lexical hits
   (`spotlight_results`-style). *(prereq: 981, 982)*
5. **CPE-985** — NL search surface + ranked snippets. **GUI-verified — attended.** *(prereq: 984)*

## Goal
Find files by **meaning**, not just filename: type "the invoice from the plumber last spring" or "photos of
the whiteboard from the offsite" and get ranked hits from file *content* (document text, and later image
captions), independent of whether those words appear in the name. A local, privacy-respecting embedding
index over the indexed corpus, queried with natural language, complementing the lexical instant-search
([[CPE-703]] / `index_query`) rather than replacing it.

## Why
Everyone has files they can't find because they don't remember the name. Lexical/glob search (already strong
here) can't answer "the doc where I compared the two vendors." Semantic search is the headline capability
that makes this an **AI** file explorer, and it composes with the pieces already built: the crawl/index
(CPE-703), content extraction (`doc_text`, `content_search`), and — once OCR ([[CPE-980]]) lands — scanned
docs too. Delivered as a delete-testable mode: with it off, the plain explorer stays fast/small (the same
hard rule as CPE-703).

## Rough scope (areas, not child tickets)
- A **pure vector index**: store `(id, embedding)` per chunk, cosine-similarity top-k, persist per corpus —
  provider-agnostic (embeddings arrive as `Vec<f32>`), the backend-neutral core that commits to no model,
  exactly as `index_query` did for CPE-703.
- An **embedding provider seam** (`trait Embedder`): pluggable — a bundled small local model, or an opt-in
  external endpoint — behind a feature gate so the plain build pulls in no ML stack (lean-core).
- A **chunk + extract pipeline**: reuse `doc_text`/`content_search` to turn files into text chunks to embed;
  incremental (re-embed only changed files, reusing the CPE-833 change signals).
- A **query pipeline**: embed the NL query → top-k over the index → merge/re-rank with lexical hits into one
  result list (reuse `spotlight_results`-style grouping).
- A results surface: NL search box + ranked snippets with why-matched context.

## Open questions (resolve at activation — big-design)
- **Embedding backend:** bundle a small local model (size/perf/licensing) vs. a pluggable external endpoint
  vs. both? This is the central big-design call (privacy, binary size, offline, the fast/small tiebreaker).
- Index footprint + on-disk format for vectors (quantise? budget per corpus, mirroring CPE-832's budget).
- Chunking granularity (per-file vs per-paragraph) and how semantic + lexical scores combine.
- Staleness/re-embed policy on change; cost control if an external embedder is used.

## Definition of Done
- A natural-language query returns meaning-ranked file hits from content, blended with lexical matches.
- The index stays current as files change; re-embedding is incremental, not a full rescan.
- With the mode disabled, no embedder/model loads and there is zero measurable startup/memory cost.

## Notes
- Flagship AI feature. Build order should mirror CPE-703: **the pure vector-index core first**
  (backend/model-agnostic, cargo-tested), then the embedder seam, then the pipeline + UI. Composes with
  [[CPE-978]] (a semantic query can back a smart folder) and [[CPE-980]] (OCR feeds scanned docs in).
  See [[headless-frontier-and-cpe-net]], [[prefer-streaming-liveness]].

2026-07-24 (dayshift) — **CPE-982** landed the embedder seam: `embedder::Embedder` (object-safe trait) + a dependency-free deterministic `FakeEmbedder` (feature-hashed bag-of-words, stable FNV-1a). Proven end-to-end with `vector_index` (token-overlap retrieval). CPE-983 (pipeline) / CPE-984 (query blend) can now build against a real `Embedder` with zero model weight; the **real backend stays the deferred big-design call** (user: "build the seam now, decide later").

2026-07-24 (dayshift) — **CPE-983** landed the document pipeline: `semantic_index::{chunk_text, SemanticIndex}` — chunk (overlapping word windows) → `embed_batch` → `vector_index`, with `upsert_document`/`remove_document` (exact chunk bookkeeping) and per-document `search` (best chunk per doc, only positive-similarity docs). 7 tests via FakeEmbedder. Next: CPE-984 blend with lexical hits; then CPE-985 UI (attended) + the deferred real embedder.
