---
id: CPE-1195
title: "Fix archive single-entry extract temp-path collision (macOS-red base fix)"
type: bug
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
**Base-red hotfix.** `archive::tests::extract_archive_entry_any_delegates_zip_to_the_zip_extractor` (CPE-1180)
failed on the **macos-latest** Server-crates CI leg (`archive.rs:928`, `fs::read(&tmp).unwrap()` on a
missing file) — reddening `main` at 73d32ba1. Root cause: `temp_extract_target(inner)` returned a **shared flat
path** `%TEMP%/cpe-archive/<basename>`, so two concurrent extractions of a same-named entry (`a.txt`) raced —
one call read a file another had already replaced/removed. Deterministic on the macOS CI leg under parallel
`cargo test`; also a genuine app hazard for two concurrent extract-and-opens of same-named files. (Local
Windows-only verification missed it — the 3-OS matrix caught it: [[ci-runs-three-os-backend-matrix]].)

## Fix
- `crates/server/src/archive.rs`: give each extraction a **unique subdir** `cpe-archive/<pid>-<seq>/<basename>`
  (process id + a monotonic `AtomicU64`), preserving the basename for the opened file while making concurrent
  extractions collision-free. No new deps.

## Acceptance Criteria
- [x] `cargo test -p cpe-server archive::` green under parallelism (17 passed, incl. the formerly-flaky test);
      `cargo clippy --all-targets -D warnings` clean.
- [ ] macos-latest Server-crates CI leg green on the fix commit (confirm post-merge).

## Work Log
- 2026-07-31 — Foreman hotfix during sprint. Verified locally; watch the macOS CI leg after merge.
