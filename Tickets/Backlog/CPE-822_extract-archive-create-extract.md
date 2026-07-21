---
id: CPE-822
title: Extract archive create/extract into cpe-server
type: refactor
component: Backend
priority: low
status: Open
tags: ready
created: 2026-07-20
epic: CPE-810
estimate: 2-3h
---

## Summary
Follow-up to CPE-815. Move the archive **create/extract** commands (compress to zip CPE-251; extract an
archive CPE-252; extract a single entry CPE-242) — `compress_to_zip`, `extract_archive` (zip/tar/tar.gz/7z),
`extract_archive_entry`, `extract_7z_safe` — into `cpe_server::archive` alongside the already-extracted
archive *listing*. The zip/tar/flate2/sevenz-rust crates already live in `cpe-server`, so this pulls no new
deps and lets `src-tauri` drop them if they become app-unused. Preserve the zip-slip / path-escape safety
(CPE-628/632/419) — those guards move with the code and their tests come along.

## Acceptance Criteria
- [ ] `compress_to_zip` / `extract_archive` / `extract_archive_entry` / `extract_7z_safe` move to
      `cpe_server::archive`; the commands dispatch.
- [ ] The zip-slip / self-inclusion / escape guards + their tests move with the code and pass headless.
- [ ] If zip/tar/flate2/sevenz-rust become app-unused, drop them from `src-tauri` (slim the plain explorer).
- [ ] clippy clean both modes; behaviour-preserving.

## Work Log
