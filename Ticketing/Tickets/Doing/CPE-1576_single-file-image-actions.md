---
id: CPE-1576
title: "Single-file image actions in the preview pane (rotate / convert / copy image)"
type: Task
status: Doing
priority: Medium
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1568 slice 3. Images preview fine but have NO single-file actions in the pane. The batch-media backend
(CPE-1093: resize/convert/rotate/…) already exists but is gated to multi-select (≥2). Surface the common single-image
actions on the CPE-1570 action bar for the currently-previewed image.

## Scope
- Declare `actions` on the image providers (`image`, `decoded-image`, `raw-image`, `heic`) in
  `src/lib/preview/provider.ts` using the CPE-1570 `PreviewAction`/`PreviewActionCtx` API: **Rotate left/right**,
  **Convert…** (to another format), **Copy image** (to clipboard). Reuse the existing batch-media backend commands —
  drop the "≥2 selected" gate for a single-file path (find `beginBatchMedia`/`batchMediaFor` in `App.svelte` +
  the underlying commands; wire a single-file invocation). Do NOT duplicate the backend.
- Rotate should persist (write the rotated image) with a confirm/undo-aware path if one exists; Convert prompts for
  target format; Copy image puts the decoded image on the clipboard.
- Labels via `$t()` (keys in all 12 locales, CPE-481 gate); Icon glyphs; theme-only colors (MENUS.md).

## Acceptance criteria
- Opening an image shows Rotate/Convert/Copy in the action bar; each performs the operation via the existing backend.
- Rotate writes the result (respects the no-overwrite / backup conventions used by batch-media); Convert produces the
  chosen format; Copy image works.
- Unit/component tests: actions render + run (mock backend), enablement gating; `npm run check` clean; vitest green.
- Frontend-only wiring (reuse backend); no new deps.

## Notes
Builds on CPE-1570 action bar + CPE-1093 batch-media backend. Touches `provider.ts`/`PreviewPane.svelte`/`App.svelte`
— serialize vs other CPE-1568 preview slices (Foreman won't run them concurrently). Verify against the actual
batch-media command signatures. Model: sonnet.
