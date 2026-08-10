---
id: CPE-1582
title: "Docs: keyboard-shortcut reference page (every action + what it does)"
type: Task
status: Done
priority: Medium
component: Frontend
epic: CPE-1569
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1569 slice 5. The audit found `36-keyboard-shortcuts.md` documents only the *rebind dialog* — there is no
page listing what each shortcut actually DOES. Add a real reference.

## Scope
- New `src/docs/*.md` reference page (category-prefixed slug, e.g. `input-keyboard-reference.md`, category
  "Appearance & Input", `categoryOrder` matching `35/36/37`) listing **every** shortcut from
  `src/lib/shortcuts.ts` (`SHORTCUT_GROUPS`) + `src/lib/keymap.ts` (`ACTIONS`), grouped as the in-app cheat sheet
  groups them (Navigation / Tabs / Selection / File actions / View / General / Macros), each row = chord + what it does.
- Note that these are remappable (link to `36-keyboard-shortcuts.md`, the rebind UI page) and that Navigation Mode
  (vim) has its own motions (link to `37-navigation-mode.md`).
- Add to `src/docs/00-index.md`. Verify against `shortcuts.ts`/`keymap.ts` (accuracy — actual chords + labels).

## Acceptance criteria
- Page lists every action/chord accurately (cross-checked against `shortcuts.ts` + `keymap.ts`); grouped; cross-linked
  to the rebind page + navigation mode.
- Appears in `DocsView` under Appearance & Input; `sectionDocs.test.ts` + `docs.coverage.test.ts` + `docs.test.ts` green.
- `npm run check` clean. Docs-only, no new deps.

## Notes
36-* stays the rebind-UI page; this is the companion *reference*. Verify chords against the code (glyphs like ← are
displayed but the real key is ArrowLeft — describe the user-facing key). Model: sonnet.
