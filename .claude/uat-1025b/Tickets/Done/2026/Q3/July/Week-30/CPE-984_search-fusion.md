---
id: CPE-984
title: Blend semantic + lexical search results (rank fusion)
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
Fourth child of AI semantic search (CPE-976). Merges the **semantic** `DocHit`s from CPE-983 with the
**lexical** hits (filename/`index_query`, CPE-703/831) into one ranked list — so a query surfaces both
meaning-matches and exact name/term matches, best of both. Uses **Reciprocal Rank Fusion (RRF)**: scale-free
(it fuses *ranks*, not the incomparable cosine-vs-term-score magnitudes), simple, and robust. Pure + tested.

## Design (pure, std-only)
- `rrf(rankings: &[Vec<String>], k) -> Vec<Fused>` — generic RRF: each id scores `Σ 1/(k + rank)` over the
  rankings it appears in; returned best-first, deterministic tiebreak (score desc, id asc). `k` defaults to
  the standard 60.
- `blend_semantic_lexical(semantic: &[DocHit], lexical: &[String]) -> Vec<BlendedHit>` — the convenience
  over the two search sources, carrying provenance (`in_semantic` / `in_lexical`) so the UI can badge *why*
  a result showed (matched meaning, name, or both). An id in **both** naturally rises (two contributions).
- No score-normalisation guesswork; a doc present in only one source still ranks fairly.

## Acceptance Criteria
- [x] `rrf` fuses N ranked id lists; an id ranked high in multiple sources outranks one high in a single
      source; deterministic tiebreak.
- [x] `blend_semantic_lexical` merges the two sources with correct provenance flags; a both-sources hit
      outranks equal single-source hits.
- [x] Empty inputs → empty; a single source degrades to that source's order.
- [x] Cargo-tested; clippy clean both modes; no new deps (pure std).

## Notes
- The final headless slice of the CPE-976 query path; CPE-985 (NL search surface) is the attended GUI, and
  the real embedder backend stays the deferred big-design call. `DocHit` reused from `semantic_index`.

## Work Log
- 2026-07-24 (dayshift) — Building RRF fusion over the semantic + lexical rankings.
