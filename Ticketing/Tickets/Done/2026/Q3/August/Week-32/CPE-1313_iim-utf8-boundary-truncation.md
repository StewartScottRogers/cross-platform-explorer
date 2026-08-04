---
id: CPE-1313
title: IPTC IIM push_iim_dataset truncates mid-UTF-8-codepoint at 0x7FFF
type: bug
component: Backend
priority: medium
tags: ready
created: 2026-08-04
epic: CPE-725
estimate: XS
---

## Summary
Found by the shift-3 bench researcher. `push_iim_dataset` in `crates/server/src/media_meta_write.rs`
(~line 1460) does `value.len().min(0x7FFF)` then slices `&value[..len]` on a `&str`'s bytes — a raw byte
clamp with no char-boundary check. An IPTC Caption/Keywords value ≥32767 bytes gets truncated
mid-UTF-8-codepoint, writing invalid UTF-8 into the IIM dataset (mojibake/corrupt bytes on reopen). Not a
panic, but a data-correctness bug.

## Acceptance Criteria
- [ ] `push_iim_dataset` truncates on a UTF-8 char boundary at or below 0x7FFF (e.g. walk back to the last
      `is_char_boundary`), never emitting a partial codepoint. IIM dataset length stays ≤ 0x7FFF.
- [ ] Regression test: a value with multi-byte chars straddling the 0x7FFF boundary → assert the written
      dataset value is valid UTF-8 (round-trips through read_iptc without corruption) and length ≤ 0x7FFF.
      Prove falsifiable (fails against the current raw-byte clamp).
- [ ] `cargo test -p cpe-server` green; clippy clean (3 cpe-server CI modes). No new deps.

## Work Log
2026-08-04 (workshift) — Filed by the Foreman from the shift-3 bench researcher (grep-verified real bug).
Dispatched to a worker.
