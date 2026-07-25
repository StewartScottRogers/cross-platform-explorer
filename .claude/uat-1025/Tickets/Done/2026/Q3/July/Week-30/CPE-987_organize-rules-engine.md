---
id: CPE-987
title: Pure organization rules engine (declutter planner)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-979
---

# CPE-987 — Pure organization rules engine

## Summary

A PURE, deterministic, filesystem-free organization **rules engine** for the AI
auto-organize & declutter epic (CPE-979). No AI, no I/O. Given a flat list of
directory entries (already gathered by the caller) and a chosen rule, it computes
the proposed destination subfolder for each **file**. Directories are left in
place. The real move engine (a later ticket) executes these proposals.

Lives in `crates/server/src/organize.rs`, pure `std`, zero new dependencies.

## Design

- `OrganizeEntry { name, is_dir, ext /* lowercased, no dot, "" if none */, size,
  modified_secs /* unix epoch seconds */ }` — the filesystem-free input row.
- `OrganizeRule` enum: `ByKind`, `ByExtension`, `ByModifiedYear`, `BySizeBucket`.
- `MoveProposal { name, target_subdir }` — proposed destination subfolder per file.
- `plan_organize(entries, rule) -> Vec<MoveProposal>`:
  - Skips `is_dir` entries (never proposes moving a directory).
  - Produces one proposal per file, in the **input order** (stable/deterministic).
  - `ByKind`: maps `ext` → a category folder (Images / Documents / Audio / Video /
    Archives / Code / Other). Unknown/extensionless → `Other`.
  - `ByExtension`: `target_subdir` = uppercased ext, or `NoExtension`.
  - `BySizeBucket`: `Tiny` (<1 MiB) / `Small` (<100 MiB) / `Large` (>=100 MiB).
  - `ByModifiedYear`: 4-digit year computed from `modified_secs` with a pure
    integer civil-date calc (NO chrono / NO new deps).

### Year-from-epoch approach

`modified_secs` is UTC unix epoch seconds. Year is derived without chrono using
Howard Hinnant's well-known `civil_from_days` algorithm (public-domain), adapted
to integer arithmetic: take `days = secs / 86400`, shift the epoch so March 1st is
the start of the internal year (absorbs the leap-day into the year end), then
recover the civil year via era math. Exact for all in-range dates and needs no
leap-year special-casing beyond the algorithm. Pre-1970 is not representable
(epoch seconds are unsigned) and is out of scope.

## Acceptance Criteria

- [x] `OrganizeEntry`, `OrganizeRule`, `MoveProposal`, `plan_organize` implemented, pure std.
- [x] `ByKind` groups images/documents/audio/video/archives/code correctly; extensionless/unknown → `Other`.
- [x] Directories (`is_dir`) are never proposed for moving.
- [x] `ByExtension` → uppercased ext or `NoExtension`; `BySizeBucket` → Tiny/Small/Large; `ByModifiedYear` → 4-digit year.
- [x] Output order is stable/deterministic (input order).
- [x] `#[cfg(test)] mod tests` covers all rules + edge cases; all in-memory fixtures.
- [x] `pub mod organize;` added to `crates/server/src/lib.rs` with a doc comment.
- [x] `cargo test organize::` passes; `cargo clippy --all-targets` clean in both default and `index` feature modes.

## Work Log

- 2026-07-24: Read `restore_plan.rs` + `duplicates.rs` for house style (module
  doc-comment header, `#[cfg(test)] mod tests` with small fixture helpers,
  deterministic ordering). Matched that idiom.
- 2026-07-24: Assumption — one proposal per file even when the file already sits in
  a folder implying its target (ticket says keep it simple). The caller/move engine
  can no-op a same-target move later.
- 2026-07-24: Assumption — `ext` is trusted to already be lowercased & dot-free per
  the struct contract; `ByExtension` uppercases it for the folder name.
- 2026-07-24: Year-from-epoch uses the integer `civil_from_days` era algorithm (no
  chrono, no new deps). Verified against known dates in tests (1970, 2000 leap,
  2024 leap, 2026).
- 2026-07-24: `cargo test organize::`, `cargo clippy --all-targets -- -D warnings`
  (default) and `--features index` all green.
