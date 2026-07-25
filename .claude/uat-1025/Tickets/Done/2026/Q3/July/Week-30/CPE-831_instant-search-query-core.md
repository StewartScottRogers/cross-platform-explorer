---
id: CPE-831
title: Instant-search query grammar + ranked-result model
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-703
estimate: 2h
---

## Summary
First child of CPE-703 (instant index search). The **backend-agnostic** query core — reused by whatever
index backend CPE-832 lands — as a pure `cpe-server::index_query` module:

- **`parse(input)`** → a structured `Query`: space-separated tokens, where `ext:png,jpg` and `path:foo`
  are structured filters and every other token is a **name term** (substring, or a glob with `*`/`?` and
  `{a,b}` brace groups). All terms AND together.
- **`matches(&Query, &Candidate)`** — evaluates a candidate `{name, path, ext}` against the query, reusing
  `name_search`'s existing glob/brace/substring matcher (`name_matches`, exposed for reuse) so instant
  search and folder search share one matching semantics.
- **`score` / `rank`** — a relevance model: exact name > prefix > word-boundary > substring, with shorter
  paths as a tiebreak; `rank` filters to matches and returns them best-first with a total, stable order.

Pure and fully cargo-tested; no volume access, no privileges, no new dependency. The index engine
(CPE-832), change-watcher (CPE-833), and search overlay (CPE-834) build on this.

## Acceptance Criteria
- [x] `parse` handles `ext:` (comma lists, leading dot tolerated), `path:`, and name terms; case-insensitive.
- [x] `matches` ANDs all terms: name terms via the shared glob/substring matcher, `ext` any-of, `path`
      substrings; an empty query matches nothing.
- [x] `score` ranks exact > prefix > word-boundary > substring; `rank` returns matches best-first with a
      deterministic total order (name then path tiebreak).
- [x] `name_search::name_matches` is exposed (made `pub`) and reused — no second glob implementation.
- [x] `cargo test` green in `crates/server` (138 passed); `cargo clippy --all-targets -D warnings` clean.
      App untouched.

## Resolution
Added **`cpe-server::index_query`** — the backend-agnostic query core of the instant search:

- `Query { name_terms, exts, path_terms }` + `parse(input)` — `ext:png,jpg` / `path:foo` structured
  filters, everything else a name term; case-insensitive, leading dot on extensions tolerated.
- `matches(&Query, &Candidate{name,path,ext})` — ANDs all constraints; name terms go through
  `name_search::name_matches` (exposed `pub`) so instant search and folder search share one glob/brace/
  substring semantics. Empty query matches nothing.
- `score` / `rank` — relevance model (exact 100 > prefix 70 > whole-word 55 > substring 30; glob terms a
  flat 40) with a shorter-path tiebreak scaled so term score stays primary; `rank` filters + returns
  best-first with a deterministic total order (score, then name, then path).

Files: `crates/server/src/index_query.rs` (impl + 8 tests), registered in `lib.rs`;
`crates/server/src/name_search.rs` — `name_matches` made `pub` (reused, no second glob impl).

Verification (local, Windows): `cargo test` in `crates/server` → **138 passed** (was 130); `cargo clippy
--all-targets -D warnings` clean. Pure — no volume/privilege/dep. App untouched.

Scope/next: this is the piece reused by *any* index backend, so it commits to no architecture. The index
engine (CPE-832, big-design — confirm roll-our-own vs embedded attended), the live change-watcher
(CPE-833), and the search overlay (CPE-834, GUI) build on it.

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous) after activating epic CPE-703. Estimate 2h. Built the
  backend-agnostic query core first (zero architecture lock-in) rather than the big-design index engine.
  Reused `name_search`'s existing glob/brace matcher (exposed it) instead of a second implementation.
  8 tests; full suite 138 green; clippy clean. Closing.
