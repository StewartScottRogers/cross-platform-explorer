---
id: CPE-1014
title: Fix single 0x00 byte misclassified as UTF-16 in encoding sniffer
type: bug
component: Backend
priority: low
tags: ready
status: Done
created: 2026-07-24
closed: 2026-07-24
epic: CPE-1002
estimate: 20m
---

## Summary
Found by the 2026-07-24 sprint bug-audit. In `crates/server/src/text_encoding.rs`, `classify_nul_bytes`
computed `lane = (sniff_len / 2).max(1)`. For a **single-byte** input `[0x00]`, `sniff_len == 1`, `lane == 1`,
`even_nul == 1`, so `even_nul * 2 >= lane && odd_nul == 0` held and it returned `EncodingGuess::Utf16Be`. A
1-byte file can never be valid UTF-16 (needs ≥2 bytes), so this was a spurious positive — should be `Binary`.

## Fix (shipped, PR #337)
Guard the degenerate short case before the lane math (UTF-16 needs ≥2 bytes):
```rust
if sniff_len < 2 {
    return EncodingGuess::Binary;
}
```

## Acceptance Criteria
- [x] `detect_encoding(&[0x00])` returns `EncodingGuess::Binary` — regression test added.
- [x] Existing `text_encoding` tests still pass; real UTF-16 detection (≥2 bytes) unchanged.
- [x] `cargo test -p cpe-server text_encoding` green (21/21); clippy clean both feature modes; no new deps.

## Work Log
2026-07-24 (sprint) — Diagnosed by the audit researcher, fixed by a worker, independently reviewed
(APPROVE). Merged in PR #337.
