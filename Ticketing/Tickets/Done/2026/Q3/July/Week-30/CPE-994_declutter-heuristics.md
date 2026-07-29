---
id: CPE-994
title: Junk / clutter detection heuristics (declutter suggestions)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-979
---

## Summary
Extends the CPE-979 `organize` engine (CPE-987) with pure **clutter detection**: flag likely-junk files
(empty, installers, partial/temp downloads, backups/leftovers) as *suggestions* for the declutter view —
never auto-actioned. Workshift (Foreman) foreground slice, 2026-07-24.

## Design
- `find_clutter(entries) -> Vec<ClutterFinding{ name, reason }>` — pure metadata heuristics over the existing
  `OrganizeEntry` (no content hashing, no I/O), one finding per flagged **file** in input order.
- `ClutterReason` (with a human `label()` for the UI): `ZeroByte`, `Installer` (exe/msi/dmg/pkg/deb/rpm/
  appimage), `TempOrPartial` (part/crdownload/tmp/temp/download or `.part` suffix), `Backup` (bak / trailing
  `~` / office `~$` lock). Most-definitive reason wins; directories are never flagged.
- Exact-duplicate detection is intentionally left to `crate::duplicates` (needs content hashing); this is the
  cheap metadata pass.

## Acceptance Criteria
- [x] `find_clutter` flags each category with the right reason; a real file is not flagged; directories skipped.
- [x] Zero-byte wins over an installer extension (most-definitive first); `label()` is human.
- [x] Cargo-tested (organize now 10 tests); clippy `--all-targets -D warnings` clean both modes; no new deps.

## Notes
- Feeds the CPE-979 declutter preview (GUI/attended) alongside `plan_organize`. The opt-in AI classifier +
  apply/undo UI remain attended / user-resource.
