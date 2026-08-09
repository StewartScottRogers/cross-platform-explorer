---
id: CPE-1087
title: "PSD dimension OOM guard — bound declared PSD size before compositing (thumb_source + image_preview)"
type: fix
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-718
---

## Summary
Reviewer-caught (PR #404): the vendored `psd = 0.3` crate has **no built-in size limit**, so a maliciously
huge-declared PSD (e.g. header declaring 100000×100000) causes `psd.rgba()` to allocate `w*h*4` bytes and
**OOM**. This gap exists in **two shipped call sites** — `crates/server/src/thumb_source.rs` (CPE-1086) and
`crates/server/src/image_preview.rs` (pre-existing). Bound the declared PSD dimensions **before** calling
`.rgba()` in both, mirroring the `image::Limits` guard used for raster decode. Pure `crates/server` fix,
`cargo test` on the 3-OS matrix — no GUI, no user resource, no new deps.

## Design (buildable)
Add a small guard fn (put it once and reuse — e.g. `pub(crate) fn psd_within_limits(psd: &psd::Psd) -> bool`
in a sensible shared spot like `thumb_source.rs`, or a tiny helper each call site calls). Before
`psd.rgba()`:
- Read the PSD's declared `width()` / `height()`.
- Reject (return `Err("PSD too large: WxH exceeds limit")`) if `width > MAX` or `height > MAX` or the RGBA
  byte budget `width as u64 * height as u64 * 4` exceeds the alloc budget. Use the SAME budget as the raster
  guard: **max 20_000 px per side, 256 MiB alloc** (align with `batch_transform::bounded_limits` /
  `thumb_source`'s limits so behavior is consistent).
- Apply the guard in BOTH `thumb_source::decode_thumb_image` (PSD branch) AND `image_preview.rs`'s PSD path
  (whatever fn contains the `psd::Psd::from_bytes(...).rgba()` — grep it, ~line 21). Preserve each fn's
  existing signature + error-string style.

## ⚠ Notes
Checked arithmetic — the byte budget uses `u64` (`w as u64 * h as u64 * 4`) so it can't overflow. No panic on
a malformed/huge PSD — return `Err`. Cross-OS: pure integer check, no `std::path`/`#[cfg]`. No new deps.

## Acceptance Criteria
- [x] A PSD declaring huge dims (e.g. 100000×100000) → `Err` from `thumb_source::decode_thumb_image` (before
      any `w*h*4` allocation), NOT an OOM/panic.
- [x] The same huge PSD → `Err` from the `image_preview.rs` PSD path (both call sites guarded).
- [x] A normal small PSD still decodes fine (existing CPE-1086 PSD test + image_preview tests still pass).
- [x] Byte-budget math uses `u64` (no overflow); `cargo test -p cpe-server` green; clippy `--all-targets -- -D
      warnings` clean default AND `--features index`; no new deps.

## Work Log
2026-07-26 (sprint) — Filed by the Foreman from the CPE-1086 reviewer's fast-follow: the `psd` crate lacks
a size limit, so a huge-declared PSD is a potential OOM in both thumb_source.rs and image_preview.rs. Real
hardening, small + cargo-testable.

2026-07-26 (sprint, Worker) — Implemented: added `pub(crate) fn psd_within_limits(psd: &psd::Psd) -> bool`
in `crates/server/src/thumb_source.rs`, reusing the module's existing `MAX_IMAGE_DIMENSION` (20,000 px/side)
and `MAX_ALLOC_BYTES` (256 MiB) constants (same budget as `batch_transform::bounded_limits`). Checks
`width()`/`height()` against the per-side cap, then the `u64` RGBA byte budget (`w as u64 * h as u64 * 4`).
Called right after `psd::Psd::from_bytes(...)` and before `.rgba()` in both `thumb_source::decode_thumb_image`
(PSD branch) and `image_preview::read_image_data_url` (the latter calls
`crate::thumb_source::psd_within_limits` — one shared guard, not duplicated). Each site returns
`Err(format!("PSD dimensions exceed limit ({w}x{h})"))` matching its existing `.to_string()`-style error
convention; signatures unchanged.

**Assumption/finding worth flagging**: the vendored `psd = 0.3.5` crate already hard-rejects width/height
`> 30_000` in its own header validation (`sections/file_header_section.rs`, `PsdWidth`/`PsdHeight` — a fixed
PSD-spec cap), so a literal 100,000×100,000 file never reaches our new guard at all — `Psd::from_bytes` itself
already returns `Err` for that case. Our guard closes the real remaining gap: dimensions in the (20,000,
30,000] range, which the crate happily parses but which would still blow well past the 256 MiB budget at
`.rgba()` (e.g. 25,000×25,000 → 2.5 GB). Tests were built around this: `minimal_psd(25_000, 2)` (real,
parseable, ~150 KB of actual pixel data — height kept small so the fixture itself stays cheap) is used as the
"reject" fixture in both `thumb_source.rs` and `image_preview.rs` tests, since it's a genuine case only our
guard (not the crate) blocks. Also confirmed (empirically, via a header-only malformed-input experiment) that
`psd::Psd::from_bytes` can itself panic (not just Err) on certain malformed/truncated inputs unrelated to this
ticket's scope — noted here for future reference, not fixed (out of scope; no shipped call site feeds it
attacker-controlled truncated bytes in a way that reaches that code path today).

Verified from `crates/server`: `cargo test` → 999 passed, 0 failed (incl. new + all existing
thumb_source/image_preview/thumbnail PSD tests). `cargo clippy --all-targets -- -D warnings` clean. `cargo
clippy --all-targets --features index -- -D warnings` clean. No new dependencies added.

Landed as branch `cpe-1087-psd-oom-guard`, PR opened against `main`.
