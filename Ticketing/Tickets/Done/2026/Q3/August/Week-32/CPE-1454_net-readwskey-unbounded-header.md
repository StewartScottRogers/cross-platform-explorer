---
id: CPE-1454
title: "net server read_ws_key does an unbounded read_line on the WS-upgrade headers → server-side DoS"
type: Bug
status: Done
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

## Work Log (2026-08-08)

**Fix.** `crates/net/src/server.rs`: `read_ws_key` now delegates to a new private
`read_ws_key_capped(reader, max_bytes)`, mirroring CPE-1416's `wire::read_envelope_capped` pattern —
each loop iteration hands the reader a fresh `Read::take(remaining)` bounded to whatever's left of the
budget (not just a single `.take()` around the whole loop, since the original bug was a *multi-line*
`read_line` loop with no per-call bound at all). A running `total` byte counter is checked at the top of
each iteration, and after each `read_line` the same "limit hit with no `\n`" check `wire.rs` uses decides
whether the read stopped because of a real line/EOF or because the budget ran out — either way, once the
aggregate budget is exhausted before the blank line ending the headers is seen, the function returns
`Err(io::Error::new(InvalidData, ...))` instead of continuing to allocate.

New const: `MAX_WS_HEADER_BYTES: u64 = 64 KiB` — a real WS upgrade request is a handful of short header
lines (well under 1 KiB in practice); 64 KiB is generous headroom for any legitimate client while still
bounding a peer that never sends the terminating blank line.

**Tests** (`crates/net/src/server.rs`, new `#[cfg(test)] mod tests`):
- `reads_a_normal_upgrade_request_and_finds_the_key` / `missing_key_header_yields_an_empty_key_not_an_error`
  — behavior parity checks (normal parsing untouched by the cap).
- `unbounded_peer_with_no_header_terminator_errors_promptly_at_the_cap` — `std::io::repeat(b'x')` (a
  genuinely infinite source) against the real `MAX_WS_HEADER_BYTES` cap errors promptly with
  `InvalidData`; the test's own termination (it doesn't hang/OOM) is as much the assertion as the `Err`.
- `reader_with_no_terminator_errors_right_at_a_small_cap` — same shape at a tiny 64-byte cap, pinning the
  exact boundary.
- `many_short_lines_that_never_terminate_still_hit_the_aggregate_cap` — the case the original bug's
  single-line framing missed: ~148 KiB of complete, well-formed header lines (each properly `\r\n`
  terminated) but never followed by the blank line ending the headers, against a 1 KiB cap. Proves the
  budget is aggregate across the whole loop, not reset every line.
- `headers_exactly_at_the_cap_still_round_trip` / `headers_one_byte_over_the_cap_errors` — exact boundary
  pins, matching `wire.rs`'s `envelope_exactly_at_the_cap_still_round_trips` /
  `envelope_one_byte_over_the_cap_errors` style.

**Verification run:**
- `cargo build` (crates/net) — clean.
- `cargo clippy --all-targets -- -D warnings` (crates/net; the crate declares no optional `[features]`
  of its own, so this is the only build config) — clean.
- `cargo test` (crates/net) — 37 passed, 0 failed, finished in 0.17s (includes the 6 new tests above and
  CPE-1453's 3 new tests).

**PR:** #716 (branch `cpe-1453-1454-net-dos-caps`), batched with CPE-1453 (same crate, `client.rs`) —
one PR covering both tickets per the epic's DoS-hardening sweep.
