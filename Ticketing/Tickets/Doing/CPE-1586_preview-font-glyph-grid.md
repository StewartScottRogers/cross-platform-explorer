---
id: CPE-1586
title: "Per-file-type pane slice 5: font glyph grid + specimen view with copy-glyph actions"
type: Feature
status: In Progress
priority: Medium
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1568 (type-aware right pane) slice 5. Fonts (`.ttf`, `.otf`, `.woff`, `.woff2`) currently get no
custom preview — selecting one shows the generic fallback. A font should show a **specimen + glyph grid**
plus the actions that make sense for a font.

## Depends on
CPE-1570 (action-bar groundwork, merged #783) — the declarative `actions?: PreviewAction[]` field on
`PreviewProvider` + the generic action bar in `PreviewPane.svelte`. **Reuse it; do not fork the registry.**

## Scope
- A font preview provider in `src/lib/preview/provider.ts` for the font extensions, rendering:
  - a **specimen** line (editable sample text is a nice-to-have, not required) rendered in the actual font,
  - a **glyph grid** of the font's characters, and
  - basic metadata (family, style, version, glyph count) where cheaply available.
- Render the real font by loading it as a `FontFace` from the file bytes (read through the existing preview
  byte-loading path / `invoke.ts`) — no new dependency unless genuinely required (lean-core guardrail; if a
  parser is needed, prefer a **pure-Rust** one in `cpe-server` and justify it in the work log).
- **Actions** on the action bar: copy the selected glyph (character), copy its codepoint, and — only if it is
  cheap and safe — "Open with system font viewer". Skip anything requiring elevation/install privileges.
- Large fonts must not stall the pane: cap/virtualize the glyph grid per STREAMING.md and PURPOSE.md's
  fast/small/predictable tiebreaker.
- **Docs per CPE-579**: update the relevant `src/docs/*.md` preview page with the font view + its actions.
  This is a preview provider, **not** a new `Section` — do **not** edit `src/lib/sectionDocs.ts`.
- Tests: provider selection by extension, glyph-grid capping, and the copy actions.

## Acceptance criteria
- Selecting a font file in the middle pane shows the specimen + glyph grid in the right pane.
- Copy-glyph / copy-codepoint work from the action bar (per the CPE-1570 action shape).
- `npm run check` + frontend tests green; UI follows MENUS.md / theme tokens (no hard-coded colours) and the
  pill/tick-tack reflow rule if any chips are used.

## Notes
Model: sonnet. Conflict surface: `src/lib/preview/provider.ts`, `src/lib/preview/*` (new component),
`PreviewPane.svelte`, one `src/docs/*.md` page. Do **not** touch `App.svelte`, `sectionDocs.ts`,
`ContextMenu.svelte`, or `src-tauri/src/lib.rs` (other workers are in those files).
