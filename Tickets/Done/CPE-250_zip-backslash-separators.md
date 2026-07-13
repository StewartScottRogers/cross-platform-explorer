---
id: CPE-250
title: Zip browsing doesn't group folders when entry names use backslash separators
type: Bug
status: Done
priority: Medium
component: Multiple
estimate: 20m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

The zip browser (CPE-242) groups inner folders by splitting entry names on `/`.
Some zips (e.g. PowerShell `Compress-Archive`) store entry names with `\`
separators, so those appear as flat files (`src\main.js`) instead of a `src`
folder. Normalise separators so folders group regardless of the archive's
separator, and make extraction tolerant of either separator.

## Acceptance Criteria
- [ ] A zip whose entries use `\` groups into folders correctly.
- [ ] A zip whose entries use `/` still works.
- [ ] Opening a file inside such a zip extracts + opens it (backend tries both separators).
- [ ] `npm run check` + `cargo build` pass.

## Resolution

## Work Log

### filled
Frontend archiveChildren normalises `\` → `/` before deriving folders, so
backslash-separated zips (Compress-Archive) group correctly. Backend
extract_archive_entry resolves the entry by index, trying the given name then a
backslash variant. check 0/0; cargo build ok. Ships 0.10.5.
