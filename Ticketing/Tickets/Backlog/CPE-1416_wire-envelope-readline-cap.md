---
id: CPE-1416
title: "Security: cap wire-envelope read_line to prevent unbounded memory growth from a peer (cpe-net)"
type: Bug
status: Backlog
priority: Medium
component: Backend
tags: [ready]
epic: CPE-810
created: 2026-08-07
---

## Problem (untrusted-parser scout, item #7)
`crates/net/src/wire.rs:read_envelope` reads a peer's framed message with an unbounded `read_line` (or
equivalent) — a peer (the `cpe-net` loop's socket data) that sends bytes but NEVER a `\n` makes the buffer grow
without limit = a memory-exhaustion DoS. The `serde_json` decode itself is panic-safe (returns Err), but the
line accumulation before it is not bounded.

## Fix direction
Cap the accepted line/frame length BEFORE `read_line` fills memory: use a `take(MAX)`-limited reader, or a manual
read loop that returns `Err("envelope too large")` once the accumulated bytes exceed a sane cap (envelopes are
JSON control messages — a few MB max is generous; pick a cap well above any legit envelope). Add a test: a
reader that yields > cap bytes with no newline returns Err promptly (bounded memory), and a normal envelope still
round-trips. `cargo test -p cpe-net` + `cargo clippy -p cpe-net --all-targets -- -D warnings` clean. (Local
`os error 225` = Defender, not a fail.) This is a real fix (production change), not just coverage.
