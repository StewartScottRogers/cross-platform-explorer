---
id: CPE-254
title: New file (New Text Document)
type: Feature
status: Done
priority: Low
component: Backend + Frontend
estimate: 45m
created: 2026-07-13
---

## Summary

The empty-area menu can create a New folder but not a new file — a canonical
"New > Text Document" gap. Add a **New file** action that creates an empty
`New Text Document.txt` in the current folder and drops it straight into inline
rename, exactly like New folder.

## Acceptance Criteria

- [ ] New backend command `create_file(path, name)` creates an empty file and,
      like `create_dir`, errors rather than clobbering an existing name.
- [ ] Empty-area context menu offers **New file**; the new file is selected and
      put into inline rename.
- [ ] Auto-numbered name (`New Text Document (2).txt`) when the default is taken.
- [ ] Unavailable in Home and inside the read-only archive view.
- [ ] `cargo test` and `npm run check` pass.

## Resolution

Added backend `create_file(path, name)` — mirrors `create_dir`, using
`OpenOptions::create_new` so it fails atomically rather than truncating an
existing file. Added a frontend `newFile()` that names the file
`New Text Document.txt` (auto-numbered via `uniqueNameWithExt`) and reuses the
existing `pendingRenamePath` flow to select + inline-rename it, exactly like New
folder. Wired **New file** into the empty-area context menu. Guarded out of Home
and the read-only archive view.

## Work Log
2026-07-13 — Filed and picked up during Nightshift.
2026-07-13 — Implemented backend + frontend. Verified: cargo test 61 pass (incl.
create_file empty + no-clobber), clippy clean, npm run check + vitest 241 pass.
Landed on branch cpe-254-new-file.
