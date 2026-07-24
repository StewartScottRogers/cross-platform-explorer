---
id: CPE-832
title: Instant-search index engine — backend design + build
type: feature
component: Backend
priority: medium
status: Open
tags: big-design
created: 2026-07-24
epic: CPE-703
estimate: 4h+
---

## Summary
The compact filename index that CPE-831's query core (`cpe_server::index_query`) runs against: an initial
crawl + a persistent per-volume on-disk store, returning cross-volume matches in <100ms warm. This ticket
is the **big-design confirmation** the epic held for attended review — the analysis + recommendation are
below; **the build starts once the backend choice is confirmed.** Written 2026-07-24 (user said "do it
all"; this is the writeup, not a blind build).

## The decision: index backend
Three realistic options, judged against the epic's hard tiebreaker (fast/small/**zero cost when off**) and
the target (<100ms warm, cross-volume, filename-only).

### Option A — roll our own compact filename index  ✅ recommended
A purpose-built structure: per volume, a flat `Vec` of entries `{ name, parent_id, flags }` with names
interned, plus a **case-folded trigram → posting-list** map for substring/fuzzy and a sorted prefix table
for `name:` / `ext:` fast paths. Persisted as a single memory-mappable file per volume (bincode/rkyv-style
fixed layout), loaded lazily.
- **Pros:** smallest footprint (filenames only — no content, no positional postings); startup is an `mmap`,
  not a parse; **zero deps beyond what's vendored**; honours "small when off" (the file just isn't opened);
  full control over the <100ms path (CPE-831 already owns match/rank, so this only has to *feed candidates*).
- **Cons:** we own the crawl, the on-disk format + versioning, and incremental updates (CPE-833). ~The most
  code, but all of it is the pure, cargo-testable kind this repo already does well (see `cpe-server`).~
- **Footprint estimate:** ~40–70 bytes/file resident (interned name + ids + trigram postings). 1M files ≈
  40–70 MB warm, less on disk. Tunable by dropping trigrams for pure-prefix mode.

### Option B — SQLite + FTS5
Store `(path, name, ext)` rows; FTS5 (or a plain index + `LIKE` with a trigram tokenizer) for lookup.
- **Pros:** durable, transactional incremental updates for free; well-understood; `rusqlite` already
  appeared in the tree historically (CPE-815 removed it as a direct dep, but it's a known quantity).
- **Cons:** FTS5 is a **full-text** engine — heavier than a filename index needs; substring (`%foo%`)
  defeats the default tokenizer (needs a trigram tokenizer or `LIKE` scans); a DB handle + WAL is real
  "when-on" weight, and pulling SQLite back as a hard dep cuts against the lean-core rule. ~100ms at 1M+
  rows for unanchored substring is not guaranteed without the trigram tokenizer, at which point we're
  building Option A's index *inside* SQLite.

### Option C — tantivy (embedded search engine)
- **Pros:** batteries-included ranked full-text, mature.
- **Cons:** by far the heaviest (a Lucene-class engine + its deps) for a **filename-only** need; large
  binary/size cost; overkill for substring-on-names; strongly against the fast/small tiebreaker. Rejected.

**Recommendation: Option A.** It's the only one that fully honours the epic's "zero cost when off / small
when on" DoD, reuses CPE-831 as the whole query brain, and keeps the work in the pure-Rust, cargo-tested
lane. B is the fallback if incremental-update correctness (CPE-833) proves too costly to own; C is out.

## Proposed shape (Option A)
- New `cpe-server::index` module, **feature-gated OFF by default** (`index` cargo feature) so the plain
  build compiles zero indexer — the delete-test.
- `Index` type: per-volume `mmap`-backed store; `build(root, cancel) -> Index` (initial crawl, reusing the
  `list_dir` skip-on-error discipline); `save/load(path)`; `candidates(&self) -> impl Iterator<Candidate>`
  feeding CPE-831's `rank`.
- On-disk: a versioned header (`magic`, `format_version`, `volume_id`, `entry_count`) + interned-name blob
  + entry table + trigram postings. A format-version mismatch → transparent rebuild (never a hard error).
- Crawl is cancellable + streamed (STREAMING.md) so first results paint before the crawl finishes.
- **Staleness/rebuild:** full rebuild on first use / format bump; CPE-833 keeps it live incrementally.

## Open questions for the user (the "attended confirm")
1. **Approve Option A** (roll-our-own) vs prefer B (SQLite/FTS)?
2. **Disk-footprint budget** per volume (drives whether trigrams are always-on or opt-in for huge volumes)?
3. **Scope of the first index build**: user home + mounted fixed drives by default, or explicit opt-in per
   volume? (Affects crawl time + the privilege story on Windows.)

## Acceptance Criteria (once confirmed)
- [ ] `cpe-server::index` behind an off-by-default feature; plain build unaffected (zero indexer code).
- [ ] `build` (cancellable crawl) + `save`/`load` (mmap) + `candidates` feeding `index_query::rank`.
- [ ] Versioned on-disk format with transparent rebuild on mismatch; unreadable entries skipped.
- [ ] Cargo-tested: build a temp tree, save, reload, query via CPE-831, assert <-budget + correct matches.
- [ ] Warm cross-volume query <100ms on a representative corpus (bench, ties into CPE-691 harness).

## Notes
- Prereq CPE-831 is **done** (query core). CPE-833 (live watch) and CPE-834 (overlay UI) follow this.
- Ties to [[headless-frontier-and-cpe-net]]: this was flagged as the index-engine big-design gate.
