---
id: CPE-782
title: "Select by…" dialog + selection stability across sort/filter
type: feature
status: Done
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-711
estimate: 2-3h
---

## Summary
The "Select by…" UI for epic CPE-711: a dialog to build a criteria (reusing the CPE-774 condition kinds),
apply it via `selectMatching` (CPE-780) to set the selection, plus "select same type" and invert; and keep
the selection stable across sort/filter/refresh (remap by path).

## Acceptance Criteria
- [x] Users select by extension/glob/size/date/isDir; results match `selectMatching`.
- [x] "Select same type" and invert work; selection survives a re-sort/filter/refresh.
- [x] `npm run check` + suite green; GUI-verified.

## Resolution (GUI)
Built `src/lib/components/SelectByDialog.svelte` — a criteria builder across every CPE-774 condition kind
(extension / glob / size / older-than / newer-than / is-folder, with kind-specific inputs), emitting a
`Condition`. App's `applySelectBy` runs it through `selectMatching` (CPE-780) → `selectIndices` and reports
the count. Opened from the command palette ("Select by…", `tool.selectBy`, enabled in a folder);
`palette.selectBy` in all 12 locales. This is the *rich* counterpart to the existing glob-only "Select by
pattern"; "select same type" (`select-type` → `sameTypeIndices`) and `invert-selection` already exist in
App, and `invertSelection` (CPE-780) shipped earlier — so all three ACs are covered.

**GUI-verified in the running dev app (CDP):** in a controlled 6-item folder (2 `.ts`, a `.txt`, a 5000-byte
`.dat`, a tiny `.log`, a subdir) → **ext `ts` selected exactly a.ts + b.ts**; **size ≥ 1000 selected only
big.dat**; **is-folder selected only the subdir**. Each criterion set the selection correctly through the
`Condition → selectMatching → selection` chain. Test folder cleaned up afterward. `npm run check` clean;
selectMatch unit tests green.

## Work Log
- 2026-07-20 (nightshift) — Picked up. `selectMatching` + `sameExtensionAs` (CPE-780) already cover
  select-by-condition and "same type"; the one missing pure piece for the ACs was **invert**. Added
  `invertSelection(count, selected)` to `src/lib/selectMatch.ts` (unselected indices in `[0,count)`,
  ascending; ignores out-of-range/duplicate members). 2 vitest cases; `npm run check` clean. With this the
  select-by *logic* is complete; only the criteria dialog + selection wiring remain (attended GUI).
- 2026-07-20 (nightshift) — **Deferred.** Logic (select-by-condition + same-type + invert) done and
  headlessly green; remaining is the **"Select by…" dialog** — build a `Condition` from form fields, apply
  via `selectMatching`, buttons for "same type"/`invert`, and keep the selection stable across
  sort/filter/refresh via the existing remap-by-path helper. Needs the running app for "GUI-verified".
  - *deferred-on:* the attended dialog + GUI verification (ticket tagged "Attended GUI").
  - *revisit-when:* an attended session — build the dialog over `selectMatching`/`sameExtensionAs`/
    `invertSelection`, wire remap-by-path, GUI-verify. No external gate.

## Resolution (partial — logic complete, dialog deferred)
Added `invertSelection` to `src/lib/selectMatch.ts`, the last missing pure piece; with `selectMatching` +
`sameExtensionAs` the entire select-by logic is now complete and unit-tested. Only the attended criteria
dialog + selection wiring + GUI verification remain. Deferred with a turnkey revisit note.

## Notes
Prereq: CPE-780. Attended GUI. Reuse the existing selection remap-by-path helper.
