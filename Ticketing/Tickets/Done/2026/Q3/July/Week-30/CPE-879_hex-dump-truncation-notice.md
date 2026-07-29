---
id: CPE-879
title: Fix hex-dump "more bytes" truncation notice never appearing
type: bug
component: Server
priority: medium
tags: ready
epic: CPE-706
created: 2026-07-21
closed: 2026-07-21
status: Done
---

## Summary
`binary_preview::hex_dump` caps its read with `.take(max)` and then set `let n = bytes.len()`, so the
truncation notice guarded by `if bytes.len() > n` was **unreachable dead code** — `bytes.len()` always
equals `n`. Result: the hex preview of a file larger than `max` (e.g. a multi-GB binary) showed exactly
`max` bytes with **no indication the file continues**, so it looked complete.

Fix: capture the file's true length from `metadata()` before the capped read and compare against the bytes
actually shown, so the notice fires (and now reports shown/total accurately).

## Bug
- Before: viewing a big binary's hex dump silently omitted the "… N more bytes" hint — misleading.
- After: `… 9968 more bytes (showing first 32 of 10000).` when the dump is partial; no notice when the
  whole file fits.

## Acceptance Criteria
- [x] A partial dump appends a notice with the remaining byte count and shown/total.
- [x] A fully-shown file has no notice; a 0-byte file yields empty output (no panic, no notice).
- [x] Existing cap behavior (never dumps past `max`) unchanged.
- [x] `cargo test` + `cargo clippy --all-targets -D warnings` green in `cpe-server`.

## Work Log
- 2026-07-21 (autonomous) — Found the dead-code notice while auditing bounds handling of the binary-preview
  providers. Fixed via `metadata().len()`; added tests for the partial, complete, and empty cases (the
  partial-notice assertion would have failed against the old code). 4/4 hex tests pass; clippy clean.
