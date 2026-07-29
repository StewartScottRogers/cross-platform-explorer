---
id: CPE-892
title: Frontend RemoteTransport over WebSocket — the browser end of the CPE-811 envelope (slice 3)
type: feature
component: Frontend
priority: medium
tags: ready
epic: CPE-810
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Slice 3 of the browser remote transport (CPE-819). The transport seam in `src/lib/invoke.ts`
(`Transport` + `setTransport`, CPE-819 slice 0) had only the local in-process implementation; this adds
`RemoteTransport`, which runs the whole GUI against a headless `cpe-net` reference server over a
WebSocket. Every call site (`invoke`/`rawInvoke`) reaches the remote server unchanged — only the active
transport differs.

`RemoteTransport` is the browser end of the CPE-811 envelope the slice-2 server now speaks (#193):
opens the socket, completes the Hello→Welcome handshake, then turns each `invoke(cmd, args)` into a
`request` message (a WebSocket text frame) correlated by a monotonic `id`, resolving when the matching
`response` frame arrives. Errors surface as a typed `RemoteCallError` carrying the contract `code` +
`retryable`. A socket close rejects everything in flight and resets so the next call reconnects.

The socket factory is injectable, so the handshake + id-correlation logic is unit-testable headlessly
against a mock server.

## Decisions
- **Streaming (`stream_item`/`stream_end`) is deferred to a later slice.** This base transport carries
  the request/response surface (the ~113 explorer commands); the three `ipc::Channel` producers need a
  channel-shim over `stream_item` frames, which is its own slice. Logged rather than blocking.
- **`RemoteTransport` is not wired to config/startup here.** The seam defaults to local; selecting the
  remote transport at startup (from config) is a follow-up so this slice stays a pure, tested unit.

## Acceptance Criteria
- [x] `RemoteTransport implements Transport`; `invoke(cmd, args)` → `request` envelope → resolves on the
      correlated `response`.
- [x] Hello→Welcome handshake runs once per connection; a `rejected`/close rejects cleanly.
- [x] `Err` responses reject with a `RemoteCallError` carrying `code` + `retryable`.
- [x] Injectable socket factory; 5 headless unit tests (handshake, hello-first, error mapping,
      concurrent id-correlation, close-fails-in-flight) pass. `npm run check` clean.
