---
id: CPE-1962
title: `hc-contrast.test.ts` and `app.css.test.ts` lack the symmetric parity check `dark-contrast.test.ts` already has — a new token can be added to light and silently missed by three theme blocks
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1069's Reviewer (round 3), which **measured** the gap rather than describing it.

A semantic colour token added to the `light` block can be omitted from another theme block and no
guard will say so — **except for `dark`**, which is already covered. The coverage is uneven, and until
now it was described in the codebase as uniformly absent, which is worse than describing it as
uniformly present because it aims the fix at the wrong place.

| block | fixture check (`SEMANTIC_TOKENS`) | symmetric check | a brand-new token covered? |
|---|---|---|---|
| bare `:root`, `light` (`app.css.test.ts`) | yes | **no** | **no** |
| `dark` (`dark-contrast.test.ts`) | yes | **yes** | **yes** |
| `hc-light`, `hc-dark` (`hc-contrast.test.ts`) | yes | **no** | **no** |

`dark-contrast.test.ts` (~lines 155-161) carries a second, **fixture-independent** check:

```js
const lightOnly = [...lightSemanticDecls.keys()].filter(
  (name) => !SEMANTIC_TOKENS.includes(name) && !darkSemanticDecls.has(name),
);
```

with a comment stating its purpose exactly: *"keeps the fixture itself honest if a future ticket adds a
new semantic token to light but not dark."* A brand-new token joins it automatically — **no fixture
edit required**, which is the whole point, since the fixture is the thing nobody remembers to update.

`hc-contrast.test.ts` has only the two `missing = SEMANTIC_TOKENS.filter(...)` checks and no symmetric
counterpart.

## The measurements (do not re-derive; do re-confirm)

Both taken against PR #1069's head by deleting the newly-added `--accent-text`:

- deleted from the **`dark`** block → `dark-contrast.test.ts` fails with
  `tokens present in light but missing from dark: --accent-text`, **naming the token**, which never
  appears in `SEMANTIC_TOKENS`. **Caught.**
- deleted from the **`hc-dark`** block → `hc-contrast.test.ts` stays **green**. **Missed.**

## Scope — this is a copy, not a design

Copy the `lightOnly` idiom twice in `hc-contrast.test.ts` (light→`hc-light`, light→`hc-dark`), and give
`app.css.test.ts` the same treatment for its bare-`:root`↔`light` pair. Roughly nine lines each. It
closes the only three blocks a new token can silently miss.

## Acceptance criteria

- [x] **Re-confirm both measurements first** on current `main`, by deletion, not by reading. This
      ticket exists because an *unmeasured* claim about these same files reached a code comment — twice
      in one PR. Do not make it three.
- [x] Add the symmetric check to `hc-contrast.test.ts` for **both** hc blocks, and to `app.css.test.ts`
      for bare `:root`↔`light`.
- [x] **Red-proof each one individually.** Delete a token from each of the three newly-covered blocks
      in turn and confirm the right test names the right token. A single deletion proving one of three
      is not evidence for the other two.
- [x] Decide what "semantic" means at each site and keep it consistent with `dark-contrast.test.ts`'s
      existing derivation, rather than inventing a second definition. If the definitions must differ,
      say why at the site.
- [x] Check whether the hc blocks legitimately omit any light token today. If they do, the check needs
      an explicit, **reasoned** exception list — not a widened filter that quietly swallows the case.
      A fixture the guard skips is the thing this ticket is about.
- [x] Update PR #1069's guard-header note, which will by then describe this ticket as the fix.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1069's round-3 Reviewer, which caught the codebase
asserting *"there is no general theme-parity guard in this repo"* when one of the four blocks was in
fact guarded — and proved it by deletion.

