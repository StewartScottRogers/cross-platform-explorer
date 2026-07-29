---
id: CPE-565
title: "Preview — word-wrap toggle for text/code/JSON (preserve indentation with scroll)"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The text/code/JSON preview always **wraps** long lines (`white-space: pre-wrap`), which mangles code
indentation and wide log lines. Add a toggle to switch wrapping **off** — preserving structure with a
horizontal scroll — remembered across files.

## Acceptance Criteria
- [x] A wrap toggle in the preview edit-bar for `<pre>`-rendered previews (text/code + JSON); not shown
      for images/PDF/markdown/tables where it doesn't apply.
- [x] Default stays **wrapped** (current behaviour); toggling off sets `white-space: pre` +
      `overflow-x: auto` so indentation is preserved and long lines scroll.
- [x] The choice persists across files/sessions (localStorage).
- [x] `npm run check` clean; a component test covers the toggle + persistence.

## Resolution
`PreviewPane` gained `wrapLines` (from `localStorage cpe.previewWrap`, **default on** — only an explicit
`"0"` disables) + `toggleWrap()` that persists, and `$: isPreText` (kind `json` | `text`). An icon-only
`↩` toggle (`aria-label="Wrap long lines"`, `aria-pressed`) sits in the edit-bar for pre-text previews;
the two `.preview-text` `<pre>`s get `class:nowrap={!wrapLines}` → `.preview-text.nowrap { white-space:
pre; word-break: normal; overflow-x: auto; }`. Icon-only avoids shipping an untranslated label. Component
test: a text preview wraps by default, the toggle adds `nowrap` + persists `"0"`. Full suite **601 pass /
62 files**; `npm run check` 0/0. (Ran the full suite before landing, per the CPE-563 lesson.)
