---
id: CPE-416
title: "Search inside files (content search) — backend engine"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
The current search is filename-only (`matchesQuery(e.name, …)`). A good explorer can also search
*inside* files. This loop delivers the backend engine: a `search_file_contents(root, query,
case_sensitive)` command that walks a folder, skips binary/oversized/hidden noise, and returns
line matches (path + line number + snippet), bounded so it stays fast/predictable. The results UI is
the follow-up (CPE-417).

Nightshift research loop 5 — the highest-value confirmed gap.

## Acceptance Criteria
- [x] `search_file_contents(root, query, case_sensitive)` returns `{ matches:[{path,lineNumber,line}],
      filesScanned, truncated }`; empty query → empty; unreadable entries skipped (never fail whole).
- [x] Skips binary files (NUL sniff), files over a size cap, and dot-dirs (`.git`, etc.); caps total
      matches + files scanned and reports `truncated`.
- [x] Case-insensitive by default; `case_sensitive` honoured.
- [x] Unit tests over a temp tree (matches, case, binary skip, caps, nested); `cargo clippy` clean.

## Work Log
2026-07-15 — Nightshift loop 5. Estimate 1-2h. Backend-only engine + tests; UI in CPE-417.

2026-07-15 — Done. `search_file_contents(root, query, case_sensitive)` in `src-tauri/src/lib.rs`: explicit-stack recursive walk (skip-on-error like `list_dir`), skips dot-dirs / binary (NUL sniff) / >5 MB files, case-insensitive default, caps (1000 matches / 20k files / 400-char snippet) with a `truncated` flag. Registered. Unit test over a temp tree (recursive match, 1-based line numbers, case-sensitive filter, binary + .git skipped, empty-query + non-folder). `cargo test` pass, `cargo clippy` clean. UI follows in CPE-417.

## Resolution
Backend-only engine landed + tested; the command is invokable now, the results panel is CPE-417.
