---
id: CPE-1447
title: "thumbnails_stream skips the 128 MiB size gate the single-thumbnail command applies → OOM on a huge image file"
type: Bug
status: Done
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-718
created: 2026-08-07
---
## Vector (found in the CPE-1437 resource-exhaustion sweep, 2026-08-07)
`src-tauri/src/lib.rs:~1203` `thumbnail` calls `ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)`
(`~:1205`), but the streaming batch `thumbnails_stream` (`~:1248`) → `run_thumb_batch` →
`thumbnail_cached`/`make_thumbnail_png` → `thumb_source::decode_thumb_image` does `std::fs::read(path)`
with **no gate**. The grid uses the streaming path, so the gate is effectively bypassed for the common case.

## Concrete pathological input
A folder containing a 6 GB file with an image extension (e.g. `huge.png`, even all zeros). Scrolling it
into view → `fs::read` slurps 6 GB → OOM. Lower amplification than CPE-1445/1446 (≈1:1 — attacker needs a
genuinely large file), but a real crash from merely browsing, and the guard already exists 40 lines away.
This is an inconsistency, not a design question.

## Fix direction
Apply `ensure_previewable_size` inside the `run_thumb_batch` compute closure (or per-request before
decode), mirroring the single `thumbnail` command. The gate is a Tauri-layer helper today, so place it
where the per-file read happens. A skipped oversize file falls back to the type icon (same UX as any decode
failure). Add a test that an oversize file is skipped by the streaming path.

## Effort / blast radius
XS / one closure in lib.rs — disjoint from the SVG (thumb_svg.rs) and doc_text.rs work, parallel-safe.

## Work Log (2026-08-07)

**Where the gate landed.** `src-tauri/src/lib.rs`: added a private `thumb_compute(path, edge,
cache_dir, cap)` helper right after `thumbnails_stream`. It calls the same `ensure_previewable_size`
helper the single `thumbnail` command already uses, THEN dispatches to `thumbnail_cached` (with a
cache dir) or `make_thumbnail_png` (without one) — the identical order the single `thumbnail` command
uses at `~:1212` (gate first, decode/cache second). `thumbnails_stream`'s `compute` closure passed to
`cpe_server::thumb_pipeline::run_thumb_batch` now reads
`|path, edge| thumb_compute(path, edge, cache_dir.as_deref(), PREVIEW_INFO_MAX_BYTES)` instead of
going straight to `thumbnail_cached`/`make_thumbnail_png`.

**Constant used.** `PREVIEW_INFO_MAX_BYTES` (128 MiB) — the exact same constant the single `thumbnail`
command passes to `ensure_previewable_size`, so both paths now refuse at the same size. `thumb_compute`
takes `cap` as a parameter (rather than hard-coding the constant) purely so unit tests can use a tiny
budget instead of allocating a real 128 MiB+ fixture file; production code always calls it with
`PREVIEW_INFO_MAX_BYTES`.

**How an oversize file is skipped.** `ensure_previewable_size` returns `Err` for a file over the cap
via a plain `fs::metadata` size check — no bytes are read. That `Err` propagates out of `thumb_compute`
through `run_thumb_batch`'s existing `Err(_) => None` arm (`crates/server/src/thumb_pipeline.rs:206`),
which is the exact same code path a decode failure already takes. The frontend already renders
`data_url: None` as the type-icon fallback, so no new error-handling branch was needed — the batch
keeps draining the rest of its queue and the stream's shape/contract is unchanged (per STREAMING.md).

**Root cause confirmed.** Read through the full path: `thumbnails_stream` → `run_thumb_batch` →
(previously) `thumbnail_cached`/`make_thumbnail_png` → `thumb_source::decode_thumb_image`, which does
an unconditional `std::fs::read(path)` at `crates/server/src/thumb_source.rs:83` for every non-video
extension, with no size check anywhere upstream of it. The `image::Limits` bomb-guard in that same file
only bounds *declared pixel dimensions* after the bytes are already in memory — it does nothing for a
file that's simply huge on disk (all-zeros 6 GB `.png`, say). Confirmed the single `thumbnail` command
(`~:1210-1212`) already had the `ensure_previewable_size` gate the streaming path lacked.

**Tests added** (`src-tauri/src/lib.rs`, all passing):
- `thumb_compute_rejects_an_oversize_file_before_reading_it` — a 4 KiB file against a 1000-byte cap is
  refused with a "too large" error.
- `thumb_compute_still_thumbnails_a_normal_size_file` — a small real PNG under `PREVIEW_INFO_MAX_BYTES`
  still decodes fine (no regression for the common case).
- `thumbnails_stream_pipeline_skips_oversize_and_thumbnails_normal_in_the_same_batch` — drives the real
  `cpe_server::thumb_pipeline::run_thumb_batch` (the exact function `thumbnails_stream` calls) with a
  two-request batch: the oversize file comes back `data_url: None` (icon fallback), the normal-size
  file in the *same* batch still comes back with a thumbnail — proves the gate doesn't collaterally
  break the rest of a batch.

**Verification run:**
- `cargo build` (src-tauri) — clean.
- `cargo clippy --all-targets -- -D warnings` (src-tauri, default features) — clean.
- `cargo test` (src-tauri) — 137 passed, 0 failed (includes the 3 new tests).
- `cargo clippy --all-targets -- -D warnings` (crates/server, default features) — clean (module
  untouched; ran to confirm no collateral breakage).
- `cargo test thumb` (crates/server) — 62 passed, 0 failed (module untouched; unaffected).
- No `specta::Type` struct touched (`thumb_compute` is a plain private fn, not exposed to the frontend
  contract) — no bindings regen needed.

**PR:** #711 (branch `cpe-1447-thumbnails-stream-size-gate`).
