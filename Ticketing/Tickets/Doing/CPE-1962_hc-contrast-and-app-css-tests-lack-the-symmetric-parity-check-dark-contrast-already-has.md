---
id: CPE-1962
title: `hc-contrast.test.ts` and `app.css.test.ts` lack the symmetric parity check `dark-contrast.test.ts` already has — a new token can be added to light and silently missed by three theme blocks
type: task
priority: Medium
status: In Progress
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
