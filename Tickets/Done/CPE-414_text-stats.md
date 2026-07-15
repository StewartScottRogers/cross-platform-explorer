---
id: CPE-414
title: "Text file stats (lines / words / characters) in Properties"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Add an on-demand content summary to Properties for text/code files: **N lines · N words · N
characters**. Opt-in (a Count button) like the checksum, since reading the file is I/O work.
Nightshift research loop 3 — a distinct capability (content analysis) from the checksum loops.

## Acceptance Criteria
- [x] Backend `text_stats(path)` returns lines (str::lines semantics) / words (whitespace) /
      chars (Unicode scalars) / bytes; dir, non-UTF-8, or over-cap (25 MB) → Err, never a panic.
- [x] Properties shows a Contents row **only for text/code files**; opt-in Count → counts, error state.
- [x] Verified: backend unit test + `npm run check` clean + JS suite green; `cargo clippy` clean.

## Work Log
2026-07-15 — Nightshift loop 3. Estimate 1h. Backend `text_stats` (+test: 2 lines/3 words/16 chars,
final-unterminated-line, binary→Err, dir→Err) registered; Properties `Contents` row gated on
`categoryOf ∈ {text, code}`, opt-in Count, reusing the checksum UX; component tests (offered for
text, hidden for image). Verified headlessly: `cargo test text_stats` + `cargo clippy` clean,
`npm run check` 0/0, `npm test` **401 passed**. GUI left for the user (Nightshift machine-share rule).

## Resolution
`src-tauri/src/lib.rs` (`text_stats` command + `TextStats` + cap + test), registered in the handler;
`src/lib/components/PropertiesDialog.svelte` (`Contents` row, text/code-gated, opt-in) + tests.
Tradeoff: 25 MB cap keeps it fast/predictable; larger files report a clear limit message.
