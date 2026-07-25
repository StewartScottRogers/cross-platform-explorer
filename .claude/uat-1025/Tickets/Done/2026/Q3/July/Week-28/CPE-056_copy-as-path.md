---
id: CPE-056
title: Copy as path (Ctrl+Shift+C) copies quoted full paths to the clipboard
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Explorer's "Copy as path" (Ctrl+Shift+C) puts the selected item's full path, wrapped in double quotes,
on the OS text clipboard — handy for pasting into a terminal or dialog. The app has no equivalent. Add
it: a pure formatter, a keyboard shortcut, and a context-menu entry, writing via
`navigator.clipboard.writeText`.

## Acceptance Criteria

- [ ] `Ctrl+Shift+C` copies the selected path(s) to the clipboard, each wrapped in double quotes
- [ ] Multiple selected paths are newline-joined (Explorer behaviour)
- [ ] A "Copy as path" context-menu entry with the Ctrl+Shift+C hint
- [ ] No-op when nothing is selected; a clear notice on success and on clipboard failure
- [ ] The shortcut must NOT be swallowed by the plain Ctrl+C (copy) handler
- [ ] Pure formatter unit-tested; App integration test asserts the clipboard write; check + suite green

## Resolution

Added `formatPathsForClipboard()` in `format.ts` (each path double-quoted, newline-joined) and
`doCopyPath()` in `App.svelte` writing via `navigator.clipboard.writeText`. Bound `Ctrl+Shift+C`
ordered BEFORE the plain `Ctrl+C` so copy-as-path isn't swallowed; added a `"copy-path"` action and a
"Copy as path" context-menu entry. 3 formatter unit tests + an App integration test that mocks
`navigator.clipboard.writeText` and asserts the exact quoted text. `npm run check` 0 errors; suite 137
passed; `vite build` clean. Committed, merged to `main`, pushed. Residual unverified: the real OS
clipboard receiving the text (thin webview layer).

## Work Log

2026-07-11 — Nightshift loop: Copy as path chosen because its behaviour is headlessly verifiable (mock navigator.clipboard.writeText and assert the exact text) — only the thin OS-clipboard hand-off is unverified. Confirmed Ctrl+Shift+C is free and must be ordered before the Ctrl+C copy handler.

## Notes

Uses `navigator.clipboard.writeText` (works in the Tauri webview's secure context) rather than adding a
Tauri clipboard plugin + capability. Residual unverified layer: the real OS clipboard receiving the text.
