---
id: CPE-895
title: StreamEnd carries the producer's terminal value (unblocks remote streaming stats)
type: feature
component: Multiple
priority: medium
tags: ready
epic: CPE-810
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
The wire ended a stream with a **payload-less `StreamEnd`**, but the local `*_stream` commands
(list_dir, name/content search) **return a final stats value** (total count, `truncated`, …) alongside
their `ipc::Channel`. So remote streaming couldn't deliver those stats — the documented blocker for the
frontend streaming slice (see `docs/design/REMOTE-TRANSPORT.md`). This slice fixes the wire + Rust layers.

Changes:
- **`cpe-contract`:** `Message::StreamEnd` → `StreamEnd { result: serde_json::Value }`. A **struct**
  variant, not a newtype — an internally-tagged enum (`tag = "type"`) cannot serialize a newtype variant
  wrapping a scalar/array, but the struct's field holds any JSON. `#[serde(default)]` on `result` keeps
  backward-compatible decode of a bare `stream_end`.
- **`cpe-net` server:** `StreamHandler` now returns `Result<Value, ContractError>` (its terminal
  value); the emit path sends `StreamEnd { result }`. The 3 builtin handlers return their stats
  (`list_dir_stream` → `{total}`, `name_search_stream` → `{dirs_scanned, truncated}`, `content_search_stream`
  → `{files_scanned, truncated}`).
- **`cpe-net` client:** `call_stream` now returns `StreamOutcome { items, result }` instead of just the
  items.

## Decisions
- **Struct variant over newtype** for `StreamEnd` — the internally-tagged serde limitation makes a
  newtype-wrapping-a-scalar fail at runtime; a dedicated round-trip test (`every_message_variant_round_trips`
  now sends a bare number as the terminal value) guards this.
- **Terminal value shape is per-producer JSON**, built explicitly in each handler (the walker stats
  structs aren't all `Serialize`), keeping the handlers independent of those internal types.

## Acceptance Criteria
- [x] `StreamEnd` carries a JSON terminal value; scalar/array/object all round-trip.
- [x] Builtin stream handlers return their stats; `call_stream` surfaces them as `StreamOutcome.result`.
- [x] `list_dir_stream`'s `total` on `StreamEnd` matches the items streamed (test).
- [x] contract 10/10 + net 23/23 + clippy `-D warnings` clean; no other workspace consumer of `StreamEnd`.

## Work Log
- 2026-07-22 (nightshift) — Wire + net layers landed. **Remaining for end-to-end remote streaming (next
  slice):** the frontend seam-owned channel abstraction that routes `stream_item` frames to a component's
  `onmessage` and resolves the `invoke` with `StreamEnd.result` — so the ~5 `*_stream` call sites work
  over `RemoteTransport` without Tauri's `Channel`. See `docs/design/REMOTE-TRANSPORT.md`.