Related: **CPE-1919** (the `--accent-text` split, PR #1069 — where the gap was found), **CPE-1492** /
**CPE-1493** (the dark theme + its WCAG guard), **CPE-1933** (derive provenance, don't claim it — the
family this belongs to: a written claim about another file that nobody ran).

## Work Log

**2026-08-27 — implemented.** Branch `cpe-1962-symmetric-theme-parity-guards`, based on
`origin/main` @ `753cd8a6` (CPE-1919 / PR #1069 merged).

### 1. Re-confirmed both measurements on current `main`, by deletion

Not by reading — this ticket exists because an unmeasured claim about these files reached a code
comment twice in one PR.

| deletion | test run | result |
|---|---|---|
| `--accent-text` removed from the `dark` block | `app.css.dark-contrast.test.ts` | **FAILS**, `tokens present in light but missing from dark: --accent-text` — 1 failed, 12 passed |
| `--accent-text` removed from the `hc-dark` block | `app.css.hc-contrast.test.ts` | **fully GREEN, 23/23** |

Both reproduce the ticket exactly.

### 2. The symmetric checks

- `app.css.hc-contrast.test.ts` — the `lightOnly` idiom copied twice (light→`hc-light`,
  light→`hc-dark`), folded into the two existing completeness `it`s exactly as
  `dark-contrast.test.ts` folds its own.
- `app.css.test.ts` — one new `it` for the bare-`:root`↔`light` pair, run in **both** directions
  (that pair is symmetric by construction: bare `:root` is the fallback the explicit selector must
  reproduce).

One definition of "semantic", reused: the light block's own declarations, with `SEMANTIC_TOKENS`
names excluded because the adjacent `missing` assertions already require those present in both
blocks. Same derivation as `dark-contrast.test.ts`; no second definition invented.

### 3. Red-proofed each new check individually

One deletion proving one of three is not evidence for the other two, so each was run on its own
against the finished code:

| deletion | test | message | tally |
|---|---|---|---|
| `--accent-text` from `hc-light` | `hc-contrast.test.ts` | `tokens present in light but missing from hc-light: --accent-text` | 1 failed, 22 passed |
| `--accent-text` from `hc-dark` | `hc-contrast.test.ts` | `tokens present in light but missing from hc-dark: --accent-text` | 1 failed, 22 passed |
| `--accent-text` from the bare `:root` semantic block | `app.css.test.ts` | `tokens declared in :root[data-theme="light"] but missing from the bare :root semantic block: --accent-text` | 1 failed, 21 passed |

### 4. The CPE-1919 `--accent-text: ;` hole is NOT inherited

`extractDecls`'s regex accepts the degenerate empty form (`\s*` after the colon backtracks so
`[^;]+` can consume its space) — verified directly: it matches, capturing `" "`, trimmed to `""`.
A raw `decls.keys()` parity check would therefore have read `--accent-text: ;` as *present* and
stayed green. Closed with a local `declaredNames` filter (value must be non-empty) at the new call
sites; the shared `extractDecls` is **untouched**, per the ticket's warning about the
palette-resolution path and the valid multi-line declaration form. Measured: replacing hc-dark's
`--accent-text` with `--accent-text: ;` now fails with `tokens present in light but missing from
hc-dark: --accent-text`.

### 5. Legitimate hc omissions — checked, found one, and it was NOT legitimate

Sweeping every name the `light` block declares against each other block found exactly one omission
in the whole file: **`--log-warn`**, declared in bare `:root`, `light` and `dark` but in **neither**
hc block. Both hc themes were therefore inheriting light's `#8a5a00` — an amber calibrated for a
white surface — through the fallback block. Measured against each theme's own surfaces
(`--bg` / `--surface` / `--surface-alt`):

| theme | inherited `--log-warn` | ratios | verdict |
|---|---|---|---|
| `hc-light` | `#8a5a00` | 5.93 / 5.93 / 5.29 | clears AA, misses hc's own >=7:1 bar |
| `hc-dark` | `#8a5a00` | **3.54 / 3.28 / 2.94** | under AA's 4.5:1 body-text floor on every surface, and under 1.4.11's 3:1 on the hover fill |

That is the log viewer's WARN badge text and row rule at 10-11px, in the theme whose entire premise
is legibility — the CPE-1810/CPE-1821 defect class again, reached via the fallback *block* rather
than a `var()` fallback argument. So it was **fixed, not excepted**: both hc blocks now declare
`--log-warn: var(--warn)`, their own already-verified amber (`hc-light` 7.83 / 7.83 / 6.99;
`hc-dark` 14.08 / 13.03 / 11.67). `--warn` is the right alias target in hc and *not* in `dark`,
where `--warn` doubles as `--warn-fill` under white text and so must stay dim — which is exactly
why `dark` declares a separate, brighter `--log-warn`. `hc-dark` already split `--warn-fill` onto
its own primitive, so its `--warn` is free to be the bright one both roles want.

**The new checks therefore ship with no exception list at all**, which is the strongest available
outcome: a fixture the guard skips is the thing this ticket is about. A note at the site says so,
and says that a future genuine omission must be named with a reason rather than swallowed by a
widened filter.

### 6. Selector lists

The new checks read one brace-balanced block per theme selector, so a token declared via a selector
list that merely *includes* the block (`:root, [data-theme="hc-dark"] { ... }`) reads as missing —
the same behaviour CPE-1919 documented. Deliberately kept and stated at the site: over-strict, but
it fails **closed**, and it enforces the one-block-per-theme convention both files already assume
with their `length !== 1` guards.

### 7. Guard-header note updated

`src/app.css.accent-text-contrast.test.ts`'s CPE-1919 note now reports the coverage *after* this
change (all three rows `YES`), records all four deletions with their exact messages and tallies —
including hc-dark's before/after — states what the symmetric checks still do **not** cover (a token
missing from every block including light; a present-but-wrong value), and records the `--log-warn`
finding and why it was fixed rather than excepted.

