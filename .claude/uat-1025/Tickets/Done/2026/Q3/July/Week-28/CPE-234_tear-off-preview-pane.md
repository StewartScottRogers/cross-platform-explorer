---
id: CPE-234
title: Tear-off preview pane into a floating tabbed window
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 3-4h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

A "Pop out" button on the right (preview) pane detaches the current file's
preview into its own floating window, pinned to that file. The in-app pane keeps
working for the next selection. Popping out again while a floating window exists
adds the new preview as a TAB in that same window (one float, many tabs).
Floating tabs are close-only.

Decisions (user, 2026-07-12): pop-out button (not drag); pinned to file;
auto-dock as a new tab; close-only.

## Acceptance Criteria
- [ ] A "Pop out" button on the preview pane opens the current file in a floating window.
- [ ] The floating window is pinned to that file; the in-app pane is unaffected.
- [ ] A second pop-out docks as a new tab in the existing floating window.
- [ ] Tabs can be closed; closing the last tab closes the window.
- [ ] Duplicate pop-out of an already-open file focuses its tab instead of duplicating.
- [ ] `npm run check` passes; `cargo build` compiles (capabilities valid).

## Resolution

Added a floating "float mode": `main.ts` checks `?float=1` and mounts
`FloatPreview.svelte` (tabbed pinned previews) instead of the explorer.
`FloatPreview` reuses `PreviewPane` + `DetailsPane` and the extracted
`lib/preview/loaders.ts`; it listens for `float:add` (DirEntry) to add a tab,
de-dupes by path, and closes the window when the last tab closes.

`App.svelte` adds a Pop-out button (share icon) to the preview toggle row,
enabled when exactly one entry is selected. `popOutPreview()` gets-or-creates the
`preview-float` WebviewWindow (`index.html?float=1`), waits for a `float:ready`
handshake (with a 2.5s fallback so a slow load can't hang), emits `float:add`
with the selected entry, and focuses the window. The in-app pane is untouched, so
it keeps following the selection while the float stays pinned.

Capabilities: added `core:webview:allow-create-webview-window`,
`core:event:default`, `core:window:allow-close`, `core:window:allow-set-focus`,
and the `preview-float` window label.

Verified: `npm run check` 0/0; `npm run build` compiles; `cargo build` validated
the new capability identifiers and window label. Live multi-window interaction to
be spot-checked with the 0.10.0 build.

## Work Log

2026-07-12 — Built float mode (main.ts ?float), FloatPreview tabbed window reusing PreviewPane + shared loaders.
2026-07-12 — App pop-out button + get/create preview-float window + float:ready/float:add handshake.
2026-07-12 — Added webview-create/event/window-close/set-focus caps + preview-float label. check + both builds clean.

## Notes
Second window runs the same app in "float mode" (`index.html?float=1`) rendering
FloatPreview (tabs + reused PreviewPane). Main ↔ float coordinate over Tauri
events (`float:add` / `float:ready` handshake). Needs
`core:webview:allow-create-webview-window` + the `preview-float` window label in
capabilities. Reuses the preview loaders (extracted to lib/preview/loaders.ts).
