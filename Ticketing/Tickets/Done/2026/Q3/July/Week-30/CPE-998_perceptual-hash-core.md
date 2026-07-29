---
id: CPE-998
title: Perceptual-hash + similarity-clustering core
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-997
---

# CPE-998 — Perceptual-hash + similarity-clustering core

## Summary

The pure engine behind epic CPE-997 ("Near-duplicate & similar-image detection"): a dHash (difference
hash) perceptual hash over decoded image bytes, a Hamming-distance similarity metric, and a
transitive-closure clustering pass that groups near-duplicate images by id. [`crates/server/src/duplicates.rs`]
already finds **byte-identical** files; this module finds the complement — images that look alike even
though their bytes differ (recompression, resize, format change, a crop, a colour tweak).

New module `crates/server/src/perceptual.rs`. Pure over already-decoded bytes: no filesystem I/O, no new
dependencies (reuses the `image` crate already in `crates/server/Cargo.toml`).

## Design

Three free functions:

- `pub fn phash(image_bytes: &[u8]) -> Option<u64>` — decode via `image::ImageReader`, convert to
  grayscale, resize to a fixed 9×8 grid with `image::imageops::FilterType::Triangle` (deterministic,
  cheap, doesn't ring at tiny target sizes), then for each of the 8 rows compare each of the 9 pixels to
  its right-hand neighbour (8 comparisons/row × 8 rows = 64 bits; bit = 1 if left pixel is brighter than
  right). Packed row-major, MSB-first, into a `u64`. Returns `None` (never panics) if the bytes aren't a
  decodable image — same "skip what can't be read" spirit as the rest of the crate.
- `pub fn hamming(a: u64, b: u64) -> u32` — `(a ^ b).count_ones()`.
- `pub fn cluster(items: &[(String, u64)], max_distance: u32) -> Vec<Vec<String>>` — union-find over
  item indices, unioning any pair within `max_distance` Hamming bits (transitive/single-link: if A~B and
  B~C, A/B/C land in one group even if A and C alone exceed `max_distance`). Only groups with 2+ members
  are returned (singletons dropped); each group's ids sorted; groups sorted by their own first id for a
  fully deterministic result independent of input order.

`pub mod perceptual;` added to `crates/server/src/lib.rs` after `op_plan`, with a doc comment describing
it as the complement of `duplicates`.

## Acceptance Criteria

- [x] `phash` decodes+grayscales+resizes to 9×8 and packs a 64-bit dHash; returns `None` for
  non-image/corrupt bytes without panicking.
- [x] `phash` is deterministic: identical bytes hash identically; the same image re-encoded (even at a
  different pixel size) hashes to the same or a very close value.
- [x] `hamming` is `(a^b).count_ones()`: 0 for equal hashes, 64 for a fully-inverted pair, symmetric.
- [x] `cluster` groups ids within `max_distance` Hamming bits transitively (single-link), omits
  singletons, returns sorted ids within a deterministically-ordered list of groups.
- [x] Zero new dependencies; pure over bytes, no filesystem I/O.
- [x] `pub mod perceptual;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib perceptual` passes (11 tests).
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `perceptual.rs` end-to-end: `phash` (dHash), `hamming`, `cluster` (union-find,
  single-link, 2+ member groups only).
  - **Hash choice:** dHash over aHash/pHash-DCT — robust to brightness/contrast shifts (compares
    *adjacent* pixels, not each pixel to a global average) while staying as cheap as aHash (one resize +
    one grayscale pass, no DCT). 9×8 grid → 64 bits fits a `u64` exactly, matching the ticket's spec.
  - **Known dHash limitation (by design, not a bug):** the algorithm as specified only compares
    *horizontal* neighbours, so a purely column-invariant image (uniform per row, varying only by row —
    e.g. a vertical-only gradient or a horizontal edge) always hashes to all-zero bits regardless of its
    content. This is inherent to dHash's row-wise horizontal-diff construction, not something this
    implementation should special-case; documented in the module doc-comment and exercised directly by a
    test (solid black vs. solid white both hash to the same all-zero value).
  - **Test fixture assumption:** a smooth monotonic gradient (brightness strictly increasing
    left→right) always produces `left < right` at every comparison, so it collapses to a single repeated
    all-zero-bit pattern — a poor "clearly different" fixture despite looking maximally different to a
    human eye. Used inverted-parity column-striped images (9 solid vertical bands, alternating
    black/white, one image starting black and the other starting white) instead, which reliably produces
    a fully-inverted 64-bit hash pair (distance 64) — a robust, non-flaky "far apart" fixture. Kept the
    smooth gradient (resized to two different pixel sizes) as the "near identical" fixture, since
    resize-then-rehash of the same monotonic gradient reliably stays at or very near distance 0.
  - **Default `max_distance` reasoning (documented here since `cluster` takes it as a caller-supplied
    parameter rather than hard-coding a default):** for a 64-bit dHash, published guidance (and this
    module's own near/far fixtures) puts truly-near-duplicate pairs under ~10 bits and unrelated images
    well above ~20 bits, with a wide gap between. A **threshold of 10** is the recommended default for a
    future caller (e.g. the folder-scan command this core will back) — permissive enough to catch
    recompression/resize/minor-edit near-duplicates, tight enough to avoid false-positive clustering of
    merely similar-looking-but-distinct images. Left as a caller parameter (not hard-coded) so a future
    UI can expose it as a sensitivity slider.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib perceptual` → 11/11 passed; `cargo clippy --all-targets -- -D warnings` clean;
    `cargo clippy --all-targets --features index -- -D warnings` clean. One clippy fix needed along the
    way: `cluster`'s bucketing loop tripped `needless_range_loop` — switched to
    `items.iter().enumerate()`.
  - Scope note: epic CPE-997 doesn't yet have a filed `Tickets/Epics/CPE-997*.md` brief in this repo at
    the time of this ticket; per the work order this ticket only touches `perceptual.rs` + the one
    `lib.rs` module line + this ticket file, so the epic file wasn't created here. Frontmatter still
    references `epic: CPE-997` as instructed.
  - Status → Done; ACs checked; moving to `Tickets/Done/2026/Q3/July/Week-30/`.
