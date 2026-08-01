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
