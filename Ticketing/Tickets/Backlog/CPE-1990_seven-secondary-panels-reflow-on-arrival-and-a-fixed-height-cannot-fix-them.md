---
id: CPE-1990
title: seven secondary panels reflow on **arrival**, where a fixed height cannot help — the allowlist CPE-1983 had to open
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **CPE-1983**'s worker (PR #1107) while sweeping every centred dialog in the app for bodies that
grow after an async load. Of the **22** scroll boxes it found, ten were fixed with a stable height and
**seven were allowlisted with per-entry reasons**, because **the growth is the panel's own arrival**, not a
body expanding inside a panel that was already there.

**A stable height cannot fix that.** The dialog is one element taller the moment the panel mounts; giving
the panel a fixed height only decides *how much* taller. Everything above it still moves on a centred
backdrop, and the pointer still lands somewhere else.

The allowlist is registered as the `dialog-body-reflow-allowlist` ratchet with a declared row in
`docs/design/RATCHETS.md`, so it cannot grow silently — **but an allowlist is a count wearing a coat, and
seven entries is seven open instances of a real defect.**

## Why this is its own ticket rather than a widening of CPE-1983

That ticket's decision — **give the body a single stable height** — was chosen (in CPE-1968) over two
alternatives, and the one that actually addresses arrival is the one deliberately deferred:

- **Stop centring** (`place-items: start center` + a top offset) fixes **both** shapes completely, and is
  the cleanest answer in isolation — **but 28 components share the centred-backdrop rule**, so changing one
  dialog is visibly inconsistent and changing all 28 is an app-wide visual-identity decision with its own
  review.
- **Freeze the measured height while loading** needs JS measurement and has a first-load case with no
  previous height to hold.

**So this ticket is very likely the app-wide backdrop decision**, arriving from the direction that
justifies it. Treat it as a design question first, not a CSS edit.

## What this needs

- [ ] **Re-derive the seven at run time** (CPE-1932) from the allowlist and confirm each entry's stated
      reason against the component — **including the ones that turn out to be wrong**. An allowlist entry
      is a claim like any other, and this repo's rule is that each earns its place.
- [ ] **Decide the shape, and record the reasoning at the site, not only in the PR body.** If the answer is
      "stop centring", say what it costs across all 28 backdrops and how consistency is preserved; if it is
      per-panel, say why arrival is tolerable where growth was not.
- [ ] **Whatever lands, the allowlist should shrink.** Report the before/after count and **have the ratchet
      prove it** — a lowering always sails through the guard; it is a raise that needs a row.
- [ ] **Red-proof with both instruments CPE-1983 proved worth having**: model the vertical stack from the
      component's own declarations and **hit-test the point aimed at the control**, *and* cross-check in a
      real browser. CPE-1983 measured `CheckpointDialog` at **93 px (model)** vs **84.0 px (headless Chrome
      at 1000×700)** and explained the 9 px as a documented lower bound in the model — **carry that
      explanation forward rather than rediscovering the gap.**
- [ ] **Mind the two-property trap.** Where a component needs `height` **and** `flex: 0 0 auto` inside a
      flex column, **red-proof them separately** — CPE-1983 measured that deleting the flex half alone reds
      one test in its own block and **leaves the repo-wide guard GREEN**, so **the flex half is the one no
      enumerating guard covers.** That blind spot is stated at the site and is still open.
- [ ] **Photograph the states you change**, and **report the ones you did not**. CPE-1983 photographed one
      of ten components and said so; CPE-1968's harness had a fixture that was never added to its state
      list, so an empty-state frame went unseen and turned out to be the weakest one.

## Two harness lessons this ticket inherits, both paid for

- **`svelteCss.ts`'s comment stripper is load-bearing for an enumerator, and the opposite was measured on a
  narrower scan.** PR #1099 measured it "buys nothing"; CPE-1983 measured that **unstripped, the population
  drops 22 → 8 and the 14 lost are exactly the boxes already fixed** — because **a fix ships with a
  comment, and a comment above a rule is swallowed into that rule's selector.** A smaller population is all
  green: **a silent pass.** Do not inherit either finding; **re-measure for your own scan.**
- **A before/after harness can report the fixed state for both arms.** CPE-1983's first run used
  `delay=0`, so both probe samples landed post-load and **every `before` shot read 0.0 px — identical to the
  fixed build.** Settled states need a real delay. **Check your own harness cannot do this before trusting a
  single number from it.**

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1983's worker (PR #1107), which enumerated the class,
fixed what a stable height can fix, and **declared the remainder rather than quietly widening the
allowlist's meaning.**

Related: **CPE-1983** (PR #1107 — the sweep, the guard, the ten fixes and this allowlist), **CPE-1968**
(PR #1099 — the original decision and both red-proof techniques), **CPE-1965**, **CPE-1728** (the
slow-renderer family), **CPE-1934** (ratchets — an allowlist is a stored count), **CPE-1932** (enumerate,
don't recall).
