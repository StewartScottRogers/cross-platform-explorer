---
title: "How to add PDF + video thumbnails (CPE-1238) without bloating/relicensing the signed binary?"
date: 2026-08-02
tags: [thumbnails, cpe-1238, cpe-718, pdfium, ffmpeg, native-deps, feature-gate, licensing, binary-size, decode_thumb_image]
status: current
---

## Question
CPE-1238 (epic CPE-718) needs PDF first-page + video representative-frame thumbnails. These need heavy
native rendering. How to add them while respecting PURPOSE's small/fast/predictable tiebreaker and keeping
the code-signed release license-clean?

## Decision (Foreman decide-and-log, 2026-08-02)
**Hybrid, both feature-gated off-by-default:**
- **PDF → `pdfium-render` (MIT/Apache) + dynamically-loaded pdfium prebuilt (BSD-3-Clause), IN-PROCESS**,
  behind `#[cfg(feature="pdf-thumb")]`. Chrome's PDF engine = best fidelity; license links + signs cleanly.
  **Reject `mupdf` (AGPL) outright** — landmine in a distributed binary. Pure-Rust `lopdf`/`pdf-render`
  can't render arbitrary PDFs faithfully.
- **Video → shell out to a BUNDLED `ffmpeg` executable (no linking)**, behind `#[cfg(feature="video-thumb")]`.
  Keeps ffmpeg a separate program (mere aggregation) so LGPL/GPL never attaches to our signed binary.
  `ffmpeg -ss <~10% in> -i <path> -frames:v 1 -vf scale=... -f image2 <tmp>.png` (seek ~10% avoids the
  black lead-in frame). Reject `ffmpeg-next`/`ffmpeg-sys`/`video-rs`/GStreamer — they LINK ffmpeg (license
  + brutal cross-platform CI). Prefer an LGPL (`--enable-gpl`-free) ffmpeg build to be conservative.

**Integration = (a) in-process feature-gate for PDF + (c) sidecar/shell-out for video.** NOT download-on-demand
(adds runtime fetch/trust/signing surface; breaks offline/first-run) — CI bundles the binaries as bundle
`resources` (same mechanism the sidecars already use). Binary-size delta: PDF ~6-13 MB + video ~15-25 MB
per platform (minimal ffmpeg) ≈ **~25-40 MB total**; **features OFF = exactly 0** (delete-test holds).

## Wiring
Both plug into `crates/server/src/thumb_source.rs::decode_thumb_image(path, max_edge)`:
- **Video arm goes FIRST, before `fs::read`** (never slurp a multi-GB video): `thumb_video::extract_frame(path, max_edge)`.
- **PDF arm** alongside svg/psd: `"pdf" => thumb_pdf::render_first_page(&bytes, max_edge)?`.
- Mirror `thumb_svg.rs`: self-contained extractor → `DynamicImage`, bomb-guard, Err on malformed.
- **Graceful fallback already exists:** feature off (or lib/ffmpeg missing at runtime) → extension falls
  through to the default image decode → `Err` → frontend shows the generic type icon. No new logic, no panic.
- The thumbnail Tauri command already runs in `spawn_blocking`, so the synchronous pdfium render + ffmpeg
  subprocess are off the UI thread — **no new async path needed.**
- Features declared in `crates/server/Cargo.toml [features]` (`pdf-thumb = ["dep:pdfium-render"]`,
  `video-thumb = []`), ship-enabled in `src-tauri/Cargo.toml` cpe-server dep line (where `index` is turned on).

## Slices (child tickets under CPE-718)
- **CPE-1256** — PDF first-page extractor `thumb_pdf.rs` behind `pdf-thumb` (+ Cargo feature/dep, lib.rs mod,
  one thumb_source arm, cargo tests). Conflict surface: thumb_pdf.rs (new), Cargo.toml, lib.rs, thumb_source.rs.
- **CPE-1257** — Video rep-frame extractor `thumb_video.rs` via bundled ffmpeg behind `video-thumb` (+ early
  thumb_source dispatch, sidecar conf resource lines). ffmpeg present locally (v8.1.1) → real-render test runs
  headless. Conflict surface: thumb_video.rs (new), Cargo.toml, lib.rs, thumb_source.rs, sidecar confs.
- **CPE-1258** — Ship-enablement + CI + docs: turn on both features in src-tauri/Cargo.toml, per-feature
  clippy+test CI jobs (mirror ci.yml:167-178), pdfium/ffmpeg binary acquisition in release-sidecar.yml,
  docs (src/docs + sectionDocs.ts, CPE-579). Depends on 1256+1257.

## Verification reality
ffmpeg present locally → CPE-1257 real-render test is headless-runnable now. pdfium NOT present → CPE-1256's
real-render test either fetches a pdfium prebuilt or is presence-gated + relies on CI. Full end-to-end
bundling verified only once CI wires the resource acquisition (CPE-1258) — CI currently intermittently
stalled, so verify locally where possible.

## Licensing bottom line
Ship-safe: pdfium (BSD) links in-process, ffmpeg stays a separate bundled exe (carry its LICENSE in the
bundle). Never link mupdf (AGPL).
