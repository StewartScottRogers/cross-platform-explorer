---
id: CPE-776
title: Rules editor UI + persistence for file coloring & labels
type: feature
status: Deferred
priority: low
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-709
estimate: 3-4h
---

## Summary
The user-facing rules editor for epic CPE-709: an ordered list of rules (condition → color/label), add /
edit / remove / enable-disable / reorder, with a live preview, persisted as a global rule set in settings.

## Acceptance Criteria
- [~] Users create/edit/remove/reorder/enable rules across the CPE-774 condition kinds; live preview.
      *(**CRUD + reorder + enable core landed & tested** — `colorRulesStore.ts`; the editor dialog + live preview are the attended GUI tail.)*
- [~] The rule set persists in settings and reloads on startup; menus follow MENUS.md + CPE-748 icons.
      *(**serialize/parse core landed & tested** (tolerant); wiring to the settings module + reload-on-startup is the GUI tail.)*
- [~] `npm run check` + suite green; GUI-verified.
      *(`npm run check` clean + vitest 7/7 now; **GUI-verified** is the attended part.)*

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

## Resolution (partial — store core landed, editor UI deferred)
Landed `src/lib/colorRulesStore.ts` — the pure ordered-rule-list store (immutable CRUD, precedence reorder
with end-clamping, enable-toggle, and tolerant serialize/parse) that the rules editor and settings
persistence are thin layers over. With it and the existing `evaluateRules` engine, the entire CPE-776
logic path is complete and unit-tested (7 cases); only the attended editor dialog + live preview + settings
wiring + GUI verification remain. Deferred with a turnkey revisit note.
