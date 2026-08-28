---
id: CPE-1962
title: `hc-contrast.test.ts` and `app.css.test.ts` lack the symmetric parity check `dark-contrast.test.ts` already has — a new token can be added to light and silently missed by three theme blocks
type: task
priority: Medium
status: Open
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

- [ ] **Re-confirm both measurements first** on current `main`, by deletion, not by reading. This
      ticket exists because an *unmeasured* claim about these same files reached a code comment — twice
      in one PR. Do not make it three.
- [ ] Add the symmetric check to `hc-contrast.test.ts` for **both** hc blocks, and to `app.css.test.ts`
      for bare `:root`↔`light`.
- [ ] **Red-proof each one individually.** Delete a token from each of the three newly-covered blocks
      in turn and confirm the right test names the right token. A single deletion proving one of three
      is not evidence for the other two.
- [ ] Decide what "semantic" means at each site and keep it consistent with `dark-contrast.test.ts`'s
      existing derivation, rather than inventing a second definition. If the definitions must differ,
      say why at the site.
- [ ] Check whether the hc blocks legitimately omit any light token today. If they do, the check needs
      an explicit, **reasoned** exception list — not a widened filter that quietly swallows the case.
      A fixture the guard skips is the thing this ticket is about.
- [ ] Update PR #1069's guard-header note, which will by then describe this ticket as the fix.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1069's round-3 Reviewer, which caught the codebase
asserting *"there is no general theme-parity guard in this repo"* when one of the four blocks was in
fact guarded — and proved it by deletion.

Related: **CPE-1919** (the `--accent-text` split, PR #1069 — where the gap was found), **CPE-1492** /
**CPE-1493** (the dark theme + its WCAG guard), **CPE-1933** (derive provenance, don't claim it — the
family this belongs to: a written claim about another file that nobody ran).
