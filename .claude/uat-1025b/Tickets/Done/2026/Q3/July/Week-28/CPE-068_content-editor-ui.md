---
id: CPE-068
title: Content editor in the preview pane (Edit when available, save back to file)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Turn the read-only content viewer into a "view, with the option to edit if available." For editable
file types (per [[CPE-067]]), show an **Edit** control; entering edit mode replaces the rendered view
with an editable textarea of the raw text; **Save** persists via `write_file_text` ([[CPE-066]]).
Non-editable types show no Edit affordance (viewer only).

## Acceptance Criteria

- [ ] An "Edit" button appears in the preview pane only when the selected file is editable
- [ ] Edit mode shows the raw text in a textarea (source text for md/json/csv, not the rendered form)
- [ ] Save writes via `write_file_text`, then refreshes; Cancel discards and returns to the viewer
- [ ] Dirty state: Save is disabled when unchanged; Ctrl+S saves while editing
- [ ] Switching the selected file leaves edit mode (and does not silently save)
- [ ] Save is injected into `PreviewPane` as a prop (mockable), like `loadText`/`loadEntries`
- [ ] jsdom tests: Edit shows a textarea for editable types / is absent for non-editable; Save calls the backend with the edited text
- [ ] `npm run check` clean; suite green; `vite build` clean

## Resolution

`PreviewPane` now shows an **Edit** button for editable files; edit mode swaps the rendered view for a
`<textarea>` bound to the raw text with **Save**/**Cancel** and Ctrl/Cmd+S. Save is disabled until
dirty, persists via an injected `saveText` prop (wired in `App` to `write_file_text`, then refreshes the
listing), and changing the selected file exits edit mode without saving. Non-editable types show no Edit
affordance. 2 jsdom tests (Edit present for text / absent for image; edit→Save calls backend with the
new text). `npm run check` clean; suite 187 passed; `vite build` clean.

## Work Log

2026-07-11 — Part of the content-editor set. Viewer already exists (CPE-060/065); this adds the edit affordance + textarea + save wiring.

## Notes

Keep it a simple raw-text editor (no per-format structured editing). Relates to [[CPE-066]] [[CPE-067]].
