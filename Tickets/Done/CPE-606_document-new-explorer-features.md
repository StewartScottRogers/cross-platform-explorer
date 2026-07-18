---
id: CPE-606
title: Document the command palette + find-by-name in the in-app docs
type: docs
component: docs
priority: low
status: Done
tags: ready
estimate: 15m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Per CLAUDE.md ("in-app docs are self-maintaining"), the new user-facing explorer features from this
Nightshift — the command palette (CPE-602/605), Find files by name (CPE-603), and recent locations
(CPE-604) — must be reflected in the built-in Documents library.

## Acceptance Criteria
- [x] `src/docs/03-explorer.md` describes Find files by name (Ctrl+P) and the three-way Search story.
- [x] It gains a "Command palette" section (Ctrl+Shift+P, greyed-out actions, recent locations).
- [x] Docs tests (`sectionDocs`) still pass; no new section/slug needed (content-only edit).

## Resolution
Expanded the Search bullet in `03-explorer.md` into name-filter / find-by-name / search-in-files, and
added a "Command palette" section. No `sectionDocs` change needed — these are features within the
existing Explorer section, not new sections.

## Work Log
2026-07-17 (Nightshift Loop 5) — Updated in-app docs to cover the four features shipped this shift.
