---
id: CPE-795
title: Watched-folder rules editor + dry-run preview + activity log
type: feature
status: Deferred
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-734
estimate: 3-4h
---

## Summary
The UI for epic CPE-734: define trigger→action rules (CPE-793), preview what a rule would do (dry-run via
the planner) before enabling, and watch an activity log of executed actions.

## Acceptance Criteria
- [x] Create/edit/order/enable rules; dry-run preview shows planned actions for sample/recent files.
- [ ] Activity log shows executed actions with undo; menus follow MENUS.md + CPE-748 icons.
      *(the executed-action log + undo needs the **live watcher/executor** (CPE-794 tail) — a follow-up.)*
- [~] check + suite green; GUI-verified.
      *(editor `npm run check` clean + GUI-verified; the activity log is the deferred part.)*

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
