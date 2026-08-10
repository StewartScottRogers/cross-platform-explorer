---
id: CPE-1573
title: "JSON tree preview + actions (collapsible tree, copy path/value, format, validate)"
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
Epic CPE-1568 slice 2 — the highest-value gap: JSON files render today as a pretty-printed `<pre>` only. Give them
a proper collapsible tree + JSON-specific actions, using the action-bar mechanism just landed in CPE-1570.

## Scope
- Add a JSON tree view: either a new `kind: "json-tree"` or a view-mode toggle on the existing `json` kind in
  `src/lib/preview/provider.ts`. Build a `JsonTree.svelte` (pure-JS, no new dep — `JSON.parse` + a small recursive
  tree component) rendering collapsible objects/arrays with types.
- Declare JSON `actions` on the provider (using the `PreviewAction`/`PreviewActionCtx` API from CPE-1570):
  **Copy value**, **Copy path** (for the focused/selected node), **Format** (pretty-print/normalize), **Collapse all /
  Expand all**, **Validate** (report parse errors clearly). Labels via `$t()`, Icon glyphs, theme-only colors (MENUS.md).
- Large-JSON safety: cap/virtualize or lazy-expand deep/huge trees so a giant file can't stall the pane (STREAMING.md spirit).

## Acceptance criteria
- A `.json` file shows the collapsible tree; toggling nodes works; the pane doesn't freeze on a large file.
- Copy value/path copy the correct data; Format normalizes; Validate surfaces a clear message on malformed JSON.
- Actions appear in the CPE-1570 action bar; i18n keys in all complete locale catalogs (CPE-481 gate).
- Unit tests for the tree render + each action; component test for declare→render→run.
- `npm run check` clean; vitest green. Frontend-only, no new deps.

## Notes
Builds directly on CPE-1570's action-bar. Touches `provider.ts` + `PreviewPane.svelte` — other CPE-1568 slices touch
the same files, so this must not run concurrently with them (Foreman serializes). Model: sonnet.
