---
id: CPE-1016
title: Fix capacity-overflow panic loading a crafted/corrupt vector index (.cpevec/.semidx)
type: bug
component: Backend
priority: high
tags: ready
status: Doing
created: 2026-07-24
epic: CPE-976
estimate: 45m
---

## Summary
Found by the 2026-07-24 workshift bug-audit (third wave, binary parsers). `VectorIndex::from_bytes`
(`crates/server/src/vector_index.rs:194-200`) reads `dim` and `count` as raw untrusted `u32` header fields,
then does `Vec::with_capacity(count)` (line 199) and `Vec::with_capacity(count * dim)` (line 200) **with no
validation** against the file's remaining size.

**Malformed input → guaranteed panic:** a 20-byte file — `MAGIC`(8) + `FORMAT_VERSION`(4) + `dim=0xFFFFFFFF`(4)
+ `count=0xFFFFFFFF`(4). `count * dim` ≈ 1.84e19 (fits `usize`, no wrap), but `Vec::with_capacity` then
computes `capacity * size_of::<f32>()` ≈ 7.4e19 > `isize::MAX`, so `RawVec` calls `capacity_overflow()` →
`panic!("capacity overflow")` — **deterministically, before any allocation, on any machine regardless of RAM.**
No file body beyond the 20-byte header is needed. This contradicts the function's own doc ("short/garbled body
→ `Io` — never a panic"): the `Reader` reads are bounds-checked, but the pre-loop `with_capacity` hints aren't.

**Reachability:** `vector_index` + `semantic_index` compile **unconditionally** (not behind the `index`
feature) — `SemanticIndex::from_bytes`/`load` (`semantic_index.rs`) delegates straight in. Latent today (no
production caller until the CPE-982/983 embedder lands), but it will fire the instant any caller/fuzzer/test
feeds a corrupted `.cpevec`/`.semidx`.

## Fix
Do not pre-allocate from an untrusted count. Bound the `with_capacity` hint by the buffer's actual remaining
bytes, or drop the hint and grow incrementally (each per-item read is already bounds-checked). `Reader` has
`buf` + `pos`; add a small `fn remaining(&self) -> usize { self.buf.len() - self.pos }` if needed. Example:
```rust
let cap = count.min(r.remaining() / 4);                 // each id needs ≥4 bytes (its u32 len)
let mut ids = Vec::with_capacity(cap);
let mut vectors = Vec::with_capacity((count.saturating_mul(dim)).min(r.remaining() / 4)); // each f32 = 4 bytes
```
(or simply `Vec::new()` for both — correctness over the capacity hint). Keep `dim==0` guard as-is.

## Acceptance Criteria
- [ ] `VectorIndex::from_bytes` on the 20-byte crafted header (huge `dim`/`count`) returns `Err(..Io..)`,
      **never panics**. Add a regression test with that exact input; also a test with a large `count` but
      truncated body → clean `Err`.
- [ ] Valid round-trip (save→load) still works unchanged; existing vector_index tests pass.
- [ ] Also confirm `SemanticIndex::from_bytes` (which delegates here) no longer panics on the same input
      (add/extend a test if `semantic_index` has its own `from_bytes` path).
- [ ] `cargo test -p cpe-server vector_index` (and `semantic_index`) green; clippy clean both feature modes;
      no new deps.

## Notes
Sibling bug with the same root cause in `index.rs` is CPE-1017 — fix both in one PR (same pattern).
