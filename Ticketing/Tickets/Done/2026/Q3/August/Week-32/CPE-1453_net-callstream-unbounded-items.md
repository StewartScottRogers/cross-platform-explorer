---
id: CPE-1453
title: "net Client::call_stream accumulates unbounded StreamItems from a hostile server → client OOM"
type: Bug
status: Done
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-810
created: 2026-08-08
---
## Vector (found in the net/webdav deep audit, 2026-08-08)
`crates/net/src/client.rs:~176-181`: `items: Vec` grows one push per `StreamItem` with NO count or aggregate-byte
cap. `read_envelope` caps each FRAME at 16 MiB (CPE-1416) but NOT the number of frames. A hostile server answers a
stream request with an endless `StreamItem` sequence (never sending `StreamEnd`) → client OOM.

## Reachability
LOW — the `cpe-net` client isn't wired into the shipped app today (same latent posture as the remote providers).
Real code bug; fix while hardening the network stack.

## Fix direction
Cap total items AND/OR aggregate bytes in `call_stream`; error out past the cap (surface a truncation/limit error).
Pick caps consistent with the 16 MiB per-frame cap and realistic listing sizes.

## Effort / blast radius
S / client.rs. Epic CPE-810. Parallel-safe with the transfer.rs and sidecar work (different crate). Batch with
CPE-1454 (same net crate, different file server.rs).

## Work Log (2026-08-08)

**Fix.** `crates/net/src/client.rs`: `Client::call_stream` now delegates to a new private
`call_stream_capped(method, params, max_items, max_bytes)` that checks BOTH caps before accepting each
`StreamItem` — if `items.len() >= max_items` OR the running `total_bytes` (aggregate of each item's
`serde_json::to_vec` length) would exceed `max_bytes`, the loop returns an `Err(ContractError)`
(`ErrorCode::Internal`, `retryable: false`) instead of pushing the item, so `items` can never grow past
either budget. The public `call_stream` calls it with two new module consts:

- `MAX_STREAM_ITEMS: usize = 1_000_000` — an item-count ceiling. The builtin streaming producers
  (`list_dir_stream`/`name_search_stream`/`content_search_stream`) have no per-listing cap of their own
  today, so a legitimate very-large directory could stream hundreds of thousands of entries; 1,000,000 is
  generous headroom above that (for comparison, `cpe_server::archive_safety_scan::MAX_ENTRIES` = 200,000
  is this codebase's existing working definition of "a lot of real entries"), while still bounding a
  peer that streams forever.
- `MAX_STREAM_BYTES: u64 = 256 MiB` — an aggregate serialized-bytes ceiling, independent of the item
  count (catches a peer sending fewer but larger items). 256 MiB is 16x CPE-1416's single-frame cap
  (`wire::MAX_ENVELOPE_BYTES` = 16 MiB) and matches `cpe_server::batch_transform::MAX_ALLOC_BYTES`
  (also 256 MiB), this codebase's existing precedent for "generous but bounded" memory budgets.

Whichever cap is hit first ends the stream with a clear error naming both the accumulated and the
allowed counts/bytes — never a silent drop, never a panic.

**Tests** (`crates/net/src/lib.rs`, using the existing `count_stream` test harness and the new
`pub(crate) call_stream_capped` so the test can use tiny caps instead of actually streaming a million
real items):
- `a_stream_exceeding_the_item_cap_errors_instead_of_accumulating_unbounded` — `count_stream` asked for
  1,000,000 items against a 5-item cap errors with `ErrorCode::Internal` and a message containing
  "exceeded client-side limits", never accumulating past 5.
- `a_stream_exceeding_the_byte_cap_errors_instead_of_accumulating_unbounded` — same shape with
  `max_items = usize::MAX` and a 32-byte cap, proving the byte budget is enforced independently of the
  item-count cap.
- `a_stream_under_the_caps_completes_normally` — a 3-item stream under both caps still completes and
  returns the producer's terminal `StreamEnd` value, proving the caps never bite a legitimate stream.

**Verification run:**
- `cargo build` (crates/net) — clean.
- `cargo clippy --all-targets -- -D warnings` (crates/net; the crate declares no optional `[features]`
  of its own, so this is the only build config) — clean.
- `cargo test` (crates/net) — 37 passed, 0 failed, finished in 0.17s.

**PR:** #716 (branch `cpe-1453-1454-net-dos-caps`), batched with CPE-1454 (same crate, `server.rs`).
