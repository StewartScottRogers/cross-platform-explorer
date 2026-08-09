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
