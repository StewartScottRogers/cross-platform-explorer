---
id: CPE-1983
title: At least five more dialogs reflow under the pointer — and in one of them the mis-landed click can hit **Revert**
type: bug
priority: High
status: Done
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

## Closing record — merged as PR #1107 (`3c8a87e1`), 2026-08-29

### The ticket said "at least five more". The sweep found 22.

**137 `.svelte` files → 68 centred dialogs → 22 scroll boxes**, re-derived independently by the Reviewer
with its **own `postcss`-based scanner** rather than the repo's helper: **exact match.** Split: **10 fixed
here + 7 allowlisted + 5 already `vh`-stable on `main`.**

The defect: a centred dialog whose body grows after an async load **re-centres**, sliding everything above
it up by half the growth. In `CheckpointDialog` that body contains **`Revert…` buttons**, so the mis-landed
click can hit a **destructive** control — and it re-fires on every Refresh, not only on open.

### The author's own first sweep found nine, and why is the useful part

The obvious CSS-rule regex — `/(?:^|\}|\{)\s*([^{}]+?)\s*\{([^{}]*)\}/g` — **consumes each rule's closing
brace, so every second rule is skipped.** It found 9 boxes over 8 files and **missed three of the instances
the ticket itself names.** **A guard that under-counts reports clean.** Pinned as its own case:
`"returns CONSECUTIVE rules, not every other one"`, which reds with `['.a','.c']` vs `['.a','.b','.c']`.
Swapping the shipped enumerator for that regex drops the population to 12 and reds the floor.

### The guard

`src/lib/dialogBodyReflow.test.ts` — `git ls-files` at run time, with **three loud non-vacuity floors**
(>100 files, >40 centred dialogs, ≥15 boxes), **each sabotaged individually and each red**: truncating the
file list reds all three; `position: sticky` reds the centred floor only; `overflow: clip` reds the boxes
floor only. Dialog roots are excluded **structurally** via `role="dialog"` — the exclusion removes exactly
12 root boxes and **no class is shared between a `role="dialog"` tag and a non-root tag** (measured).
`styleRules` in `src/lib/svelteCss.ts` is a real brace-depth enumerator, shared.

### The matcher defect the review's #6 exposed

`class="[^"]*\blist\b[^"]*"` — **and a hyphen is a word boundary in a regex.** It matched `drift-list`,
`log-line`, `res-outcome`, **reporting five multi-element boxes where there are three.** Confirmed
independently by a tokenised-vs-`\b` diff: **population 22 either way, nothing in either only**, phantoms
**exactly** `CheckpointDialog#list` (via `drift-list`) and `SyncDialog#log` (via `log-line`).

**The dangerous half is the direction nobody looks:** the same matcher backs the `role="dialog"`
**exclusion**, so **a body class that was a hyphen-substring of its root's would have been silently removed
from the population.** Over-matching can only *raise* the inclusion count, so the population cannot shrink
that way — but the exclusion reads the same matcher. Correctly stated as *"nothing does this today; the old
matcher could not have told us."*

**And the repair made the fact derived rather than restated:** the scan now records `elements` per box and
**a leg fails on any undeclared multi-element row**, so this cannot recur behind a single-element-shaped
reason. Sabotaged: renaming one allowlist key reds **both** new legs, 2 of 18.

### Ten fixes, and one allowlist entry that did not survive review

Each fixed site **names which clamp term actually binds at 700 px** — all ten checked arithmetically,
including six floor/cap crossovers. `MacroRunConfirm.ops` also needs `flex: 0 0 auto` because its dialog is
a flex column, and **the two properties were red-proofed separately**: deleting `flex` alone reds 1 of 40
**and leaves the repo-wide guard GREEN**, so **the flex half is covered by no enumerating guard** — stated
as an open blind spot at the site.

**Six of the seven allowlist entries checked out. The seventh did not.** `ColumnPickerDialog#list` names
**two** elements, one of them `available-list`, **unconditional and the dialog's main body** — it changes
height every time the user moves a column, **with the row they just clicked directly above it.** The
CPE-1983 shape hiding inside CPE-1983's own allowlist.

**Fixed by honest justification rather than a pin**, and the arithmetic supports it: `.dialog` is 85vh =
**595 px** at the harness window, two `.list` at 220 px = **440**, leaving **155** against chrome totalling
**~176** (padding 40 + head-row 36 + two section-heads 42 + margins 28 + actions 30). **It overflows by
~21 px and the dialog scrolls itself.** A two-box budget decision, deferred to **CPE-1990** and registered
there rather than filed under the six-panel paragraph.

### Both red-proof instruments, and why their disagreement is a documented limit

**Model:** jsdom has no layout, so the test models the dialog's vertical stack **from the component's own
declarations**, takes the point aimed at the control at t=100 ms, hit-tests it at t=180 ms, and dispatches
a real click. Reverting `CheckpointDialog`'s `.list` reds 3 of 47 with `landed in "list" … moved 93px` and
`confirm-revert` on screen.

**Browser:** headless Chrome at 1000×700 measures **84.0 px** (Refresh 320.6 → 236.6, `.list` top
396.6 → 312.6); shipped CSS gives **0.0 px** with the same absolute positions in all four states.

