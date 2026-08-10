---
id: CPE-1508
title: "Image compare view: side-by-side / onion-skin / pixel-diff heatmap pane in the compare shell"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-722
parent: CPE-1490
created: 2026-08-08
---
## Context
CPE-1490 built the headless backend engine for image compare — the deferred image-compare scope of the
CPE-722 compare studio (CPE-779 shipped folder/binary/text compare and explicitly deferred image compare
as a follow-up). The backend half is done and shipped: a new `diff_images(a, b)` Tauri command
(`cpe_server::image_diff::diff_images`, backed by the already-bomb-guarded thumbnail decoder, no new
dependency) returns an `ImageDiff`:

```ts
type DiffBBox = { x: number; y: number; width: number; height: number }
type ImageDiff = {
  width: number; height: number;
  changedPixels: number; totalPixels: number; percentDifferent: number;
  bbox?: DiffBBox | null;      // absent when nothing changed
  sizeMismatch: boolean;       // true when the two inputs' dimensions differed
  maskPng: number[];           // raw PNG bytes of a grayscale diff-mask heatmap, width x height
}
```

This ticket is the **frontend half only**: wire that command into a real view in the existing compare
shell. GUI-exclusive value — a pixel-diff heatmap is meaningless in a terminal, so this is the
"GUI beats TUI" capstone CPE-722's Definition of Done named but CPE-779 didn't build.

## Scope
Add an **image compare** mode/tab to the existing compare shell (reuse CPE-722/779's compare view
plumbing — selection of two images, the existing compare pane chrome) with three sub-views, toggled:
- **Side-by-side** — the two images shown adjacently with synced zoom/pan.
- **Onion-skin** — the two images stacked with an opacity slider blending between them.
- **Pixel-diff heatmap** — render `maskPng` as an overlay/standalone image; surface `percentDifferent`,
  `changedPixels`/`totalPixels`, and highlight/zoom-to `bbox` (the changed region) when present.

Handle `sizeMismatch: true` gracefully in the UI: show a visible note ("images differ in size — showing
the union canvas, padded region counts as changed") rather than silently presenting mismatched dimensions
as if they lined up pixel-for-pixel — this mirrors how the backend documents its own alignment choice
(top-left-aligned union bounding box; see `crates/server/src/image_diff.rs` module docs).

## How
- Frontend: a new tab/pane in the compare shell, following the existing compare UX (selection → the two
  images feeds `diffImages(a, b)` from `src/lib/bindings.gen.ts`).
- `maskPng` currently comes back as a raw `number[]` (PNG bytes) via specta/serde — reasonable for a
  typed command, but consider whether to wrap it as a `data:image/png;base64,...` URL client-side (cheap:
  base64-encode the byte array before setting `<img src>`) or whether the backend should be asked to
  return that format directly, matching the `thumbnail` command's convention. Decide and note in this
  ticket's Work Log; don't silently ship an inefficient wire format without at least considering it.
- Busy-cursor convention: `diffImages` must be called through `invoke` from `src/lib/invoke.ts` (never
  `@tauri-apps/api/core` directly) per BUSY-CURSOR.md, since a diff on large images is a real (if bounded)
  compute cost.
- In-app docs (CPE-579): if this adds a user-facing compare-shell section, add/update its page in
  `src/docs/*.md` and its `section → doc slug` entry in `src/lib/sectionDocs.ts` — the guard test
  (`src/lib/sectionDocs.test.ts`) will fail CI otherwise.

## Verify
`npm run check`. gui-smoke can exercise the new pane once relevant (side-by-side render, onion-skin slider
interaction, heatmap toggle + bbox highlight, size-mismatch note). Manual/GUI-verify: build → install the
sidecar build → run the real app (never `tauri dev` per convention) and confirm the three sub-views render
correctly against a real image pair, including a same-size identical pair, a one-pixel-different pair, and
a differing-dimensions pair.

## Notes
Backend engine + its own test suite (8 tests, `cargo test` in `crates/server`) already shipped in CPE-1490
— do not re-litigate the backend's diff algorithm, bounded-decode guard, or differing-dimension alignment
choice here; this ticket only consumes the existing `diff_images` command. If the alignment/scoring choice
documented in `image_diff.rs` turns out to be wrong for the UI's needs (e.g. product wants scale-to-fit
instead of union-bbox letterboxing), that's a backend change and should be a small follow-up against
CPE-1490's module, not reimplemented client-side. Epic CPE-722; parent CPE-1490.

