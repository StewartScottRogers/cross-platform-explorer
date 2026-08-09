---
id: CPE-1017
title: Fix unbounded allocation (OOM/abort) loading a corrupt .cpeidx index cache
type: bug
component: Backend
priority: medium
tags: ready
status: Doing
created: 2026-07-24
epic: CPE-703
estimate: 30m
---

## Summary
Found by the 2026-07-24 sprint bug-audit (third wave). `Index::from_bytes`
(`crates/server/src/index.rs:550-560`) reads `name_count` and `entry_count` as raw untrusted `u32` fields off
disk and immediately `Vec::with_capacity(name_count as usize)` / `Vec::with_capacity(entry_count as usize)`
(lines 551, 560) with **no** sanity check against the file's remaining size, before reading a single name/entry.

**Malformed input → OOM/abort:** a 25-byte `.cpeidx` — `MAGIC`(8) + version(4) + volume_id(8) + trunc-flag(1)
+ `name_count=0xFFFFFFFF`(4), nothing following. Requests `Vec::<String>::with_capacity(4_294_967_295)` ≈ 103 GB
up front (`* size_of::<String>()` = 24 bytes stays under `isize::MAX`, so no clean "capacity overflow" panic —
instead a real allocation that aborts via `handle_alloc_error` on Windows/typical systems, or severe memory
pressure under Linux overcommit), well before the truncation is detected.

**Reachability:** gated behind the off-by-default `index` cargo feature (`crates/server/Cargo.toml`).
`Index::load` reads a per-volume `.cpeidx` cache CPE writes itself (atomic temp+rename), so the attack surface
is disk corruption or a process with write access to the app's cache dir — lower real-world exposure than the
unconditional CPE-1016, but the same code smell and fix.

## Fix
Bound the `with_capacity` hint by the reader's remaining bytes (each name needs ≥ a few bytes; each entry a
known minimum), or drop the hint and grow incrementally via `push` in the existing bounds-checked loop, e.g.:
```rust
let names = Vec::with_capacity((name_count as usize).min(remaining_bytes / 4));
```
Use whatever remaining-length accessor the module's reader exposes (mirror the CPE-1016 approach).

## Acceptance Criteria
- [ ] `Index::from_bytes` on a crafted header with a huge `name_count`/`entry_count` and no body returns a
      clean `Err` (no giant allocation, no abort). Add a regression test with that input (behind the `index`
      feature, matching existing index tests).
- [ ] Valid save→load round-trip unchanged; existing `index` tests pass.
- [ ] `cargo test -p cpe-server --features index index` green; clippy clean both feature modes; no new deps.

## Notes
Sibling of CPE-1016 (same root cause) — fix both in one PR.
