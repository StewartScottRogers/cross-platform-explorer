---
id: CPE-224
title: Preview Pane context menu + text selection for cut/copy/paste (view and edit)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

The Preview Pane should support a right-click **context menu** with **Cut / Copy / Paste / Select All**
for its text content, working in **both view and edit modes**. Text must be selectable ("highlight")
in both. The menu is identical in both modes — items are only *disabled* when they don't apply: in
read-only view, **Cut** and **Paste** are disabled (Copy and Select All stay enabled); in edit mode all
four are enabled.

## Acceptance Criteria

- [ ] Right-clicking text content (text/code/markdown/json/csv/tsv) opens a context menu with Cut, Copy, Paste, Select All (same items both modes)
- [ ] View (read-only): Cut and Paste disabled; Copy and Select All enabled
- [ ] Edit (textarea): all four enabled and functional (cut/paste mutate the draft at the caret/selection)
- [ ] Copy uses the selected text (or all content if nothing is selected); Paste inserts at the caret
- [ ] Text is selectable in both modes (no user-select suppression outside of an active drag)
- [ ] The app's global Ctrl+C/Ctrl+X do NOT hijack a text copy/cut when there is an active selection / the caret is in the editor
- [ ] Menu closes on action, outside click, or Escape; stays on screen
- [ ] jsdom tests: menu opens; enabled/disabled per mode; Copy calls the clipboard; edit cut/paste mutate the draft
- [ ] npm run check clean; suite green; build clean

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-12 — Requested: consistent context menu + selection/cut/copy/paste in the Preview Pane across view and edit; sub-items disabled rather than hidden when N/A.

## Notes

Only applies to text-based content; image/pdf/media/archive previews don't get the text menu. Uses
navigator.clipboard (works in the Tauri webview's secure context).

## Resolution

Added a text context menu to the Preview Pane (Cut/Copy/Paste/Select all) shown on right-click of text content, same items in view and edit — Cut/Paste disabled in read-only view, all enabled in edit. Copy/Cut/Paste use navigator.clipboard; edit cut/paste mutate the draft at the caret and restore the cursor; Select all uses textarea.select() (edit) or a Range over the content (view). App no longer hijacks Ctrl+C when a text selection is active. Menu closes on action/outside-click/Escape. 3 jsdom tests. check + suite (203) green; build clean.
