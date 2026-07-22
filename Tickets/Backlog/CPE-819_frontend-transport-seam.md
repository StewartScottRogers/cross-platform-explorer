---
id: CPE-819
title: Frontend pluggable transport seam (local IPC vs remote RPC) + streaming
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
epic: CPE-810
estimate: 3-4h
---

## Summary
Child of CPE-810. Make `src/lib/invoke.ts` — the single call chokepoint — route to either **local
Tauri IPC** or a **remote RPC** transport speaking the CPE-811 envelope, chosen by config, with the
GUI above it unchanged. Provide the network-streaming equivalent for the 3 `ipc::Channel` producers
(dir listing, name search) so the streaming-liveness convention survives the swap. Preserve the
busy-cursor wiring. Prereqs: CPE-811, CPE-815.

## Acceptance Criteria
- [ ] `invoke.ts` selects local-IPC or remote-RPC transport; call sites unchanged.
- [ ] The 3 streaming commands have a working over-the-wire form (batched, first rows paint immediately).
- [ ] Busy cursor + Diagnostics timing intact across both transports.
- [ ] GUI-verified against a local server through the remote path (loopback).

## Work Log
- 2026-07-22 (nightshift) — **Seam landed (foundation).** `invoke.ts` now routes every call through a
  pluggable `Transport` (interface + `localTransport` default + `setTransport`/`isRemoteTransport`); busy
  cursor + Diagnostics timing wrap the transport, so both survive the swap. Default is local Tauri IPC →
  in-process behaviour byte-for-byte unchanged. Unit tests: default→local passes args through; a swapped
  transport routes both `invoke` + `rawInvoke` and bypasses local IPC; `setTransport(null)` resets; a
  remote rejection still releases the busy cursor (invoke.test.ts 9/9). **AC #1 + #3 done.** Remaining
  (gated on CPE-820's reference server): a concrete remote `Transport` speaking the CPE-811 envelope, the
  3 streaming commands' over-the-wire form (AC #2), and GUI-verify against a loopback server (AC #4).
- 2026-07-22 (nightshift) — **Streaming wire mechanism landed (AC #2 core).** cpe-net now carries the
  streaming form over the socket: `ServerRuntime::with_stream_handler(method, fn)` registers a streaming
  method whose items are emitted as `StreamItem`s correlated by the request's envelope id, terminated by
  `StreamEnd`; the security chain is enforced on the stream exactly like a unary call (a denial/handler
  error arrives as a single `Response`). Client gains `call_stream(method, params) -> Vec<Value>` which
  collects items until `StreamEnd` and surfaces a denial as the stream's error. Tests: 3-item round-trip,
  empty stream (immediate end), and a default-deny stream that errors (net suite 14/14; clippy clean).
  Remaining for full AC #2: register the 3 real `ipc::Channel` producers (dir listing, name search) as
  incremental stream handlers, and wire the frontend remote `Transport` to `call_stream` (with 815's
  shared walkers yielding rather than collecting, for true first-rows-immediately). Frontend GUI-verify
  (AC #4) still needs a running server.