**The 9 px gap was verified from the shots themselves rather than accepted:** `.list top` 396.6 vs 312.6 ⇒
shift 84 ⇒ browser loading box = 210 − 168 = **42 px** = 24 px padding + one 18 px line, against the
model's stated 24 px lower bound. **A documented limit, not a bug in either instrument.**

**And a harness bug that would have made the whole comparison worthless:** with `delay=0` both probe
samples landed post-load, so **every `before` shot read 0.0 px — identical to the fixed build.** Settled
states now use 500 ms, and no shot in the set was taken with the broken timing.

### Visual

`VISUAL PASS`. The empty box needed the centring correction **CPE-1968's own review round had already
added for exactly this consequence** — without it a placeholder sits top-left with ~180 px of dead space
and reads as a failed render. Applied to the six boxes that have a placeholder, plus `.ops li.dim`
**scoped**, because the bare `.dim` also styles a `Planning…` sibling **in an auto-height parent where
`height: 100%` resolves against nothing.** Re-shot: both frames centre, **hit-test geometry untouched**
(Refresh 236.6 / `.list` 312.6 / 0.0 px), and `after-few` / `after-many` **byte-identical** — the right
invariant, since `.empty` is never mounted alongside rows.

**Five of the six are in flex columns and only one was photographed, so the author measured the rest and
committed the probe:** block, flex column and `<ul>` flex column all fill and centre at **0.0 px** —
re-run by the Reviewer, exact match. **And the probe records an artifact that nearly became a finding:**
without the app's global `box-sizing: border-box` the block case sits **8.0 px** low — reproduced verbatim,
with the provenance confirmed (`src/app.css:1070`). *A property of a probe that doesn't reproduce its
host's reset.*

### The number that was wrong three times, and is now deleted

A docblock explained what happens with the comment stripper disabled, and quoted the collapse as a figure.

| round | wrote | truth at that commit |
|---|---|---|
| 1 | "22 → 8 … the losses are exactly the fixed ones" | 8 ✓, but **two unfixed were also lost** |
| 2 | "22 → 9, losing 13, one unfixed" | **8**, 14, **two** |
| 3 | "at the time of writing 22 → 8" | **7**, 15, **three** |

**Round 3 is the one to keep.** It wrote the mechanism down correctly — *"which boxes vanish moves with the
COMMENTS in the tree; the commit that wrote the count is the commit that falsified it"* — **and left, two
lines above that sentence, a count its own three comment edits had falsified.** The Reviewer isolated it:
reverting only the three `.svelte` files to their round-2 text restores 8/14/two, and the single delta is
**a reworded line inside a block comment.** No CSS. No assertion.

**Round 4 deleted the figure rather than correcting it a fourth time** — *measuring it is what keeps
breaking it* — keeping all three failed attempts as history, each anchored to its commit with an explicit
line saying none of them describes the tree you are reading.

**And the closing argument is recorded at the site: the derived legs absorbed every one of those drifts
without a single `expect(...)` moving.** The guard was never in danger. Only the sentence beside it was.

### The stripper finding, which inverts a measured result from a merged PR

PR #1099's Reviewer measured that `svelteCss.ts`'s comment stripper **buys nothing** and that its stated
failure mode was backwards. This PR **re-measured for an enumerator rather than inheriting it**: unstripped,
the population **collapses**, and the losses include **both already-fixed boxes and still-content-driven
ones** — because **a fix ships with a comment, and a comment above a rule is swallowed into that rule's
selector.** Both facts are now **derived per-property** instead of pinned to box ids.

### Four site comments that described things which do not exist

`CopilotDialog` cited *"per-op checkboxes"* and an *"Apply / Cancel row"* — **neither exists**; the only
"checkbox" occurrences in the file are comments, one of them that one. `MacroRunConfirm` said its list is
*"empty when the confirm appears"* — **while planning it does not exist**, a one-line `Planning…` div does.
`SyncDialog`'s allowlist rationale would have excused the very box it fixes. And the `classTokens` header
cited **`res-outcome`, which exists nowhere in `src/`** — **an invented example inside the comment
explaining a defect caused by over-trusting a matcher.**

### Gates at merge

`npm run check` **0 errors, 0 warnings** · vitest **365 files / 5,564 passed / 62 skipped** ·
`ratchet-baselines.mjs compare origin/main` **exit 0**, row present in the diff and absent at base ·
`dialogBodyReflow.test.ts` **18/18** · CI `completed success — total_count=26 pending=0 skipped=1
coverage=ok`.

**Unseen, and disclosed:** **nine of the ten fixed components were not photographed** — verified by the
guard and by binding-term arithmetic. Judged sufficient: the guard covers the property, the arithmetic
checks at all ten sites, **the one photographed is the one whose mis-click is destructive**, and the
residue is a class judgement answered once — *"nine more screenshots of the same 200-odd-px box would not
add a fact."* **No `tauri build`** — the shots are real Chrome, not WebView2 and not the built app.

**Family:** CPE-1968 (PR #1099 — the original decision, both red-proof techniques, and the `.empty`
centring correction this reuses), CPE-1990 (the seven allowlisted panels that reflow on *arrival*, where a
fixed height cannot help, plus the `ColumnPickerDialog` two-box budget), CPE-1965, CPE-1728 (the
slow-renderer family), CPE-1934 (an allowlist is a stored count), CPE-1932 (enumerate, don't recall),
CPE-1933 (a fabricated mechanism next to a green test reads as vouched for).