### Verification

- `npm run check` — **0 errors, 0 warnings**.
- `npm test` — 348 files, **4986 passed**, 2 skipped. Delta vs `main`: **+1 test** (the one new `it`
  in `app.css.test.ts`; the hc additions are extra assertions inside the two existing `it`s, so that
  file stays at 23). No file-count change, no failures.
- `node scripts/ratchet-baselines.mjs compare origin/main` — 12 baselines, all **unchanged**; no
  baseline raised.
- Docs: `src/docs/35-appearance.md` gained a short user-facing note on the High-contrast WARN colour
  (no new section, so `sectionDocs.ts` is unchanged).

---

## Round 2 — three factual errors in the coverage comments themselves

Review round 2 blocked on three defects in the **comments**, not the code: the exact defect class
this ticket exists to close, in the exact files it exists to fix. Text-only change; no behaviour
moved. Every correction below was re-measured by running the thing, not by re-reading it.

### R2.1 — wrong tally, twice

`src/app.css.hc-contrast.test.ts` recorded both hc red-proofs as `(1 failed, 23 passed)`. The file
has **23 tests total** (`npx vitest run src/app.css.hc-contrast.test.ts` -> `23 passed (23)`), so a
single failure leaves **22**. Re-ran the hc-dark deletion to confirm the shape: `1 failed | 22 passed
(23)`. Both lines corrected to `22`. The PR already recorded 22 correctly in
`accent-text-contrast.test.ts`, so the diff had been contradicting itself.

### R2.2 — a coverage claim that was false, disproved by deletion

`accent-text-contrast.test.ts` annotated CPE-1919's stale "all stayed GREEN" paragraph with *"that is
no longer true of the first two"*. Re-ran the deletion under discussion — `--accent-text` removed
from the **hc-dark** block, nothing else changed — against each named file on its own:

| file | result | note |
|---|---|---|
| `app.css.hc-contrast.test.ts` | **1 failed, 22 passed** | the only file CPE-1962 changed here |
| `app.css.test.ts` | **22 passed, fully GREEN** | its new check is bare-`:root` <-> `light` only, so no hc block is in its reach |
| `app.css.warn-token.test.ts` | **1 failed, 72 passed** | fails on `main` too — not CPE-1962's doing |

