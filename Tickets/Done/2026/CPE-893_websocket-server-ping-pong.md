---
id: CPE-893
title: WebSocket server must answer client Ping with Pong (keepalive; RFC 6455 §5.5.2)
type: bug
component: Sidecar
priority: medium
tags: ready
epic: CPE-810
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The WebSocket-accepting reference server (CPE-819 slice 2, #193) ignored inbound control frames: its
`WsIo::read_env` matched a Ping under the catch-all `Some(_) => continue` and never replied. RFC 6455
§5.5.2 requires an endpoint to answer a Ping with a Pong echoing its payload — browsers and reverse
proxies send Pings as keepalives, and an unanswered Ping causes them to drop the connection mid-session.
For the remote-transport GUI (CPE-819) that would surface as the connection silently dying during idle
periods.

Fix: in `WsIo::read_env`, handle `op::PING` by writing a `PONG` frame with the same payload before
continuing to await the next envelope. (Pong/continuation stay ignored; Close and data frames unchanged.)

## Acceptance Criteria
- [x] A client Ping after the upgrade is answered with a Pong echoing the payload.
- [x] Data/handshake path unchanged (all prior cpe-net tests green).
- [x] `cargo test` (net suite 23/23) + `cargo clippy --all-targets -D warnings` clean.

## Work Log
- 2026-07-22 (nightshift) — Found while reviewing the just-merged slice 2. Added the Pong reply +
  `a_websocket_ping_is_answered_with_a_pong` integration test (masked Ping right after upgrade → Pong
  with matching payload). net suite 22→23.
