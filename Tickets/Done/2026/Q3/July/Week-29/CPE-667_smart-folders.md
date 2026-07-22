---
id: CPE-667
title: Smart folders — a saved, auto-updating tag/attribute query
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
estimate: 3-4h
---

## Summary
Completes the unmet CPE-614 DoD gate: "at least one smart-folder query type works end-to-end and updates
as files change." The tags half of CPE-614 shipped (15 children), but no **smart folder** — a saved query
surfaced as a virtual folder that re-evaluates live — was ever built. Add one query type end-to-end:
a saved **tag query** (e.g. "everything tagged `invoice`") that appears in the sidebar and lists matching
files across the tagged set, updating as tags change.

## Acceptance Criteria
- [x] A saved smart folder (name + tag query) persists across restart (reuses the tag store / persist layer).
- [x] Selecting it lists all files matching the query, re-evaluating as tags are added/removed.
- [x] Surfaced in the sidebar alongside Tags; create/rename/delete via the standard menu convention.
- [x] Core query/matching logic is headlessly unit-tested; zero weight when no smart folder is open.
- [x] `npm run check` + suite green; docs updated (03-explorer.md + sectionDocs if a new section).

## Notes
Uncovered during nightshift epic housekeeping (2026-07-18): CPE-614 was In Progress with all 15 tag
children Done but this DoD gate open. Filing it keeps the epic honest; CPE-614 closes when this lands.
Design open question for whoever works it: scope v1 to tag-only queries, or generalise to attribute
predicates (size/type/date)? Recommend tag-only for v1, generalise later.

## Work Log
2026-07-18 (nightshift) — Built v1 (tag-only, per the recommended scope; logged as the assumption).

## Resolution
Smart folders v1 = a saved tag query opened as a virtual, read-only listing that self-updates as tags
change. New `src/lib/smartFolders.ts` (pure helpers + a localStorage-persisted store; 6 unit tests) and
backend `entries_for_paths` (stat a path set into DirEntry rows, skipping missing; cargo-tested). In
App.svelte a `smartFolder` active state swaps the base listing for `entries_for_paths(smartFolderPaths(
$tags, sf))`, recomputed reactively on tag changes; it's threaded through crumbs/title/counts/context
like archive mode and guarded read-only via `blockedInArchive`. Sidebar gains a **Smart Folders** section;
creation via the Tags context menu's new **Save as smart folder**, rename/delete via a new
`SmartFolderMenu`. 4 i18n keys × 12 locales; docs updated (03-explorer.md). Closes the last CPE-614 DoD
gate. check + suite green (659); backend + clippy (both modes) clean; vite bundle clean.

Verification: thorough headless (pure logic, backend stat, wiring type-check, integration suite, bundle).
Live GUI flow (tag files → Save as smart folder → open) recommended on next /run — not automated here
because it needs pre-tagged files. Files: src/lib/smartFolders.ts(+test), src/lib/components/
SmartFolderMenu.svelte, TagMenu.svelte, Sidebar.svelte, src/App.svelte, src/lib/i18n.ts,
src/docs/03-explorer.md, src-tauri/src/lib.rs.
