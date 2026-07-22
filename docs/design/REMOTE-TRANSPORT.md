# Remote transport (WebSocket) — design

**Epic CPE-810 · tickets CPE-819 / CPE-820.** How the GUI reaches a **remote** headless server
instead of in-process Tauri IPC, and why it's a WebSocket carrying the CPE-811 envelope.

This is the *browser-reachable* half of the client/server epic. The Rust-to-Rust path
(`Client(Rust)` ↔ `cpe-server-ref` over raw length-prefixed TCP) is described in
[SERVER-ARCHITECTURE.md](SERVER-ARCHITECTURE.md); this doc covers the transport the **frontend** speaks.

## The one seam

`src/lib/invoke.ts` is the single chokepoint every backend call flows through (busy-cursor + Diagnostics
timing wrap it — see [BUSY-CURSOR.md](BUSY-CURSOR.md)). It exposes a `Transport` interface and
`setTransport()`:

```
interface Transport { invoke<T>(cmd, args): Promise<T> }
```

- `localTransport` (default) → in-process Tauri IPC. Zero overhead; single-user behaviour is byte-for-byte
  unchanged when no remote transport is selected.
- `RemoteTransport` (`src/lib/remoteTransport.ts`) → a `cpe-net` server over a WebSocket.

Every call site imports `invoke`/`rawInvoke` unchanged; only the active transport differs. The seam
defaults to local, so the plain explorer's fast/small/predictable path is never touched by the remote
code existing.

## Why WebSocket

A browser client needs a **bidirectional** channel: it sends requests *and* receives streamed
`StreamItem`s (directory listings, searches). The options:

| Transport | Verdict |
|-----------|---------|
| Raw TCP (the Rust client's wire) | Impossible from a browser — no raw sockets. |
| HTTP request/response | No server-push; streaming needs polling. |
| SSE (EventSource) | One-way (server→client) only. |
| gRPC-web | Needs a proxy (Envoy) + codegen; heavy. |
| **WebSocket (RFC 6455)** | **Browser-native, full-duplex, carries requests + streaming.** Chosen. |

The server stays `std`-only, thread-per-connection, no async runtime — the WebSocket handshake needs only
two tiny crates (`sha1` + `base64`) for `Sec-WebSocket-Accept`.

## The wire

The **CPE-811 `Envelope`** (`cpe-contract`) is the payload; WebSocket is just framing. An envelope JSON
rides as one WebSocket **text frame**:

```
Envelope { schema_version, id, session?, message }
Message  = { type: "hello" | "welcome" | "rejected" | "request" | "response"
                   | "stream_item" | "stream_end" | "error", …fields }
```

`Message` is internally tagged (`#[serde(tag = "type")]`), so a request is
`{"type":"request","method":"list_dir","params":{…}}` and a response is
`{"type":"response","result":{"Ok":…}}` / `{"Err":{code,message,retryable}}`.

## Server: one listener, two transports (CPE-819 slice 2)

`ServerRuntime::handle()` peeks the first bytes of a connection **without consuming them**:

- starts with `GET ` → an HTTP upgrade → reply `101 Switching Protocols` with
  `Sec-WebSocket-Accept = base64(sha1(key + RFC-GUID))`, then speak envelopes as WS text frames.
- otherwise → the historical raw length-prefixed wire.

Both feed one transport-agnostic session loop behind an `EnvelopeIo` seam (`WireIo` / `WsIo`), so the
Hello→Welcome handshake, the request/response dispatch, and streaming (`StreamItem`/`StreamEnd`) are
written once and work over either framing. The security chain (CPE-816) enforces per request on both
paths identically.

`WsIo` handles control frames: a **Ping is answered with a Pong** echoing its payload (RFC 6455 §5.5.2)
so browser/proxy keepalives don't drop the session; a Close frame (or EOF) ends it. The codec
(`cpe-net::ws`) refuses a declared payload over `MAX_WS_PAYLOAD` (16 MiB) before allocating, so a hostile
length can't drive an unbounded allocation.

## Frontend: `RemoteTransport` (CPE-819 slice 3)

- Opens the socket and completes the **Hello→Welcome** handshake once per connection.
- Each `invoke(cmd, args)` → a `request` text frame with a monotonic `id`; the promise resolves when the
  matching `response` frame arrives (correlated by `id`, so concurrent calls are not FIFO-coupled).
- An `Err` response rejects with a typed `RemoteCallError` carrying the contract `code` + `retryable`.
- A socket close rejects everything in flight and resets, so the next call reconnects.
- The socket factory is injectable, so the handshake + correlation logic is unit-tested against a mock
  server (no real socket).

## Status

Landed & CI-green (headless): the codec, the WS-accepting server, `RemoteTransport`'s unary
request/response path, security enforcing remotely, the local-fast guard, and version conformance.

**Streaming — built (unit-verified).** The local `*_stream` commands (list_dir, name search, content
search) stream batches through a Tauri `ipc::Channel` **and return a final stats value**. Two gaps that
blocked routing these over the remote transport are both closed:

1. **Done (CPE-895).** The wire no longer ends a stream with a payload-less frame — `StreamEnd { result }`
   carries the producer's terminal value, a **struct variant** (an internally-tagged enum cannot serialize
   a newtype wrapping a scalar/array). The `StreamHandler` returns that value; the client's `call_stream`
   yields `StreamOutcome { items, result }`.
2. **Done (CPE-896).** A seam-owned channel: `createChannel()` in `invoke.ts` returns a real Tauri
   `Channel` under `localTransport` (native, unchanged) but a `RemoteChannel` under `RemoteTransport`.
   `RemoteTransport` detects a `RemoteChannel` arg (`instanceof`), strips it from the wire params, routes
   `stream_item` frames to its `onmessage` (un-flattening the item from the tagged frame, wrapping it as a
   one-element batch), and resolves the `invoke` with `StreamEnd.result`. All 7 channel call sites use
   `createChannel`; none import Tauri's `Channel`.

The one **remaining** item is user-gated: an attended **GUI-verify** browsing/searching against a live
loopback server (and one real remote host).

> **StreamItem note:** items ride as an internally-tagged *newtype* `StreamItem(Value)`, so the item's
> fields are flattened next to `"type":"stream_item"`. That's safe for today's producers (their structs
> are `type`-field-free objects). A future producer streaming a scalar/array — or an object with its own
> `type` field — would need `StreamItem` promoted to a struct variant (`{ item }`), exactly as CPE-895
> did for `StreamEnd`.

Also user-gated: browse/preview against one **real remote** host (not loopback) and an attended
end-to-end GUI verification.
