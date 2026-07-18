---
id: CPE-633
title: "hex_dump slurped the whole file into memory to show its first bytes"
type: Bug
component: Backend
priority: medium
status: Done
tags: ready
estimate: 20m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`hex_dump(path, max)` did `fs::read(path)` (reads the ENTIRE file into memory) and only then truncated
to `max` bytes for the dump. Previewing a large binary therefore loaded the whole file — a multi-GB file
could exhaust memory or hang the app just to show its first few hundred bytes. Read only `max` bytes.

## Acceptance Criteria
- [x] `hex_dump` reads at most `max` bytes (`File::take(max)`), regardless of file size.
- [x] The dump output is unchanged for the capped case (regression test).
- [x] cargo test + clippy clean.

## Resolution
`src-tauri/src/lib.rs`: replaced `fs::read` with `File::open(..).take(max).read_to_end(..)`. Added
`hex_dump_caps_output_at_max_bytes`. (`text_stats` already caps by metadata before reading — left as is.)

## Work Log
2026-07-18 (dayshift) — Found auditing the file-preview readers for unbounded reads.
