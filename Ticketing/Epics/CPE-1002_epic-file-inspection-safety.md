---
id: CPE-1002
title: "EPIC: File inspection & safety utilities"
type: Task
status: In Progress
priority: Medium
component: Backend
tags: [epic]
created: 2026-07-24
closed:
---

> **Filed + activated 2026-07-24** (workshift, Foreman; ideas from a research sweep — each grep-verified
> non-duplicative against the 78 `cpe-server` modules + 39 epics). An umbrella for small, **pure,
> headless** file-inspection detectors — deterministic algorithms over already-gathered bytes/metadata, no
> GUI logic and no live AI model in the core, no new deps. Each ships its detector; the column/badge/review
> UI is attended.

## Goal
A suite of pure detectors that surface useful facts + hazards about files the explorer doesn't show today:
the true text **encoding + line endings** of a file, **archive expansion ratios** (zip-bomb warning),
**empty-folder** cascades, **orphaned sidecar** files, and **dangling symlinks**. Each is a small,
cargo/vitest-testable core the UI later surfaces as a column, badge, or cleanup view.

## Why
The explorer previews and organises, but doesn't inspect for these common real-world issues (a Latin-1 file
mangled on save, an archive that balloons 1000× on extract, folders full of nothing, `.srt`/`.xmp` sidecars
whose media was deleted, symlinks pointing nowhere). All are pure algorithms over metadata/bytes — ideal
headless slices, and genuinely useful for a cross-platform explorer.

## Child tickets
1. **CPE-1003** ✅ — Text **encoding + line-ending** detector (`cpe-server::text_encoding`). Done +
   independently reviewed (QA gate caught + fixed a NUL/BOM-less-UTF-16 misclassification, re-reviewed).
   PR #325.
2. **CPE-1004** ✅ — Archive **expansion-ratio / zip-bomb** score (`cpe-server::archive_safety`): pure
   per-entry `expansion_ratio(entries, limits) -> RatioReport`, divide-by-zero-safe. Done + reviewed. PR #324.
3. **CPE-1005** ✅ — **Empty-folder** cascade finder (`cpe-server::empty_dirs`): `cascade_empty(tree)` →
   topmost cascade-empty dirs. Done + reviewed. PR #326.
4. **CPE-1006** ✅ — **Orphaned-sidecar** detector (`cpe-server::orphan_sidecars`): `find_orphans(entries,
   rules)` → sidecars whose primary is gone. Done + reviewed. PR #327.
5. **CPE-1007** ✅ — **Near-identical-folder** detection (`cpe-server::folder_similarity`): `jaccard` +
   `cluster_similar_folders(folders, threshold)` (union-find over file-hash-set overlap). Done + reviewed. PR #329.
6. **CPE-1008** ✅ — **Dangling / cyclic symlink** classifier (`cpe-server::dangling_links`):
   `scan_dangling(links)` → Missing / Cyclic (bounded cycle-walk, Cyclic precedence). Done + reviewed. PR #330.
7. **UI** — columns / warning badges / cleanup-review surfaces for the above. **GUI/attended.**

**Status (2026-07-24):** all six pure detectors from the research sweep are **done + independently reviewed**
(the QA gate caught + fixed 2 real bugs en route: CPE-1003 NUL-as-UTF-8, CPE-1001-style edge in the sibling
epic). The research shortlist is **exhausted** — further detectors would need another research pass. What
remains is the **attended UI** (columns/badges/cleanup views) that surfaces these engines.

## Definition of Done
- Each detector is a pure, cargo/vitest-tested `cpe-server`/`src/lib` function with no new deps and no cost
  when unused; the UI surfaces them (attended).

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** The DoD's 'the UI surfaces them' clause unbuilt (all 6 pure detectors shipped).
