---
id: CPE-1322
title: File-Health mismatch tab — "rename to correct extension" fix-it action
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-08-05
epic: CPE-1000
estimate: 2h
---

## Summary
The File-Health type-mismatch tab (CPE-1316) SHOWS files whose extension lies (`MismatchHit { path,
claimed_ext, detected_label, detected_ext }`). Add a per-row **fix-it action** to RENAME the file to its
`detected_ext` (e.g. `foo.jpg` that's really an exe → rename to `foo.exe`), so the user can fix the disguised
files, not just see them. Completes the CPE-1000 "mismatch review + rename-to-correct-extension" remainder.

## Scope
- Investigate the existing rename backend (grep `move_exact` / rename commands / how BatchRenameDialog /
  in-place rename call it) — reuse it, don't add a backend command.
- On each mismatch row, add a small "Rename to .{detected_ext}" button/action (only when `detected_ext` is
  present + differs from `claimed_ext`). On click: rename `path` → same dir + same stem + `.{detected_ext}`,
  via the existing rename/move command (busy-cursor `invoke`). On success, remove that row from the results
  (it's fixed) and optionally show a brief confirmation; on failure, surface the error (no silent failure).
- Guard against clobbering an existing file at the target name (the rename backend likely already refuses;
  surface that as an error rather than overwrite).

## Acceptance Criteria
- [ ] Mismatch rows show a "Rename to .{ext}" action; clicking it renames the file to the detected extension
      and removes the fixed row. Failure (e.g. target exists / permission) surfaces a visible error, never a
      silent no-op or an overwrite.
- [ ] Uses the existing rename/move backend (no new backend command); busy-cursor `invoke`.
- [ ] UI conventions: button uses theme vars + an existing Icon glyph; row layout still reflows; the subtitle
      layout from CPE-1319/1321 isn't broken by the added control.
- [ ] jsdom test: click rename → correct command+args (path → detected-ext target), row removed on success,
      error surfaced on failure. Falsifiable. i18n new key × 12 locales.
- [ ] `npm run check` clean + full `npm run test:unit` green. (Foreman batches a screenshot if the row layout
      changed materially.)

## Work Log
2026-08-05 (workshift run 2) — Filed by the Foreman as the natural fix-it extension of the mismatch tab.
