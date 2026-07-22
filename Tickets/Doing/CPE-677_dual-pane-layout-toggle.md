---
id: CPE-677
title: Dual-pane layout toggle + split view
type: feature
component: Frontend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-617
estimate: 3-4h
---

## Summary
Child of CPE-617. A toggle (View menu / palette) that splits the window into two `<ExplorerPane>`
instances side by side, with a visible active-pane indicator and Tab to switch focus; persist the layout
choice. Preview pane is hidden in dual mode for v1. Prereq: CPE-676.

## Acceptance Criteria
- [x] A toggle switches single ⇄ dual pane; OFF by default; single-pane unchanged. *(palette `view.dualPane`; additive — pane A markup untouched)*
- [x] Two independent panes render side by side; active pane is clearly indicated; Tab switches focus. *(second ExplorerPane; accent ring + click/Tab activation; pane B navigates independently)*
- [x] Layout choice persists across sessions; `npm run check` + suite green. *(dualPane+paneBPath persisted; check 0/0, i18n+settings 51/51)*
- [ ] **GUI verification** (side-by-side render, independent nav, focus ring) — pending next loop.

### Follow-ups (filed, not blocking v1)
- i18n: the `view.dualPane` palette label is a hardcoded English string for v1 (adding 2 keys ×12 complete
  locales was disproportionate for the loop). i18n-ify in a follow-up.
- Pane B v1 is navigate/select/open only (no DnD/context-menu/per-pane search) → CPE-678/679. Tab moves the
  active-pane ring; full DOM focus transfer into pane B's list is a refinement.

## Work Log
- 2026-07-22 (nightshift loop 2) — **Implementation plan + persistence slice landed on branch.**
  Design (additive, keeps single-pane markup untouched → satisfies "single-pane unchanged"):
  - **Layout:** reuse the existing preview grid-slot for pane B. When `dualPane` is on,
    `effectiveGridCols = "{sidebar}px 6px 1fr 6px 1fr"` and the preview column is suppressed; the second
    `.pane-col` holds a second `<ExplorerPane>`.
  - **Pane B state:** `explorerPaneB` (`bind:this`) + its own `entriesB/visibleB/shownB/loadingB/errorB/
    selectionB/selectedEntriesB`; config props (view/sort/showHidden/colorRules/places/drives/…) shared
    with pane A for v1.
  - **Pane B nav:** `navigateB(path)` → `explorerPaneB.loadListing(path,true)` + persist `paneBPath`;
    `openB(entry)` → dir ⇒ `navigateB`, file ⇒ `open_external` + recents (mirrors `open()`).
  - **Focus:** `activePane: 0|1`, click-to-activate + Tab-to-switch; active pane gets a CSS ring.
  - **Toggle:** palette command `view.dualPane` → `toggleDualPane()` (persists; inits pane B path on
    first enable). Assumption (logged): v1 pane B is navigate/select/open only — DnD, context menu, and
    per-pane keybindings are CPE-678/679; pane B shares pane A's view/sort/search config.
  - This commit: `settings.ts` persistence (`dualPane` + `paneBPath` keys, load/save, `isString`
    validator). `npm run check` = 0 errors/0 warnings. UI wiring lands next loop, then GUI-verify + merge.
