---
id: CPE-991
title: OCR seam + FakeOcr + content-addressed text cache
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-980
---

# CPE-991 — OCR seam + FakeOcr + content-addressed text cache

## Summary

The pure, dependency-free foundation of the OCR & scanned-document pipeline (epic CPE-980): an
object-safe `OcrEngine` seam, a deterministic `FakeOcr`, and a content-addressed `OcrCache` so the
whole pipeline is testable with **zero OCR engine weight** — no Tesseract, no ML stack, no system
libs. Mirrors how `provider.rs` ships a `FakeProvider` and `embedder.rs` ships a `FakeEmbedder`.

A real OCR engine is deliberately **out of scope** here; it will implement the same `OcrEngine`
trait behind a feature gate later.

## Design

New module `crates/server/src/ocr.rs` (pure std + the existing `sha2` dependency):

- `pub trait OcrEngine { fn recognize(&self, image_bytes: &[u8]) -> String; }` — object-safe, so a
  caller can hold `Box<dyn OcrEngine>` and swap a real engine in later without touching call sites.
- `pub struct FakeOcr` (`FakeOcr::new()`): interprets `image_bytes` as UTF-8 **lossily** and returns
  it. Tests feed text-as-bytes and assert the recognised text round-trips; non-UTF-8 becomes U+FFFD
  rather than panicking.
- `pub struct OcrCache` (`OcrCache::new()`): `recognize_cached(&mut self, engine: &dyn OcrEngine,
  image_bytes: &[u8]) -> String` hashes the bytes, returns cached text on a hit (engine NOT invoked),
  else calls `engine.recognize`, stores under the hash, and returns it — so a given image is OCR'd
  once. Exposes `len()`, `is_empty()`, `hits()`, `misses()` so a test can prove the engine ran only on
  a miss.

`pub mod ocr;` added to `crates/server/src/lib.rs` (in the AI-search cluster, after `search_fusion`),
with a short doc comment matching the neighbouring modules.

## Acceptance Criteria

- [x] `OcrEngine` trait is object-safe (`Box<dyn OcrEngine>` holds + calls it) — proven by a test.
- [x] `FakeOcr::new()` returns the text for text-as-bytes; empty bytes → empty string.
- [x] `FakeOcr` handles non-UTF-8 bytes lossily without panicking.
- [x] `OcrCache` returns the same text on a repeat and does NOT re-invoke the engine on a hit
  (proven via a counting test engine incrementing a `Cell`).
- [x] Different content → different cache entries; empty bytes handled.
- [x] Zero new dependencies (uses the existing `sha2`); pure + std.
- [x] `pub mod ocr;` declared in `lib.rs` with a doc comment.
- [x] `cargo test ocr::` passes (7 tests).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `ocr.rs` end-to-end.
  - **Content-hash choice: SHA-256 hex** via the `sha2` crate (already a cpe-server dependency, used
    by `fsutil::sha256_file`), formatted lowercase hex inline — no new dependency, and consistent with
    the crate's existing content-hashing convention. Chose SHA-256 over a small inline stable hash for
    collision resistance on real image payloads (an inline FNV-style hash would risk two distinct images
    colliding to one cache entry and returning the wrong text). A pinned test asserts SHA-256("") so an
    accidental hasher swap fails CI.
  - **Assumptions logged:**
    - Cache is in-memory (`HashMap<hex, String>`); persistence is a later ticket's concern — this
      ticket is the pure seam + cache model only, matching the "bytes behind each hash are the caller's
      to persist" pattern used elsewhere in the crate.
    - `FakeOcr` uses lossy UTF-8 so any byte input is total (never panics), simulating "this image's
      text is X" for text-as-bytes fixtures.
    - Added `is_empty()` alongside `len()` to satisfy clippy's `len_without_is_empty` lint.
    - Module placed in the AI-search cluster in `lib.rs` (after `search_fusion`), as the OCR epic is
      part of the AI/document-understanding family.
  - Verified: `cargo test ocr::` → 7 passed; both clippy invocations (default + `--features index`)
    clean with `-D warnings`.
- 2026-07-24 — Status → Done; ACs checked; moved to `Tickets/Done/2026/Q3/July/Week-30/`.
