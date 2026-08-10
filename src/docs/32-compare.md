---
title: Compare
order: 32
category: Explorer
categoryOrder: 2
---

# Compare

The **Compare** dialog diffs two folders or two files. Open it from the Tools menu, the command
palette ("Compare folders…"), or by selecting exactly two folders and choosing **Compare** from
their context menu (which pre-fills both paths). Type or paste a **left** and **right** path and
click **Compare** — the dialog figures out what kind of compare to run from what you gave it.

## Folder compare

Two folders get scanned and diffed into a tree: each entry is marked **added** (right-only),
**removed** (left-only), **changed**, or **same**, with a summary row (`+added −removed ~changed
=same`) above the tree. A folder with any changed descendant rolls up as changed itself, so you can
tell at a glance which branches to expand. Click a row with children to expand/collapse it.

## Text and byte compare

Two files that aren't images get a **line diff** when both decode as UTF-8 text (added/removed line
counts, plus the changed lines themselves), or a **byte compare** when either isn't valid text
(byte-for-byte equal, or the first differing offset, the number of differing ranges, and whether the
lengths match).

## Image compare

Two files recognised as images by extension (JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF) get the
**image compare** pane instead, backed by the same pixel-diff engine used by other diff tooling in
the app. It has three toggled sub-views:

- **Side-by-side** — the two images shown adjacently. Drag either one to pan, scroll the wheel to
  zoom (centered on the cursor), or use the **−** / **+** / **Reset** buttons; zoom and pan are
  shared between the two panes so they always line up.
- **Onion-skin** — the two images stacked directly on top of each other, blended by a slider: drag it
  toward **left** or **right** to fade between the two. Zoom/pan work the same way as side-by-side.
- **Heatmap** — a grayscale mask highlighting exactly which pixels differ, alongside the percentage
  different and a changed/total pixel count. When there's a single contiguous changed region, a
  **Zoom to changed region** button jumps the mask view straight to it (a highlighted rectangle marks
  the region on the heatmap itself); when nothing changed, the pane says so instead.

If the two images aren't the same size, a note above the sub-view tabs says so — the pane still shows
a result, aligned from the top-left corner over the union of both images' extents, with the padded
area counted as changed (rather than silently overlaying mismatched dimensions as if they lined up
pixel-for-pixel).

## Limits

The image-compare pane loads both source images directly (no resizing/downsampling before display),
so a pair of very large images may take a moment to decode in the webview. There's currently no way
to export the heatmap mask or a compare report to a file.
