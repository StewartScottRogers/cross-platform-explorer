---
id: CPE-1234
title: "Saved-search view: preview pane shows the Home icon instead of a search/folder glyph"
type: Bug
priority: Medium
component: frontend
tags: [ready]
created: 2026-08-01
epic: CPE-978
closed:
---

## Problem
Caught by the CPE-1233 Visual Critic pass. When a structured saved search (CPE-1229) is open with NO
file selected, the preview pane's placeholder renders the large orange **Home** glyph above
"<name> (N items)" — the same icon used for the Explore→Home entry. It contradicts the breadcrumb
("Home › <name>"), the search box, and the status bar ("Saved search …"), which all correctly say the
user is inside a saved search. The placeholder graphic should be a saved-search/magnifying-glass icon
(or a generic folder glyph), NOT Home's icon.

## Fix
In the preview-pane "no selection" placeholder path (grep `PreviewPane.svelte` + how App.svelte feeds
it the current-view icon/label), when the active view is a structured saved search (or a tag smart
folder — check that one too), use the `search` icon (the same glyph the sidebar "Saved Searches"
section uses) rather than reusing Home's icon. Keep the "<name> (N items)" text.

## Acceptance criteria
- Opening a saved search with nothing selected shows a search/folder placeholder icon, not the Home icon.
- Verify the tag-only smart folder placeholder is also correct (not Home).
- Re-capture the saved-search gui-smoke screenshot; Visual Critic PASS.
- `npm run check` + `npm test` green.

## Notes
Product defect in CPE-1229's feature (1229 merged before its visual leg, per the deferred-pin flow).
Also noted, likely UNRELATED/pre-existing (verify, do not necessarily fix here): "Gallery" in the
Explore sidebar section renders dimmed vs its siblings — may be an intended disabled state.

## Work Log

2026-08-01 — Root cause: the no-selection placeholder hero is `DetailsPane.svelte`, not
`PreviewPane.svelte` (`PreviewPane` slots `DetailsPane` in for its `entry === null` case).
`DetailsPane`'s else-branch hard-coded `<Icon name="home" .../>` for every "nothing selected" case,
including a structured saved search and a tag smart folder.
2026-08-01 — Fix: added a `folderIcon` prop to `DetailsPane.svelte` (default `"home"`, unchanged for
Home/archive/real-folder), and a `$: folderIcon = structuredSearch ? "search" : smartFolder ?
"filter" : "home"` derivation in `App.svelte`, threaded into both `<DetailsPane>` usages (the
preview-pane slot + the Details-tab fallback). `search`/`filter` are the exact glyphs the sidebar's
"Saved Searches"/"Smart Folders" sections already use, so the placeholder now agrees with the
breadcrumb/search-box/status-bar instead of contradicting them.
2026-08-01 — Added `src/App.previewPlaceholderIcon.test.ts`: renders the real App, opens a structured
saved search and a tag smart folder in turn, and asserts the actual placeholder `<svg>` markup
carries the `search`/`filter` glyph's signature and NOT Home's distinctive `#c94f18` roof stroke; a
third case confirms Home itself is untouched. Verified non-hollow by stashing the fix and re-running
— the search/smart-folder cases fail exactly as expected against the old always-Home behavior.
`npm run check`: 0 errors/0 warnings. `npm test` (vitest run): 155 files / 1710 tests passed.
