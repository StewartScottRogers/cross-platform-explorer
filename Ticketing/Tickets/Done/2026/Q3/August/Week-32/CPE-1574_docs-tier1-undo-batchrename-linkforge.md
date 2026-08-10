---
id: CPE-1574
title: "Docs Tier-1 batch A: Global Undo + Batch Rename + Link Forge pages"
type: Task
status: Done
priority: High
component: Frontend
epic: CPE-1569
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1569 slice 3 — three of the highest-value MISSING doc pages (shipped features with zero documentation, per
the audit). Uses the IA categories + guard test from CPE-1571.

## Scope — write three new `src/docs/*.md` pages, each to the 10-point quality rubric
1. **Global Undo** (Ctrl+Z across copy/move/rename/delete-to-trash) — `App.svelte` `undo()`. Explain the undo stack,
   what's undoable, limits. Category: Safety & Recovery.
2. **Batch Rename** — `BatchRenameDialog.svelte`: all 4 modes (Find/Replace, Add prefix/suffix, Number sequence, Change
   case) with each mode's options + a worked example. Category: Organizing & Tagging.
3. **Link Forge / New Link…** — `NewLinkDialog.svelte`: symlink vs hardlink vs junction, per-OS behavior, when to use
   each. Category: Power Tools.

Each page must follow the rubric: what-it-is · when-to-use · how-to-open (menu+palette+shortcut) · every option ·
every action+shortcut · ≥1 worked example · cross-links · a "Limits/notes" closing section · safety framing for the
destructive/irreversible bits. **Verify against the actual components**, not epic titles (the audit found stale epic
work-logs). Use category-prefixed slugs for new files (docs.ts sorts by frontmatter `order`, not filename).

## Acceptance criteria
- Three new pages exist, categorized, appearing in `DocsView`; cross-links resolve.
- Content is accurate to the components (open them); no padding.
- **Shrink `docs.coverage.test.ts`'s allowlist** by removing `BatchRenameDialog` and `NewLinkDialog` (now documented) —
  the guard should now REQUIRE them to stay documented. (Undo isn't a `*Dialog`, so no allowlist change for it.)
- `sectionDocs.test.ts` green (add a Section→slug entry only if any of these maps to a real sidebar Section — likely
  none; if F1 context help should reach them, note as a follow-up per CPE-1571's deferred registry item).
- `npm run check` clean; vitest green (incl. the coverage guard with the shrunk allowlist).

## Notes
Docs-only + the guard allowlist edit — disjoint from backend/preview work. See Library
`docs-completeness-audit-2026-08-10`. Model: sonnet.
