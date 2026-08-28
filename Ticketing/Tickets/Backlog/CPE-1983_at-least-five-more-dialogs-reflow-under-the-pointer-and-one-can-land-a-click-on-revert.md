---
id: CPE-1983
title: At least five more dialogs reflow under the pointer — and in one of them the mis-landed click can hit **Revert**
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **PR #1099**'s Reviewer (CPE-1968) by doing what that PR did not: **enumerating instead of
recalling** (CPE-1932). CPE-1968 fixed `OrganizeDialog` and the one neighbour its ticket *named*
(`MacrosDialog`). A sweep of all **137** `.svelte` files for the same shape — a centred fixed backdrop,
a body with `max-height` + `overflow` and **no** `height`, filled by an async load — returns **at least
five more**.

The shape, restated: the backdrop is `display: grid; place-items: center` (28 components share it), so
when the body grows after an async load the dialog **re-centres** and everything above the body **slides
up by half the growth**. A pointer already resting on a control ends up somewhere else. In
`OrganizeDialog` that was ~98 px and the stray click was swallowed in silence.

## The one that is worse than the one that got fixed

**`src/lib/components/CheckpointDialog.svelte:363`** — `.list { max-height: 30vh; overflow: auto; }`,
filled by `onMount(loadList)` (line 76, **two** `commands.*` round-trips).

- **Five interactive controls sit above it** (lines 201–217: help, path input, **Refresh**, label input,
  **Create checkpoint**).
- **`.list` itself contains `Revert…` buttons** (line 243).

So here the mis-landed click is **not** merely swallowed — **it can land on a destructive control.** It
also re-runs on every **Refresh** and after `doCreate`, so this is not only an on-open hazard.

## The rest, and the honest scope of the list

Also matching: `BatchMediaDialog` `.preview`, `CopilotDialog` `.op-list` / `.op-results`,
`MacroRunConfirm` `.ops` / `.collision-list`, `SyncDialog` `.log`.

**Say "at least these", never a count** (CLAUDE.md's round-9 rule). The Reviewer's sweep only sees the
`max-height`-with-`overflow` spelling; a body that grows some other way is invisible to it.

## What this needs

- [ ] **A repo-wide enumerating guard, not five per-component assertions.** *A per-component assertion
      cannot close a class.* CPE-1968 landed `src/lib/svelteCss.ts` as the single CSS derivation both
      dialog tests read through, **which is exactly what makes this cheap** — derive the component list at
      run time and **fail loudly on a near-empty enumeration** (CPE-1932: a hard-coded list of remembered
      instances is how seventeen `Cargo.lock` files became two).
- [ ] **Anchor on parsed CSS, never on comment text** (CPE-1933). Note PR #1099's Reviewer measured that
      `svelteCss.ts`'s comment stripper **buys nothing today** and that its stated failure mode is
      backwards — without it, a commented-out whole rule yields two matches and `styleBlock` **throws**, a
      loud red rather than a silent pass. Do not inherit that claim; re-measure it for your own scan.
- [ ] **Fix `CheckpointDialog` first and separately if that helps it land sooner** — it is the only one
      where the consequence is a destructive action rather than a swallowed click.
- [ ] **Reuse CPE-1968's decision rather than re-litigating it.** That ticket chose *give the body a
      single stable height* over *stop centring* (28 shared backdrops; changing one dialog is
      inconsistent, changing all 28 is its own ticket) and over *freeze the measured height while loading*
      (JS measurement plus a first-load special case on the broken path). If a component genuinely needs a
      different answer, say why **at the site**.
- [ ] **Red-proof each one the way CPE-1968 did**, and note the two techniques it proved worth having:
      model the vertical stack **from the component's own declarations** and hit-test the point aimed at
      the control, **and** cross-check against a real browser — `scripts/dev-harness/organize-dialog/`
      mounts the real component in headless Chrome at 1000×700 and agreed with the model to **97.5 px /
      0.0 px** and an absolute `.rules` top of **187.0 px**. The *shift* alone is weak evidence (every term
      cancels); **the absolute band positions are what the hit-test consumes.**
- [ ] **Where a component needs two properties to hold the height** (`height` **and** `flex: 0 0 auto`
      inside a flex column), **red-proof them separately** — removing both at once only proves the pair.
      CPE-1968 measured exactly this: with the height in place and `flex` alone deleted, **1 of 16** reds.
- [ ] **Report the states you did not photograph.** CPE-1968's harness had a `PLANS.none` fixture that was
      never added to `STATES`, so its empty state went unseen — and the Critic found the loading frame was
      the weakest one precisely because nobody had looked at it at the new size.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1099's Reviewer (CPE-1968), which enumerated rather than
accepting the PR's own neighbour list.

Related: **CPE-1968** (PR #1099 — the fix, the decision and the two red-proof techniques), **CPE-1965**
(the spec-side fix and the CI rate), **CPE-1728** (the slow-renderer family), **CPE-1932** (enumerate,
don't recall), **CPE-1933** (anchor on code, not prose).
