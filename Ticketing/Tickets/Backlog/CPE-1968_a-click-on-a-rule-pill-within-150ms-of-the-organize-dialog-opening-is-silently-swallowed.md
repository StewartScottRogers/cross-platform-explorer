---
id: CPE-1968
title: A click on a rule pill within ~150 ms of the Organize dialog opening is silently swallowed — the centred dialog grows ~195 px under the pointer
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`OrganizeDialog.svelte`'s backdrop is `display: grid; place-items: center` — the app-wide dialog
convention (28 components declare a `.backdrop { position: fixed; inset: 0; … place-items: center }`; 76 use `place-items: center` somewhere). Its `.preview` box is
`min-height: 120px; max-height: 45vh`. While the first `organize_plan` is in flight the preview is at
its 120 px floor; when the plan lands (~120 ms after mount — the dialog's own debounce) it grows to as
much as 45vh. At the 1000x700 window the gui-smoke harness uses that is a **~195 px growth on a
vertically centred dialog**, so everything above the preview — the title row and the four 28 px rule
pills — **slides UP by ~98 px** about an eighth of a second after the dialog appears.

A pointer already resting on "By extension" when the dialog opens therefore ends up ~98 px below the
pill it was aiming at, inside `.preview`. `.preview`'s ancestor `.dialog` carries
`on:click|stopPropagation`, so the stray click is **swallowed in silence**: no rule change, no error,
no visual feedback, dialog stays open. The user's only clue is that the pill they clicked did not
highlight.

The same jump happens on **every rule switch**, not only on open: a by_kind plan and a by_extension
plan are different heights, so the dialog re-centres and the pills move each time you change rule.

## How this was found

Diagnosed as the mechanism behind **CPE-1965** (`organize.smoke.ts` failing 3 of 69 shard-4 CI jobs).
The proof is run 33131342785's failure screenshot `organize-dialog-fail.png`: "By kind" still
highlighted after WebDriver reported a *successful* click on `[data-testid="rule-by_extension"]`, the
by_kind plan rendered, and `CPE-1143-photo.png` plainly present in the folder behind the dialog.
CPE-1965 fixed the **spec** (it now waits for the default preview to settle before clicking, and
asserts the pill became `.active`), which is correct for the harness and removes the CI red. It did
**not** fix the app, deliberately — see below.

## Why this was split out rather than fixed in CPE-1965

CPE-1965 was blocking PR #1074's merge, and every candidate app fix is a **visual-design decision**
that should go past the Visual Critic rather than ride an unblock PR:

- **Stop centring** (`place-items: start center` + a top offset) — fixes it completely and is the
  cleanest, but 28 components use this exact centred-backdrop rule; changing one dialog makes it visibly
  inconsistent, and changing all of them is a much bigger change.
- **Fix the preview's height** (`min-height: 45vh`) — no jump ever, including on rule switches, but a
  two-file plan then renders in a mostly-empty 45vh box.
- **Freeze the measured height while `loading`** — keeps small dialogs small, but needs JS measurement
  and has its own first-load case (there is no previous height to hold on mount).

Pick one with the Visual Critic. Whichever is chosen, the CPE-1965 derivation test in
`src/lib/components/OrganizeDialog.test.ts` ("gives .preview a different height while loading than once
the plan renders") will **red**, which is intended: it is the signal that the gui-smoke spec's wait has
become belt-and-braces rather than load-bearing.

## The latent neighbour

`gui-smoke/specs/macro-in-menu.smoke.ts:95` clicks `[data-testid="new-macro-btn"]` immediately after
`.dialog[aria-label="Macros"]` exists, and `MacrosDialog.svelte` does `onMount(refresh)` →
`commands.macroList()` → renders the list. Same shape: a control in the dialog's **header**, above a
body that grows when an async load lands, on a centred backdrop. It does not bite today only because
the smoke run's macro list is empty, so the load resolves to `[]` and nothing changes height. It would
bite the moment that dialog opens with macros already saved. Whatever fix is chosen here should be
checked against `MacrosDialog.svelte` too.

## Acceptance criteria

- [ ] Clicking a rule pill in the Organize dialog changes the rule **regardless of when** the click
      lands relative to the first `organize_plan` — including within the first 150 ms.
- [ ] Switching rules does not move the rule pills.
- [ ] The choice between the three options above is made with the Visual Critic, and the reasoning is
      written at the site, not only in the PR body.
- [ ] `MacrosDialog.svelte` checked for the same shape (see above) and either fixed or explicitly
      recorded as out of scope with a reason.
- [ ] Red-proof: show a click landing during the reflow window and now taking effect.

## Notes

Related: **CPE-1965** (the spec-side fix + the full CI enumeration and rate), **CPE-1728** (the
slow-renderer family), **CPE-1866** (session-per-shard, which is why the spec reaches the dialog so
fast), **CPE-1142/CPE-1143** (the dialog and its smoke spec).
