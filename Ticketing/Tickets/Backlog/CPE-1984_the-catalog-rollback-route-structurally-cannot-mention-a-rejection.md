---
id: CPE-1984
title: the catalog **rollback** route reports a plain green success and structurally cannot mention a rejection — CPE-1939's defect, one function down
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-28
---

## Summary

Found by **PR #1100**'s independent Reviewer (CPE-1939) while verifying that ticket's consumer
enumeration. It is **CPE-1939's exact shape, one function down**, and it is worse in one respect: the
refresh route *could* be reordered because the counts were there to branch on. This one **structurally
cannot be honest.**

- **`launcher.html:2169`** renders a green `"<agent> rolled back to <tag>."` on `r.applied > 0`, with
  **no rejection branch at all.**
- **`console.rs:1488-1497`** never forwards `regressedRejected` / `integrityRejected` on that route —
  `handle_catalog_rollback` re-emits only `{ indexOk, applied, tag, agents, versionMapUnreadable, error }`,
  so the three rejection counts are **dropped before the launcher can see them.**

So rolling back agent A while agent B's manifest is **mis-signed** reports a plain green success. A
rejected signature is the one outcome that should always be visible.

## Why it was not fixed in PR #1100

Correctly scoped: CPE-1939 names `refreshCatalog` only, and widening a merged-and-reviewed PR to a second
route with its own producer would have been the scope creep this repo keeps refusing. Pre-existing,
measured as such, and explicitly recommended as a follow-up rather than left as a note.

## What this needs

- [ ] **Forward the counts first.** `console.rs:1488-1497` has to carry `regressedRejected` /
      `integrityRejected` (and whatever else the rollback's `ApplyReport` actually populates) before the
      launcher can branch on anything. Check what that report really contains on this path rather than
      assuming it mirrors refresh — **the two routes have different producers**, which is how they
      diverged.
- [ ] **Reuse CPE-1939's decision, do not re-litigate it.** That ticket recorded **worse-news-wins** at the
      site above `refreshCatalog`, consistent with what CPE-1924 chose one branch earlier — *and* it keeps
      the success visible by naming how many agents did update **inside the rejection sentence**. Do the
      same here. If the rollback genuinely needs a different answer, say why at the site.
- [ ] **Mind the denominator.** CPE-1939 had to add `applied` to the regression branch's denominator or
      `1 applied + 3 current + 1 regressed` would claim *"1 of the 4"* and **lose an entry**. Construct the
      equivalent rollback mix and read the rendered sentence; do not compute it on paper.
- [ ] **`integrityRejected` must never be masked by a nonzero `applied`.** That is CPE-1939's hard
      requirement and it applies unchanged here.
- [ ] **Pin the mixed cases in the launcher's jsdom harness** (`src/lib/ai-console-launcher.test.ts`, 87
      tests as of CPE-1939) and **red-proof them by collapsing the branch order**, writing the counts at
      the site. CPE-1939's numbers are the model: restoring the old order gave **85/2**, each failure on
      its *first* assertion — i.e. on the masking itself, not on a count.
- [ ] **Run the CPE-1929 pair on any conjunct you add**, and expect one to be shadowed **by the producer**
      rather than by the chain. CPE-1939 measured exactly that: dropping `r.indexOk &&` from both rejection
      branches left **87/0 — green**, because `catalog.rs:686` sets `report.index_ok = true` only *after*
      three early `return ApplyReport::default()` with `rejected` empty, so `index_ok == false` implies all
      three counts are 0. It was kept anyway because feeding the impossible shape through the real chain
      lands on the conservative refusal branch — **the conjunct fails closed**. Say at the site when a
      green sabotage is *expected*, and say it does **not** cover the others.
- [ ] **Enumerate the readers rather than recalling them** (CPE-1932). CPE-1939's own PR body got this
      wrong and had to be corrected: `broker_client.rs:329-337` **and** `:388-396` both parse the four
      fields as `unwrap_or(0)` passthroughs, and the rollback route is the second one.
- [ ] **Theme tokens only** — the amber/warn state comes from `setMsg`'s state→class mapping
      (`launcher.html:798-803`); no colour literal.

## Notes

Filed 2026-08-28 by the sprint Foreman from PR #1100's Reviewer (CPE-1939), which found it by re-deriving
that PR's consumer enumeration instead of accepting it.

**Unverifiable without a host rebuild:** `launcher.html` is embedded in the ai-console sidecar, so a real
visual pass needs the **host** rebuilt — a launcher swap is not a host swap. CPE-1939's verification came
from the jsdom harness driving the real launcher script plus source-level checks of the Rust producers;
expect the same scope here, and say what remains unseen.

Related: **CPE-1939** (PR #1100 — the refresh route, the worse-news-wins decision, the denominator fix and
the producer-shadowed sabotage), **CPE-1924** (the `==`/`<` split the whole family descends from),
**CPE-1911** (the honest-status work), **CPE-1929** (sabotage pairs), **CPE-1932** (enumerate, don't
recall).