So `"the first two"` -> `"the first"`.

`warn-token.test.ts` turned out to be a **fourth** stale coverage claim in this lineage, and it is
not one this ticket created: re-ran the same deletion on `main` at `a334bd9f` and it fails there
identically, `:root[data-theme="hc-dark"] defines --accent-text as a concrete hex (referenced from
AgentTimeline.svelte x3, IcalPreview.svelte, SidecarManager.svelte)`. CPE-1875's guard only sees a
token some component spells as `var(--token, <fallback>)`; those five call sites landed *after*
CPE-1919 took its reading, so that third of the sentence went stale on its own, silently. Recorded
at the site with the measurement rather than left standing. Four stale claims now in one lineage,
every one found by re-running the deletion and none by re-reading the code.

Keeping the stale paragraph and annotating it (rather than deleting it) was confirmed correct in
review — it records the measurement that motivated the ticket. The annotation just had to be true.

### R2.3 — three honest limitations now stated

1. **One-directional.** `dark`'s, `hc-light`'s and `hc-dark`'s symmetric checks all ask only
   "is everything `light` declares also declared here?". A token in a theme block but absent from
   `light` is unchecked by anything. Only `app.css.test.ts`'s bare-`:root`/`light` pair runs both
   ways (that pair is symmetric by construction; a theme block is not). Swept both directions over
   all four `data-theme` blocks: each declares the **same 46-token non-empty set**, zero omissions
   either way, zero empty-value declarations — so this is a **gap in the guards, not a live
   defect**. Stated in `hc-contrast.test.ts` and in the header's "does NOT cover" list.
2. **The empty-value hole survives in the `dark` row.** `dark-contrast.test.ts`'s `lightOnly` still
   reads raw `lightSemanticDecls.keys()`, so a `--foo: ;` in `dark` would satisfy it; the other
   three blocks now filter it. The header table's `YES` for `dark` is therefore marginally weaker
   than the other two rows. Stated, not fixed — it deserves its own red-proof, not a drive-by in a
   file this ticket does not otherwise touch.
3. **Selector-list behaviour** was stated in `hc-contrast.test.ts` but not in `app.css.test.ts`.
   Added there, with the same fails-closed reasoning.

### R2.4 — the `--warn`-doubles-as-`--warn-fill` phrasing: corrected

Round 1's hc-dark comment justified aliasing `--log-warn` to `--warn` by saying dark's `--warn`
"doubles as `--warn-fill`". `hc-light` does exactly the same (`--warn-fill: var(--warn)`), so the
phrase does not distinguish the two cases — and the bare `:root` block's own comment already frames
this correctly as **role tension**. Rewritten to that language, with per-theme measurements:

- `hc-light` `--warn` `#734900` — **7.83:1** as text on its white `--surface` *and* **7.83:1**
  carrying white as a fill. Both roles want the same dark amber; `--log-warn` joins them for free.
- `dark` `--warn` `#c38800` — carries white at **3.07:1**, essentially zero headroom over 1.4.11's
  3:1 fill floor, so it cannot be brightened. `dark` `--log-warn` `#ffb84d` reads
  **9.48 / 8.24 / 8.80** as text (`--bg` / `--surface` / `--surface-alt`) but would carry white at
  **1.72:1**. Each value is right for one role and disqualifying for the other — hence two tokens
  in `dark`.
- `hc-dark` has no tension left: it already split `--warn-fill` onto its own primitive, leaving
  `--warn` free to be the bright amber both foreground roles want.

### R2.5 — a `--log-warn` role nobody had measured

`.log-chip[data-level="warn"].active` (`LogPreview.svelte:341`) paints `--log-warn` as a **16%
`color-mix` tint background** under `color: var(--text)`, with the undiluted token as
`border-color`. `LogPreview.contrast.test.ts` derives that pairing for the **light and dark blocks
only** — it never reads an hc block — so nothing guarded either hc chip before this change and
nothing guards it after. **No regression**; every reading improves or holds:

