---
id: CPE-1490
title: "Finish image compare (side-by-side / onion-skin / pixel-diff heatmap) — the deferred CPE-722 scope"
type: Feature
status: Deferred
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-722
created: 2026-08-08
---
## What (close a capstone epic that's visibly 3/4 done)
CPE-722 (compare studio) named **image compare** in its Definition of Done, but CPE-779 shipped
folder/binary/text compare and **explicitly deferred image compare** as a follow-up — it was never built. This
is the most visually GUI-exclusive comparison mode (a pixel-diff heatmap is meaningless in a terminal), so it's
a strong "GUI beats TUI" capstone. Surfaced by the competitive-landscape GUI survey (Directory Opus / ForkLift
image tools).

## Scope
Add an **image compare** mode to the existing compare shell (reuse CPE-722/779's compare view + `cpe-server`
compare crate). Two selected images →:
- **Side-by-side** (synced zoom/pan).
- **Onion-skin** (opacity slider blend).
- **Pixel-diff heatmap** (per-pixel delta highlighted; report % different + bounding region).
Handle differing dimensions gracefully (align/letterbox + note the mismatch). Bounded decode (mirror the
thumbnail pipeline's size/resource-exhaustion guards — never decode unbounded).

## How
- Backend: extend the `cpe-server` compare module with an image-diff function (decode both — reuse the
  thumbnail/`image` decode path already in-tree, **no new heavy dep**; bounded), returning the diff mask +
  stats. Headless-testable with small fixture images.
- Frontend: a new tab/pane in the compare shell (side-by-side / onion-skin / heatmap toggle). Wire selection →
  the two images, per existing compare UX.

## Verify
Backend: `cargo test` with fixture image pairs (identical → 0% diff; one-pixel change → detected; differing
sizes → graceful; garbage → Err). `cargo clippy --all-targets -D warnings`. Frontend: `npm run check`;
gui-smoke can exercise the pane once the gui-smoke suite is green (CPE-1481).

## Effort
Medium. Backend diff = pure logic + fixtures (headless-buildable half is a clean batch); the view is the GUI
half. Splits cleanly along the crate seam. Epic CPE-722.

## Work Log

### 2026-08-08 — Backend engine implemented + shipped; GUI view split off as CPE-1508

**Backend-first split (mirrors CPE-1478 waveform / CPE-1485 binary-arch):** this pass builds only the
headless image-diff engine. The GUI pane (side-by-side / onion-skin / heatmap toggle in the compare
shell) is scoped separately as **CPE-1508** and filed to the Backlog — this ticket stays in `Doing/` with
the backend half done.

**New module (`crates/server/src/image_diff.rs`):** `pub fn diff_images(a: &Path, b: &Path) ->
Result<ImageDiff, String>`. Reuses `cpe_server::thumb_source::decode_thumb_image` for both inputs — the
same bomb-guarded decoder the thumbnail pipeline already uses (CPE-1447/CPE-1449: 20,000×20,000 /
256 MiB decode caps, source-file-size gate before the read) — rather than adding a second decode path or
a new dependency.

**Result shape:**
```rust
pub struct DiffBBox { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
pub struct ImageDiff {
    pub width: u32, pub height: u32,
    pub changed_pixels: u64, pub total_pixels: u64, pub percent_different: f64,
    pub bbox: Option<DiffBBox>,       // None when nothing changed
    pub size_mismatch: bool,          // true when the two inputs' dimensions differed
    pub mask_png: Vec<u8>,            // grayscale-in-RGBA PNG, width x height, brighter = larger delta
}
```
`mask_png` is raw PNG bytes (the Tauri command layer / a future GUI can wrap it as a `data:` URL exactly
like the `thumbnail` command does) — not base64-encoded in the struct itself, so the payload stays
smaller for any non-UI consumer.

**Differing dimensions — decided + documented:** aligned at the top-left corner of the **union bounding
box** (`max(width)` x `max(height)`), not scaled to match (scaling would distort a real size difference
into a false pixel match) and not rejected outright. Any coordinate with no counterpart on one side is
scored as maximally different (delta 255) — the padding strip reads as fully "changed" in the heatmap.
`size_mismatch: true` flags the pair explicitly rather than making callers infer it from geometry.

**Bounded, twice over:** on top of `decode_thumb_image`'s own decode cap, a new `MAX_DIFF_DIMENSION =
4096` bounds the union canvas a diff will actually be computed over — a diff multiplies cost by two
source images plus a same-sized mask buffer, so two images that individually clear the thumbnail
pipeline's 20,000×20,000 cap would otherwise still risk a multi-gigabyte spike building diff/mask
buffers. An oversized pair is refused with `Err` (message contains "too large") before any diff/mask
buffer is allocated.

**Tauri command (`src-tauri/src/lib.rs`):** `async fn diff_images(a: String, b: String) ->
Result<cpe_server::image_diff::ImageDiff, String>`, `spawn_blocking`-wrapped, one-line dispatch — thin
per convention. Registered in both `generate_handler![]` and the `export_bindings` `collect_commands![]`
list, next to `files_identical`. `bindings.gen.ts` regenerated (`cargo run --bin export_bindings
--features "specta-bindings sidecar-platform"`): additive 34-line diff adding `diffImages`, `ImageDiff`,
`DiffBBox`; nothing else drifted. No new Cargo dependency — `Cargo.lock` unchanged (verified via `git
status`/`git diff --stat` on all three lockfiles).

**Tests (`crates/server/src/image_diff.rs`, 8 new, all in-code fixtures via the `image` crate, no
committed binaries):** identical images → 0% diff, changed_pixels 0, bbox None, mask decodes as
all-black; one-pixel change → changed_pixels 1, exact bbox `{3,7,1,1}`, percent 1.0; fully-different
(solid black vs. solid white) → changed_pixels == total, percent 100.0, bbox spans the whole canvas;
differing dimensions (4x4 vs 6x6, same color) → aligned to the 6x6 union, size_mismatch true, exactly the
20 non-overlap pixels counted changed, no panic; garbage bytes with a `.png` extension → `Err` (both
argument orders); a real PNG truncated to 16 bytes (signature + partial IHDR) → `Err`; a missing path →
`Err`; a real (not header-only-bomb) image genuinely decoded at `MAX_DIFF_DIMENSION + 1` px wide → `Err`
containing "too large", proving the diff-specific cap fires even for a source that fully clears
`decode_thumb_image`'s own bound.

**Verification results:**
- `cargo test` crates/server (default features): 1776 passed, 0 failed (was 1768 before this ticket; +8
  new `image_diff` tests, no regressions, no skips/panics).
- `cargo clippy --all-targets -D warnings` crates/server: clean for default, `--features index`, and
  `--features pdf-thumb,video-thumb,waveform,dicom-thumb` (the three combos CI runs).
- `cargo build` src-tauri: clean.
- `cargo clippy --all-targets -D warnings` src-tauri: clean for default and `--features sidecar-platform`.
- `bindings.gen.ts` regenerated as noted above; no Cargo.lock drift in any of the three lockfiles.
- `npm run check` NOT run — no frontend code touched (explicitly backend-only this pass); GUI consumer is
  CPE-1508.

**For Reviewer to scrutinize:** the bounded-decode path (relies entirely on `decode_thumb_image`'s
existing guard — no new decode logic here) plus the diff-specific `MAX_DIFF_DIMENSION` layered on top;
the differing-dimensions alignment/scoring choice (union-bbox + max-delta padding, documented in the
module doc, not silently panicking or truncating); and the no-panic-on-garbage/truncated/missing-file
paths (all covered by dedicated tests). `mask_png`'s wire representation (`number[]` via specta/serde —
confirmed in the regenerated `bindings.gen.ts`) is a reasonable default but CPE-1508 may want to revisit
it as a `data:` URL for payload size on the GUI side, same as `thumbnail`'s convention.

**Status:** backend done; GUI view = CPE-1508. Leaving this ticket in `Doing/` for the Foreman to move to
Blocked/Deferred/Done as appropriate.

## 2026-08-08 (sprint) — BACKEND SHIPPED (PR #726, cbae346f); DEFERRED pending GUI view CPE-1508
The image-diff engine (`crates/server/src/image_diff.rs` `diff_images` — side/onion/heatmap-ready mask + stats,
bounded decode, differing-dim union canvas) is merged and gauntlet-verified (Reviewer APPROVE + UAT PASS: real
photos + 8k+ adversarial inputs, 4096 cap fires, no panic/OOM). The remaining scope — the side-by-side /
onion-skin / heatmap **GUI pane** in the compare shell — is split to **CPE-1508**. Deferred (not Done) because
this ticket's own DoD included the view; it's postponed pending CPE-1508 which owns it.
