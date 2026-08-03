---
id: CPE-1271
title: "Guard: every runtime-resolved resource must be present in the shipped sidecar bundle"
type: chore
component: build
priority: high
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-862
---

## Problem (this class of bug bit twice in one session)
Features worked against the BASE tauri config but broke in the SHIPPED sidecar build because the code resolved a
resource that the sidecar `--config` overlays didn't bundle:
- CPE-1258/1267: pdfium.dll path (thumbnails) — resolved via resource_dir but path/bundle mismatch.
- CPE-1270: `icons/icon.png` (drag-out preview) — base tauri.conf.json's ARRAY-form `bundle.resources`
  `["icons/icon.png"]` was REPLACED by the overlays' OBJECT-form resources, silently dropping the icon.
The shipped sidecar bundle (base tauri.conf.json + `tauri.sidecar.conf.json` + `tauri.sidecar.{windows,unix}.conf.json`
+ `tauri.sidecar.pdfium.{windows,linux,macos}.conf.json`, merged by tauri's `--config`) is what users run, and nothing
verified it contains the resources the runtime code depends on.

## Build — a guard so this can't recur
Add a test (place where it runs in CI — a Rust test in a crate CI runs, or a node/vitest test; pick the one that
naturally covers config JSON) that:
1. Loads + merges the sidecar config the RELEASE uses (base `src-tauri/tauri.conf.json` + the sidecar/pdfium overlays),
   applying tauri's merge semantics (an overlay object REPLACES a base array at the same key — the exact gotcha).
2. Computes the final `bundle.resources` set actually bundled for the shipped (windows + unix) sidecar builds.
3. Asserts the REQUIRED runtime resources are all present, per OS: the drag preview icon (`icons/icon.png` → resolved
   by `src/lib/dragOut.ts` resolveDragIcon), pdfium (`pdfium.dll`/`libpdfium.so`/`libpdfium.dylib` → thumb_pdf),
   ffmpeg (`ffmpeg`/`ffmpeg.exe` → thumb_video), and the sidecar binaries. Fail with a clear message naming any
   resource the code needs but the merged config wouldn't bundle.
4. Keep a single canonical list of "runtime-required bundled resources" with a comment pointing at each consumer, so
   adding a new resource dependency has one obvious place to register + is enforced.

Optional but recommended: also make the base `tauri.conf.json` `bundle.resources` object-form (so it MERGES with
overlays instead of being replaced), removing the footgun at the source — but keep the guard regardless.

## Acceptance criteria
- The guard fails if any required runtime resource is absent from the merged shipped sidecar `bundle.resources`
  (verify by temporarily removing icons/icon.png from the overlay → test goes red).
- Passes on current main (all resources now present). `npm run check`/cargo as applicable clean. Runs in CI.
