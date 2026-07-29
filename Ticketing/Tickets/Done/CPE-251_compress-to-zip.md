---
id: CPE-251
title: Compress selection to a ZIP archive
type: Feature
status: Done
priority: Medium
component: Backend + Frontend
estimate: 2h
created: 2026-07-13
---

## Summary

The app can *browse* and extract single entries from zip/tar/7z/etc. archives
(CPE-064/242), but it cannot **create** an archive. "Compress to ZIP" is a
canonical file-explorer action. Add a right-click "Compress to ZIP" that packs
the selected files/folders (recursively) into a new `.zip` in the current folder.

ZIP is the universal, cross-platform choice (already a dependency), so it fits
the app's fast/small/predictable tiebreaker without adding a new format lib.

## Acceptance Criteria

- [ ] New backend command `compress_to_zip(paths, dest)` writes a deflated `.zip`
      containing the given files and folders; folders are added recursively and
      empty directories survive.
- [ ] Right-click one or more real entries offers **Compress to ZIP**.
- [ ] The archive is named after the single selected item's stem (`report.zip`),
      or `Archive.zip` for a multi-selection, auto-numbered to avoid collisions.
- [ ] After creation the listing refreshes and the new zip is selected.
- [ ] Action is unavailable in Home and inside the read-only archive view.
- [ ] Round-trip verified: compress then read_archive_entries lists the contents.
- [ ] `cargo test` (backend) and `npm run check` pass.

## Resolution

Added backend `compress_to_zip(paths, dest)` — a deflated ZIP writer that adds
each selection at the archive root and recurses into folders (explicit directory
entries so empty folders survive; children sorted for reproducible order). Wired
a **Compress to ZIP** context-menu action: names the archive after the single
item's stem or `Archive.zip` for a multi-selection, auto-numbered before the
extension via new `uniqueNameWithExt` (so `report (2).zip`, not `report.zip (2)`),
then selects the new zip. Guarded out of Home and the read-only archive view.

## Work Log
2026-07-13 — Filed and picked up during Nightshift. Companion to [[CPE-252]] (extract).
2026-07-13 — Implemented backend + frontend. Verified: cargo test (round-trip
compress→extract with nested folders), naming unit tests, `npm run check`,
full vitest (241) + cargo test (59) green, clippy clean. Landed on branch
cpe-251-252-archive-ops.
