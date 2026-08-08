---
id: CPE-1454
title: "net server read_ws_key does an unbounded read_line on the WS-upgrade headers → server-side DoS"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-810
created: 2026-08-08
---
## Vector (found in the net/webdav deep audit, 2026-08-08)
`crates/net/src/server.rs:~87-105`: the WS-upgrade header loop uses `reader.read_line(&mut line)` with NO `.take()`
cap. A client that sends `GET ` then streams bytes without a newline / blank line grows the buffer unbounded →
server memory-exhaustion. This mirrors exactly the CPE-1416 issue that `wire.rs` already fixed — the same cap
belongs here.

## Reachability
LOW — server-side, outside the "hostile server vs victim client" threat model, and `cpe-net` isn't shipped yet.
Real bug; fix alongside the network-stack hardening.

## Fix direction
Wrap the header read in a bounded `.take(N)` (e.g. a 64 KiB total header budget) and error past it, reusing the
CPE-1416 pattern. Add a test that an over-budget header stream errors rather than allocating unbounded.

## Effort / blast radius
S / server.rs. Epic CPE-810. Batch with CPE-1453 (same net crate).
