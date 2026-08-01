---
id: CPE-1186
title: "Archive extract polish: orphan dest dir on password-cancel + precise retry errors"
type: chore
component: Multiple
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
Two non-blocking findings from the CPE-1182/1183 review (PR #497):

1. **Orphaned empty dest dir on cancel.** `doExtract`'s password fallback runs plain `extract_archive` first,
   and the backend (`crates/server/src/archive.rs` ~`fs::create_dir_all(dest)`) creates the auto-numbered dest
   dir *before* the encryption error surfaces. If the user then cancels the password prompt, an empty
   `<name>/` folder is left behind. Harmless but untidy. (double-click/extract-to paths are already clean.)
2. **Imprecise retry error.** `promptForExtractPassword`'s bare `catch {}` labels every retry failure
   "Wrong password — try again." A non-password failure (disk full mid-extract, or an encrypted **7z**
   mis-routed to the zip-only decrypt) would show a misleading message.

## Build
- Defer `create_dir_all(dest)` until the first successful entry write (backend), OR have the frontend remove the
  empty dest dir when the password prompt is cancelled.
- Gate the encrypted retry path to `.zip` (encrypted-7z is out of scope), and distinguish password vs. other
  errors on retry so a non-password failure shows its real message.

## Acceptance Criteria
- [ ] Cancelling the password prompt leaves no empty dest folder.
- [ ] A non-password extract failure on retry shows its real error, not "Wrong password".
- [ ] `npm run check` + `npm test` + `cargo test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift) from the PR #497 reviewer's two non-blocking findings.
