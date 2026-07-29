---
id: CPE-776
title: Rules editor UI + persistence for file coloring & labels
type: feature
status: Done
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-709
estimate: 3-4h
---

## Summary
The user-facing rules editor for epic CPE-709: an ordered list of rules (condition → color/label), add /
edit / remove / enable-disable / reorder, with a live preview, persisted as a global rule set in settings.

## Acceptance Criteria
- [x] Users create/edit/remove/reorder/enable rules across the CPE-774 condition kinds; live preview.
- [x] The rule set persists in settings and reloads on startup; menus follow MENUS.md + CPE-748 icons.
- [x] `npm run check` + suite green; GUI-verified.

## Notes
Prereq: CPE-774, CPE-775. Attended GUI. Persistence via the existing settings module.

## Work Log
- 2026-07-20 (nightshift) — Picked up. The evaluation engine (`colorRules.ts`, CPE-774) already resolves a
  row's style (`evaluateRules` — first enabled matching rule wins), and its header comment scopes CPE-776
  as "a thin CRUD". So the missing *logic* is the ordered rule-list store — CRUD + reorder + tolerant
  persistence — which the editor dialog and the settings layer both consume. Built that headlessly; the
  dialog + live preview are attended GUI.
- 2026-07-20 (nightshift) — Added `src/lib/colorRulesStore.ts`: `addRule` / `updateRule` / `removeRule` /
  `toggleRule` (explicit or flip) / `moveRule` (reorder one step, clamped at the ends — this is how a user
  changes rule precedence, which is meaningful since `evaluateRules` takes the first match) /
  `serializeRules` / `parseRules` (tolerant — drops entries lacking an `id` or a known-kind `when`, so a
  bad persisted rule can't break startup). All immutable, no DOM/IO. Mirrors the job-list store in
  `backup.ts`. 7 vitest cases (add/update/remove/toggle, reorder incl. end-clamp + unknown-id no-op +
  new-array identity, round-trip, null/garbage/malformed-drop). `npm run check` clean.
- 2026-07-20 (nightshift) — **Deferred.** The rule-list logic (CRUD/reorder/enable/persist) is complete and
  headlessly green; the remaining scope is the **rules editor dialog** — an ordered list with add/edit/
  remove/reorder/enable controls (MENUS.md + CPE-748 icons), a live preview using `evaluateRules`, and
  wiring `serializeRules`/`parseRules` to the settings module (load on startup). Needs the running app for
  AC3 "GUI-verified".
  - *deferred-on:* the attended editor dialog + its GUI verification (this ticket is tagged "Attended GUI").
  - *revisit-when:* an attended session — build the dialog over `colorRulesStore` + `evaluateRules`, wire
    persistence through the settings module, add any new `Section`→doc entry (CPE-579 guard) if it becomes
    its own section, and GUI-verify. No external gate; pickable anytime.

- 2026-07-20 (attended GUI, dev-app verify) — Built the editor + wired the whole feature, then verified
  live in the running dev app via CDP:
  - `src/lib/components/ColorRulesDialog.svelte` — a thin CRUD over `colorRulesStore` (add across all six
    condition kinds via a kind picker + kind-specific inputs; per-rule colour, label, enable toggle, up/down
    reorder, delete). Emits `change` live (preview), `save` (persist), `cancel` (revert).
  - `FileList.svelte` applies `evaluateRules` per row → tints the name (and shows a rule label pill),
    threaded App → `ExplorerPane` → `FileList` via a new `colorRules` prop.
  - `settings.ts` gains `loadColorRules`/`saveColorRules` (KEY `cpe.colorRules`) reusing the tolerant
    `parseRules`; App loads at startup and saves on Done.
  - Opened from the command palette ("Color rules…", `tool.colorRules`); `palette.colorRules` added to all
    12 locales (i18n 100%-coverage gate).
  - **Live verification (CDP):** opened the editor from the palette → added an `isDir` rule → all 36 folder
    rows rendered `color: #e2504b` (the rule colour); toggling the rule off reverted them, on restored them;
    clicking Done then **reloading the whole app** and re-navigating kept the folders coloured (persistence
    across restart). Cleaned up the test rule afterward.
  - While verifying, found + fixed an unrelated pre-existing crash (diagnostics overlay duplicate `{#each}`
    key) that was aborting Svelte's flush with diagnostics on — split out as **CPE-809**.
  - `npm run check` clean; full vitest suite 862 green.

## Resolution
Shipped the full rule-based coloring/labels feature. The pure store (`colorRulesStore.ts`, 8 tests) + the
existing `evaluateRules` engine back a new editor dialog (`ColorRulesDialog.svelte`) opened from the command
palette; `FileList` applies `evaluateRules` to tint each row's name and show a rule label; the rule set
persists via `settings.ts` (`cpe.colorRules`, tolerant load) and reloads on startup. GUI-verified end-to-end
in the running dev app (add rule → live coloring → enable/disable → persistence across a full reload).
`npm run check` + 862-test suite green. (Found + fixed a blocking pre-existing diagnostics crash en route —
CPE-809.)