| reading | hc-dark | hc-light |
|---|---|---|
| `--text` on the 16% tint | 17.38 -> **13.80** | 16.75 -> **16.31** |
| chip border vs `--surface` | 3.28 -> **13.03** | 5.93 -> **7.83** |

The border reading that sat near 1.4.11's 3:1 non-text floor gains four times its headroom.
Extending `LogPreview.contrast.test.ts` over the two hc blocks is a genuine coverage gap and a
**follow-up** — widening its theme list means re-deriving its surfaces per theme and red-proofing
each, which is its own change.

### Round 2 verification

- Rebased on `origin/main` (`a334bd9f`) before touching anything.
- `npm run check` — 0 errors, 0 warnings.
- `npm test` — 348 files, 4986 passed, 2 skipped (unchanged; text-only round).
- `node scripts/ratchet-baselines.mjs compare origin/main` — 12 baselines, all unchanged;
  `hex-files: 85` and `hex-occurrences: 277` both unchanged. (That ratchet walks `.svelte` files
  only, so comment edits in `app.css` cannot reach it either way.)
- Every deletion made for measurement was reverted; `git status` clean before committing.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1077, after two rounds.

**The new guard found a real, ship-blocking defect on its first run.** `--log-warn` was declared in bare
`:root`, `light` and `dark` but in **neither** high-contrast block, so both hc themes inherited light's
`#8a5a00` through the fallback. Measured against each theme's own surfaces, `hc-dark` gives
**3.54 / 3.28 / 2.94** — the log viewer's WARN badge at **10.5px** body text, **below AA's 4.5:1 floor on
every surface, in the high-contrast theme**. The row's `border-left-color` at 2.94 also misses 1.4.11's
3:1. An independent from-spec WCAG implementation reproduced **every digit**.

**It was fixed, not excepted**, by aliasing `--log-warn: var(--warn)` in both hc blocks — so the new
checks ship with **no exception list at all**. `--warn` is the right alias in hc and wrong in `dark`,
where the two roles genuinely conflict: dark's `--warn` `#c38800` carries white at **3.07:1**, zero
headroom over the 3:1 floor, so it cannot brighten, while `--log-warn` `#ffb84d` reads ~9.5:1 as text
and only **1.72:1** as a fill. Two tokens because one colour cannot serve both.

**The ticket existed because coverage claims went unmeasured twice — and this PR shipped a third.** Its
guard header said an `hc-dark` deletion is now caught by *"the first two"* files. Its Reviewer ran that
deletion against the second: **22/22, fully green**. Only `hc-contrast.test.ts` changed behaviour. Plus a
fourth of the same family in the same diff — a red-proof tally written `(1 failed, 23 passed)` in a
23-test file where 22 pass, **while the same PR records 22 correctly elsewhere**.

**Round 2 found a fifth and handled it right.** Re-running the deletion turned up that
`warn-token.test.ts` also reds on it — and reds identically on `main`. Rather than quietly narrowing the
sentence, it **measured that on `main`** and recorded it, because leaving "warn-token stayed GREEN"
unqualified would have been the same defect again.

**Why it keeps happening, named in the record:** every author measured what they *changed*, and none
measured the sentence *about other files* they wrote while doing it — because that sentence feels like
description rather than a claim. It is a claim, and it is cheap to check. **The countermeasure that
caught all five: every coverage sentence names the deletion that proves it.**

**Honest limits now stated rather than implied:** the hc and dark checks are **one-directional**
(light to X only), and `dark-contrast.test.ts`'s own `lightOnly` still reads raw `.keys()`, so `dark`
retains the empty-value hole the other three blocks no longer have. Its `YES` is marginally weaker than
the rest, and the file says so.

**Merged past two verified reds** — shard 2 (CPE-1960) and its verdict job — after proving by
`git cat-file` that this branch predates that fix.
