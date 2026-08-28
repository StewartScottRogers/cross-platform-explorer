---
id: CPE-1968
title: A click on a rule pill within ~150 ms of the Organize dialog opening is silently swallowed — the centred dialog grows ~195 px under the pointer
type: bug
priority: Medium
status: Done
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

## Closing record — merged as PR #1099 (`1fdd79b9`), 2026-08-28

### The fix

`.preview` moves from a content-driven height (`min-height: 120px; max-height: 45vh`) to a stable
`height: clamp(200px, 40vh, 340px)`. **The invariant is that the height must not depend on the plan** —
that dependency is what moved the pills.

**Option 2 of the ticket's three, chosen by the Foreman and recorded at the site:**

- **Stop centring** fixes it completely and is cleanest in isolation, but **28 components share the
  centred-backdrop rule**; changing one dialog is visibly inconsistent and changing all 28 is a different,
  larger ticket.
- **Freeze the measured height while loading** needs JS measurement and has a first-load case with no
  previous height to hold — a special case on the exact path that is broken.
- **A single stable height** removes the jump on open **and** on every rule switch, with no JS, no
  measurement and no first-load exception. Its cost is a mostly-empty box for a two-file plan. That cost
  is **predictable**, and PURPOSE.md's tiebreaker is fast / small / **predictable**.

The three numbers were chosen against measured content (~141 px for a two-file plan, ~533 px for a
4-group/20-file one, so this is a scroll viewport; 40vh = 280 px ≈ 14 rows at 700 px) and the reasoning is
at the `.preview` rule.

**A correction to the Foreman's own framing, from the Visual Critic:** *"would a smaller clamp floor be
better?"* is the wrong lever. At 700 px, `40vh` = 280 px, so **the 200 px floor never binds** — it engages
only below a 500 px window. The knob is the `40vh` middle term, and it should stay: the large-plan shot
shows 280 px already fits only 2 of 5 groups, so shrinking it would hurt the common case to flatter the
rare one.

### `MacrosDialog` — fixed, not scoped out

Identical shape: `+ New macro` above a `.list` that grows when `onMount(refresh)` → `macroList()` lands.
Same fix **plus `flex: 0 0 auto`**, because its `.dialog` is a flex column — and the two properties were
**red-proofed separately**, since removing both at once only proves the pair. With the height in place and
`flex` alone deleted, **exactly 1 of 16 reds**: the flex column otherwise shrinks the fixed height back to
content while the `height` declaration still reads correct. `OrganizeDialog` needs no `flex` because its
`.dialog` is a plain block — asymmetry confirmed rather than assumed.

### Two independent measurements that agree

- **Model:** jsdom has no layout, so the test models the dialog's vertical stack **from the component's own
  declarations** (read through `src/lib/svelteCss.ts`), takes the point aimed at the pills at t=100 ms,
  hit-tests it at t=180 ms, and dispatches a real click at whatever is there — asserting `.rule.active` and
  the reloaded plan. Reverting the CSS reds **3 of 15** with `landed in "preview" … pills moved 97.5px`.
- **Real browser:** `scripts/dev-harness/organize-dialog/` mounts the real component in headless Chrome at
  gui-smoke's exact 1000×700 — **97.5 px** shift with the old CSS, **0.0 px** with the new, `.rules` at
  **187.0 px** in all states.

**Why that agreement is non-trivial, per the Critic:** the *shift* (97.5) is just `(315−120)/2` — every
other term cancels, so agreement there is weak evidence. But the model also produces **absolute** band
positions, and those are what the hit-test consumes. Hand-deriving from the shipped CSS (`1+20`, `26+10`,
`28+12`, `280+12`, `30` → 440, centred in 700) gives `.rules` top = **187 px**, exactly the browser's
figure. That is the agreement the verdict rests on.

### Visual verdict — `VISUAL PASS`

The shot that mattered was the two-file plan, ~140 px of empty box: judged **deliberate, not broken** — a
1 px bordered, rounded, distinctly-filled container with top-aligned content is the universal listbox
idiom, and *"crucially the box is identical in all three states, which is what sells it as intentional."*
The large-plan shot is **better** than before: a pill clipped mid-height at the bottom edge is a cleaner
"more below" affordance than the old clipped filename. Standards clean — no hard-coded hex, tick-tack
reflow fully compliant, no new `color:` resolving to `--accent` in any spelling including
`var(--accent, <fallback>)`.