## Work Log
- 2026-08-09: Implemented frontend-only, reusing `CompareDialog.svelte` (CPE-779) rather than a parallel
  shell. `CompareDialog.compare()` already had a folder-scan-fails → file-pair fallback chain (text, then
  byte); added an image branch ahead of it, gated on both paths matching `filetypes.ts`'s existing
  `isImage()` extension check (JPEG/PNG/GIF/WebP/BMP/TIFF/AVIF) — mirrors how the rest of the app already
  decides "is this an image" rather than inventing a second list. On a match it calls
  `commands.diffImages(a, b)` (the generated typed client, which already routes through `src/lib/invoke.ts`
  — confirmed by reading `bindings.gen.ts`'s `TAURI_INVOKE` import, so no extra busy-cursor wiring was
  needed) and renders the new `ImageCompareView.svelte`.
  - **Wire-format decision (asked for explicitly in Scope):** kept `maskPng` as raw `number[]` PNG bytes
    over IPC — did NOT ask the backend to pre-encode base64. Base64-encoded a thumbnail-scale mask
    client-side instead, reusing the *existing* chunked `bytesToBase64` helper in `terminalClient.ts`
    (already used for PTY output, already handles the "don't blow the call stack via
    `String.fromCharCode(...bytes)` on a big array" case) rather than writing a new one — this is a single
    small array per compare, not a hot path, so the client-side pass costs nothing worth asking for a
    bespoke backend wire shape over. New pure helper: `src/lib/imageDiffView.ts` (`maskPngToDataUrl`,
    `bboxRectPercent`, `zoomToBBox`, `clampZoom`, `formatPercentDifferent`), unit-tested in
    `imageDiffView.test.ts` (13 tests, round-trips the encode via `terminalClient.ts`'s `base64ToBytes`,
    covers a 200k-byte mask to prove the chunking actually matters).
  - **UX assumptions:** the two SOURCE images (not the mask) are loaded via `assetUrl` (`convertFileSrc`
    in the app, forwarded the same way `PreviewPane.svelte` already takes it) straight from disk — no
    backend decode round trip, since the extension gate already limits to formats the webview renders
    natively. Only the mask (which has no path — it's an IPC value) goes through the base64 data-URL path.
    Side-by-side and onion-skin share one zoom/pan state (wheel-zoom centered on cursor, pointer-drag to
    pan — pointer events, not HTML5 DnD, per CPE-1525's WebView2 finding); heatmap's "Zoom to changed
    region" uses `zoomToBBox`'s pure math against the mask viewport. `sizeMismatch: true` renders a visible
    themed note above the sub-view tabs (`var(--warn)`, matching `AgentTimeline`/`SidecarManager`'s
    existing warning-note convention) rather than silently overlaying mismatched dimensions.
  - **Docs (CPE-579):** `CompareDialog` (CPE-779) shipped in 2026 without ever getting a `Section`/doc-page
    entry — grepped `sectionDocs.ts` and `src/docs/*.md` and confirmed there was no pre-existing "compare"
    page at all, not even for the folder/text/byte modes. Since image-compare needed documenting somewhere
    anyway, closed that pre-existing gap rather than bolting the new sub-views onto nothing: added
    `compare` to `Section`, mapped it to a new `src/docs/32-compare.md` (covers all four modes — folder,
    text, byte, image), and wired `App.svelte`'s `currentSection()` to return `"compare"` while the dialog
    is open (so F1 while comparing opens the right page instead of falling through to Explorer's).
    `sectionDocs.test.ts` still passes (guard test is exhaustive over `Section`, so this was required, not
    optional, once `compare` was added to the union).
  - **Tests run:** `npm run check` (0 errors/warnings) · `npx vitest run` — full suite, 234 files / 2656
    tests, all green (no collateral breakage) · new: `src/lib/imageDiffView.test.ts` (13 tests, pure logic)
    + 2 new `CompareDialog.test.ts` cases (image-mode happy path + size-mismatch note).
  - **Not run / can't verify headless:** real GUI render (side-by-side pan feel, onion slider, heatmap
    zoom-to-bbox against a real image pair, size-mismatch against real mismatched files) — this build is
    headless; per the ticket's own Verify section that's expected, deferred to gui-smoke + the Foreman's
    UAT/Visual-Critic legs. No backend changes were made or needed (`diff_images` already shipped in
    CPE-1490); did not touch `crates/server`.
- 2026-08-09 (PR #746 review fixes): independent reviewer caught two real bugs in the zoom/pan wiring,
  both fixed on the same branch/worktree.
  - **Bug A — heatmap "Zoom to changed region" was a no-op.** `zoomToChangedRegion` guarded on
    `!viewportEl`, but `viewportEl` (`bind:this`) was only ever wired on the side-by-side left pane and
    the onion-skin container — never in the heatmap branch, so it was `undefined` (Svelte clears a
    `bind:this` target on unmount) every time the button was actually clickable. Fixed by binding
    `viewportEl` on the new `.ic-heat-img-wrap` too, and wiring the same `on:wheel`/pointer handlers there
    so heatmap gets wheel-zoom + drag-pan like the other two sub-views. Also: the heatmap `<img>` never
    had `style={transformStyle}`, so even a correct zoom state wouldn't have moved it — fixed by wrapping
    the image AND the `.ic-bbox` highlight in one new `.ic-heat-canvas` div and applying the transform to
    that shared container (not to each element separately — they have different pre-transform origins,
    the letterboxed/centered image vs. the percent-positioned bbox, so transforming them independently
    would have scaled them apart from each other instead of together). `.ic-heat-canvas` sizes via
    `aspect-ratio: {diff.width} / {diff.height}` + `max-width/max-height: 100%` (the modern CSS letterbox
    pattern) since it's a plain div, not an image with its own intrinsic size. Also stopped hiding the
    zoom toolbar for heatmap (`{#if subView !== "heatmap"}` removed) so Reset is reachable after a jump.
  - **Bug B — wheel-zoom cursor-anchoring was wrong for the right side-by-side pane.** `onWheel` computed
    its anchor from the shared `viewportEl` (bound only to the LEFT pane), so zooming over the right pane
    anchored against the left pane's rect — off by ~one pane width. Fixed by reading
    `(e.currentTarget as HTMLElement).getBoundingClientRect()` directly in `onWheel` instead (every pane
    wires `on:wheel={onWheel}` itself, so `currentTarget` is always the exact hovered element); kept
    `viewportEl` only for the button-triggered (no cx/cy) zoom-in/out center fallback in `zoomBy`.
  - **New test file `src/lib/components/ImageCompareView.test.ts`** (3 tests, direct component tests, no
    backend mocking needed since `diff` is passed in as a prop): (1) *heatmap zoom-to-region actually
    changes zoom state* — clicks `ic-subview-heatmap` then `ic-zoom-to-bbox` with a real `bbox`, asserts
    `ic-zoom-pct` moves off `"100%"` and `ic-heat-canvas`'s `transform: scale(...)` reflects it (stubs
    `clientWidth`/`clientHeight` on the bound wrap since jsdom does no real layout); (2) *Reset is
    reachable from heatmap* — zooms via the button then clicks "Reset zoom", asserts back to `"100%"`;
    (3) *wheel-zoom anchors on the actually-hovered pane* — stubs distinct `getBoundingClientRect()` rects
    on the left (x=0) and right (x=500) panes, wheels over the right pane, and asserts the resulting pan
    stays small (~-33px, consistent with anchoring 200px into the 400px-wide right pane) rather than the
    ~3.5x-larger pan (~-117px) the bug would have produced by anchoring against the left pane's rect.
  - **Re-verified:** `npm run check` — 0 errors/0 warnings. `npx vitest run` (full suite) — 235 files /
    2659 tests, all green (up from 234/2656 pre-fix, +1 file/+3 tests for the new regression coverage; no
    other collateral change). Pushed to the existing `cpe-1508` branch — PR #746 updates in place.
