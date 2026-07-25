---
id: CPE-795
title: Watched-folder rules editor + dry-run preview + activity log
type: feature
status: Done
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-734
estimate: 3-4h
---

## Summary
The UI for epic CPE-734: define trigger→action rules (CPE-793), preview what a rule would do (dry-run via
the planner) before enabling, and watch an activity log of executed actions.

## Acceptance Criteria
- [x] Create/edit/order/enable rules; dry-run preview shows planned actions for sample/recent files.
- [x] Activity log shows executed actions with undo; menus follow MENUS.md + CPE-748 icons.
      *(landed with the CPE-794 tail — the live watcher fills the dialog's activity log with `WatchFire`
      records, each with an **Undo** button; the dialog uses inline theme-only buttons, no popup menus.)*
- [x] check + suite green; GUI-verified.

## Notes
Prereq: CPE-793, CPE-794. Attended GUI.

## Resolution (editor + dry-run shipped + verified; activity log deferred)
Built `src/lib/components/WatchRulesDialog.svelte` over the tested `watchRules` store: create a rule
(name + CPE-774 condition builder across all kinds + an action pipeline — move/copy/tag/rename, added as
chips), enable/disable, **reorder** (new `moveRule` added to `watchRules.ts` + unit test), delete, and a
live **dry-run** that runs a sample filename through `planForEntry` (CPE-793) to preview the first matching
rule's resolved actions. Rules persist via settings (`cpe.watchRules`, tolerant load); opened from the
command palette ("Watch rules…", all 12 locales).

**GUI-verified in the running dev app (CDP):** added a rule (ext `pdf` → move to `/archive`) → it listed as
`Archive PDFs when .pdf move → /archive`; dry-run of `invoice.pdf` → **"→ Archive PDFs: /archive"**, of
`photo.jpg` → **"no rule matches"**; disabling the rule → dry-run "no rule matches" (planner skips disabled),
re-enabling restored it; **Done + full app reload** kept the rule (persistence). Test rule cleaned up.
`npm run check` clean; watchRules 6 tests (incl. the new `moveRule`).

Deferred tail (AC2): the **activity log of executed actions + undo** needs the live `notify` watcher/executor
(the CPE-794 tail) actually running rules — build it alongside that. No external gate.

## Update — activity log + undo landed (2026-07-20), closing AC2
The deferred tail shipped with the CPE-794 completions (live watcher #64/#65 + reversibility #67):
- The live watcher drives `folderWatch.ts`, which records each executed rule as a structured **`WatchFire`**
  and pushes it to `App.watchLog`; `WatchRulesDialog` renders the recent activity log (`watch-log` /
  `watch-log-line`) with each fire's summary **and an Undo button** that dispatches `undo` →
  `App.undoWatchFire` → `undoFire` (moves the file back / deletes copies). All buttons are inline,
  theme-variable-styled controls (no popup menus in this dialog, so MENUS.md's popup rules don't apply).
- Verified: `folderWatch.test.ts` (8 tests incl. `undoPlan` + the reversible `WatchFire`), and the undo
  round-trip **GUI-verified end-to-end in the sidecar dev build (CDP)** — a dropped file moved by a rule was
  returned to its source by the real frontend `undoFire` (`srcExists:true`, `archExists:false`). `npm run
  check` clean; full suite green.

All ACs now met; the deferred blocker (the CPE-794 live watcher + undo) has cleared and its work is on main.
CPE-795 → Done.
