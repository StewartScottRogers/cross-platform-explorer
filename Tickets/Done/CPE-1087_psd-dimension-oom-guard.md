---
id: CPE-1087
title: "PSD dimension OOM guard — bound declared PSD size before compositing (thumb_source + image_preview)"
type: fix
component: Backend
priority: medium
status: Doing
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
- [ ] A PSD declaring huge dims (e.g. 100000×100000) → `Err` from `thumb_source::decode_thumb_image` (before
      any `w*h*4` allocation), NOT an OOM/panic.
- [ ] The same huge PSD → `Err` from the `image_preview.rs` PSD path (both call sites guarded).
- [ ] A normal small PSD still decodes fine (existing CPE-1086 PSD test + image_preview tests still pass).
- [ ] Byte-budget math uses `u64` (no overflow); `cargo test -p cpe-server` green; clippy `--all-targets -- -D
      warnings` clean default AND `--features index`; no new deps.

## Work Log
2026-07-26 (workshift) — Filed by the Foreman from the CPE-1086 reviewer's fast-follow: the `psd` crate lacks
a size limit, so a huge-declared PSD is a potential OOM in both thumb_source.rs and image_preview.rs. Real
hardening, small + cargo-testable.
