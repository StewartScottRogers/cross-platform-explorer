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
- 2026-07-22 (nightshift) — **Incremental streaming (AC #2 "first rows immediately").** Evolved the
  `StreamHandler` from Vec-returning to an emit-callback (`&mut StreamSink`) that writes each `StreamItem`
  to the socket *as produced*; the server pushes items live and stops early (ControlFlow::Break) if the
  peer disconnects. `list_dir_stream` now drives cpe-server's incremental `stream_dir_entries` walker, so
  a large directory paints its first rows immediately instead of collecting the whole listing first. net
  suite 15/15; clippy clean. AC #2 is now substantially complete over the wire; remaining is wiring the
  frontend remote `Transport` to `call_stream` (needs the RemoteTransport impl) + GUI-verify (AC #4).
- 2026-07-22 (nightshift) — **All 3 stream producers now over the wire.** Added content_search_stream
  (drives CPE-662's new stream_file_contents walker) alongside list_dir_stream + name_search_stream, all
  incremental + security-guarded. AC #2's producer coverage is complete server-side (net suite 17/17).
  What remains for full AC #2/#4: the frontend remote Transport speaking these over a browser-reachable
  transport (WebSocket bridge — cpe-net is raw TCP) + GUI-verify against a live server. Those need an
  architecture decision (WS vs the CPE-812 bindings) + a running server — user-facing, not headless.
- 2026-07-22 (nightshift) — **Browser transport chosen + built, unary path end-to-end (slices 1-3).**
  Decision (logged, per go-with-recommendation): **WebSocket** is the remote transport — the only
  browser-native bidirectional channel that carries both requests and `StreamItem` streaming (SSE is
  one-way, gRPC needs a proxy, raw TCP is impossible in a browser). Shipped:
  - **Slice 1 (#192):** `cpe-net::ws` — RFC 6455 codec (`accept_key`, masked `read_frame`/`write_text`,
    `MAX_WS_PAYLOAD` DoS cap). sha1+base64 the only new deps; still thread-per-conn, no async runtime.
  - **Slice 2 (#193):** the reference server **accepts WebSocket clients** on the same listener — peek the
    first bytes (`GET ` ⇒ 101 upgrade, else raw wire), then both transports speak CPE-811 envelopes
    through one shared `run_session` behind an `EnvelopeIo` seam (`WireIo`/`WsIo`). Integration test drives
    the full browser path (upgrade→Hello→Welcome→list_dir→Response); 17 raw-wire tests unchanged (22/22).
  - **Slice 3 (#195, CPE-892):** frontend **`RemoteTransport implements Transport`** — Hello→Welcome
    handshake, each `invoke` → a `request` text frame correlated by monotonic id, `Err`→typed
    `RemoteCallError`, close rejects in-flight. Injectable socket factory; 5 headless tests.
  - **AC #1/#3 done; AC #2 unary path done end-to-end.** **Remaining (next slice):** streaming over the
    remote transport. Blocker to design first: the local `*_stream` commands **return a final stats value**
    (files_scanned/truncated/total) alongside the Channel, but the wire protocol ends a stream with a
    payload-less `StreamEnd` — so remote streaming currently can't deliver those stats. Needs a
    contract-touching decision (give `StreamEnd` a final-value payload, or emit a terminal `Response` after
    the items) + a seam-owned channel abstraction (Tauri's `Channel` can't run in a real browser). Left in
    Backlog. AC #4 (GUI-verify vs a loopback server) still needs a running server + browser (user-facing).
- 2026-07-22 (nightshift) — **Wire+net streaming stats landed (CPE-895, #198).** Resolved the blocker
  above: `Message::StreamEnd` now carries a JSON `result` (a **struct** variant — a newtype wrapping a
  scalar can't serialize under the internally-tagged enum); the `StreamHandler` returns its terminal value,
  the 3 builtin handlers return their stats, and the client's `call_stream` surfaces
  `StreamOutcome { items, result }`. contract 10/10, net 23/23. **AC #2 is now complete over the wire in
  Rust** (items stream incrementally + the terminal stats arrive on StreamEnd). **The only remaining piece
  is frontend-only:** a seam-owned channel abstraction in `invoke.ts` (`createChannel`) that returns a real
  Tauri `Channel` under `localTransport` but a `RemoteChannel` under `RemoteTransport`, plus teaching
  `RemoteTransport` to route `stream_item` frames → the channel's `onmessage` and resolve the `invoke` with
  `StreamEnd.result`; then repoint the ~5 `*_stream` call sites (list_dir, name/content search, duplicates,
  disk usage) at `createChannel`. That's a production-component refactor (local path must stay byte-for-byte
  identical) — its own slice. AC #4 GUI-verify still user-gated.
