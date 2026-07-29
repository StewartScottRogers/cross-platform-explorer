---
id: CPE-896
title: Frontend streaming over RemoteTransport — seam-owned channel (createChannel)
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
Final slice of the remote transport (CPE-819). Streaming call sites imported Tauri's `Channel` directly
and did `new Channel<T>()`, which only works over in-process IPC (a real browser can't construct one).
This adds a **seam-owned channel** so the ~7 `*_stream` call sites are transport-agnostic:

- **`invoke.ts`:** a `StreamChannel<T>` interface + `createChannel<T>()` on the `Transport` seam. Under
  `localTransport` it returns a real Tauri `ipc::Channel` (native streaming — the local path is
  byte-for-byte unchanged); under `RemoteTransport` it returns a `RemoteChannel`.
- **`RemoteTransport`:** `createChannel()` + streaming in `invoke()` — a `RemoteChannel` arg is detected
  (`instanceof`), **stripped from the wire params** (the server streams by protocol, not a serialized
  handle), and registered against the request id. Incoming `stream_item` frames are routed to the
  channel's `onmessage` (the item's fields are un-flattened from the internally-tagged frame and wrapped
  as a one-element batch to match the Tauri `Channel` shape); the call resolves with `StreamEnd.result`
  (the terminal stats, from CPE-895). A `Response` instead of `StreamEnd` is a denial → reject.
- **7 call sites migrated** off `new Channel()` to `createChannel()`: `ExplorerPane` (list_dir),
  `FileNameSearchDialog` (name search), `ContentSearchDialog`, `DuplicatesDialog`, `DiskSpaceView`,
  `App.svelte` + `BackupDashboard` (op-progress). None import `Channel` from `@tauri-apps/api/core` now.

## Decisions
- **No `StreamItem` contract change needed.** The streamed structs (DirEntry, ContentMatch, NameMatch)
  have no `type` field and serialize as objects, so the internally-tagged newtype `StreamItem(Value)`
  flattening is safe here — the shim un-flattens by stripping `type`. (If a future producer streams a
  scalar/array or a `type`-bearing object, `StreamItem` would need the struct-variant treatment CPE-895
  gave `StreamEnd`.)
- **Migrate all 7 channel sites**, not just the 5 streaming producers, so the seam owns channel creation
  uniformly and no component imports Tauri's `Channel`.

## Acceptance Criteria
- [x] `createChannel()` on the seam; local → Tauri `Channel`, remote → `RemoteChannel`.
- [x] `RemoteTransport` routes `stream_item` → `onmessage` (one-element batches) and resolves with
      `StreamEnd.result`; channel stripped from wire params; mid-stream denial rejects.
- [x] All 7 call sites use `createChannel`; local path unchanged (component tests green).
- [x] `npm run check` 0 errors; `vitest` 914 pass (incl. 3 new RemoteTransport streaming tests); guard green.

## Work Log
- 2026-07-22 (nightshift) — Completes CPE-819 **AC #2 end-to-end** (streaming works over the remote
  transport, unit-verified via a mock server). Only **AC #4** (attended GUI-verify against a live loopback
  server) remains — user-gated. See `docs/design/REMOTE-TRANSPORT.md`.
