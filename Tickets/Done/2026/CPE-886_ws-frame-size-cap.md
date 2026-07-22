---
id: CPE-886
title: Cap inbound WebSocket frame size so a huge declared length can't abort the sidecar
type: bug
component: Sidecar
priority: high
tags: ready
epic: CPE-259
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
`ai-console::http::ws_read_frame` read the RFC 6455 payload-length field (up to a full 64-bit value) from
the client and immediately did `vec![0u8; len]` with **no upper bound**. A crafted frame declaring a huge
length (e.g. ~2^48) makes the sidecar attempt an enormous allocation; the allocation failure runs
`handle_alloc_error`, which **aborts the whole process** — so a single malformed WebSocket frame kills the
console sidecar (a DoS, reachable by anything that can open the local UI socket).

Fix: reject a frame whose declared length exceeds `MAX_WS_PAYLOAD` (64 MiB — far more than a terminal's
keystrokes/resizes/pastes ever need) **before** allocating, returning an `InvalidData` error that closes the
connection cleanly.

## Acceptance Criteria
- [x] A frame declaring `u64::MAX` length is rejected with `ErrorKind::InvalidData` before any allocation.
- [x] Normal frames (incl. the masked round-trip) still decode unchanged.
- [x] `ai-console` http tests (8) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — Found the unbounded `vec![0u8; len]` while auditing the hand-rolled WebSocket
  framing (the classic protocol-length DoS). Added a `MAX_WS_PAYLOAD` cap + a regression test that feeds a
  `127` + `u64::MAX` length header and asserts rejection. 8/8 http tests pass; clippy clean.