One visual change taken: `.empty` is now centred in the roomy box. At 120 px a top-left "Loading preview…"
read fine; at 280 px it was the one frame that read as unfinished. Photographing it also exposed a defect
in the harness's own badge — it inferred "settled" from a `summary` element an empty plan never renders,
so it printed "(loading)" after the plan had landed. It now records resolution at the mock, the only place
that knows for certain. Shot count 6 → 8.

### Four corrections, all the shift's recurring family

1. **`src/lib/svelteCss.ts`'s stripper was justified by a failure mode that is backwards.** The header said
   stripping was load-bearing because the fix ships with a comment quoting the old declarations. Measured:
   with the strip disabled, **31/31 green** — the comment never matches the anchor. And without the
   stripper a commented-out *whole rule* yields two matches and `styleBlock` **throws** — a loud red, not a
   silent pass. Kept as correct-and-defensive; the sentence narrowed to what was measured, with the shape
   that *would* need it named (live rule deleted, commented copy left behind).
2. **`.err` gap: verdict kept, argument replaced.** *"A failure path, not the load path"* was untrue — a
   failing `organize_plan` arrives on **exactly** the load path at the same t=120 ms. What makes it
   acceptable is **magnitude**: `.err` is ~15 px + 8 px margin, so the pills move ~11.5 px against a 28 px
   pill and a centre-aimed pointer stays on target. The 195 px did not.
3. **The 85vh claim was bounded, and it is worse than "barely reachable."** `min_inner_size` is
   `(600.0, 400.0)` — **width-first**, so the window can legitimately be **400 px tall**, which puts
   400–424 px inside the band where the dialog now scrolls internally and did not before. Value unchanged
   (the app opens at 700 px and it degrades to a scrollbar); the regression is recorded.
4. **A second height change in the same file was added to the "not covered" list**, which had read as
   exhaustive: `{#if outcome}` replaces `.preview` entirely, so the dialog re-centres when `organizeApply`
   resolves. User-initiated, double-click inert via `applying`, and **where a stray post-Apply click lands
   has not been measured** — said plainly rather than implied.

### The enumeration gap — filed as CPE-1983, not fixed here

The PR **recalled** the neighbour the ticket named rather than enumerating (CPE-1932). Sweeping all **137**
`.svelte` files for the same shape returns **at least five more**, and one is **worse** than the one fixed:
`CheckpointDialog.svelte:363` has `.list { max-height: 30vh; overflow: auto; }` filled by `onMount`, with
**five interactive controls above it** and **`Revert…` buttons inside it** — so there the mis-landed click
can hit a **destructive** control, and it re-fires on every Refresh. The shared `svelteCss.ts` this PR
landed is what makes a repo-wide enumerating guard cheap, which is the right shape: **a per-component
assertion cannot close a class.**

### Riding refactors — both checked clean

`src/lib/svelteCss.ts` is now the single CSS derivation both dialog tests read through. The layout-guard
dev server's lifecycle moved to `layout-guard/dev-server.mjs` so the new harness **reuses its CI-hang fixes
rather than paraphrasing them** — pid-derived port, ANSI-stripped handshake, `shell: true`, POSIX
`detached` + negative-pid group kill, Windows `taskkill /T`, all preserved verbatim, and marginally
**stronger**: `startDevServer` now tears down on a failed handshake itself. `harness:layout-guard` re-run
at **14/14 PASS**.

### CPE-1965's derivation test inverted, as that ticket predicted

*"Gives `.preview` a different height while loading than once the plan renders"* now asserts the opposite,
naming both tickets and explaining the flip; `organize.smoke.ts`'s note is relabelled
`[FIXED IN CPE-1968 — historical]` rather than left to rot.

### Gates at merge

`npm run check` 0 errors · vitest **5,386 passed / 2 skipped / 359 files** · `harness:layout-guard` 14/14 ·
red-proof **3 of 15** verbatim · harness **97.5 / 0.0 / 187.0** · CI `completed success — total_count=26
pending=0 skipped=1 coverage=ok`.

**Screenshot caveat, stated by the author and repeated by the Critic:** real component, real CSS, headless
Chrome — **not the built app**, so window chrome, the folder view behind the dialog, and any WebView2
layout difference remain unseen.

**Family:** CPE-1965 (the spec-side fix, its CI rate, and the derivation test this inverts), CPE-1983 (the
five more dialogs, enumerated), CPE-1728 (the slow-renderer family), CPE-1142/1143 (the dialog and its
smoke spec), CPE-1148 (the screenshot harness), CPE-1932 (enumerate, don't recall), CPE-1933 (do not name
a backstop without checking it can fire).
