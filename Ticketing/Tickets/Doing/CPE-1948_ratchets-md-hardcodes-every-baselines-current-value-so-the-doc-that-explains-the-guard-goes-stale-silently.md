---
id: CPE-1948
title: `RATCHETS.md`'s enumeration table hardcodes every baseline's current value, so the doc explaining the guard goes stale the moment any baseline legitimately moves
type: task
priority: Low
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

CPE-1934 (PR #1052) built the guard that makes a raised ratchet baseline loud, and documented all
twelve baselines in `docs/design/RATCHETS.md`. That table carries each baseline's **current value as
a literal**, and nothing ties those literals to the measured ones.

It went stale within an hour of landing. PR #1055 (CPE-1922) recounted the manual-test burndown from
16 to 13 — a **legitimate lowering**, exactly what a ratchet is for — and `RATCHETS.md:102` went on
saying `16`. Corrected by hand on `main` (`ratchet-baselines.mjs print` reports **13**).

**This is the family the guard itself belongs to.** CPE-1933 is filed about provenance claims in
comments; CPE-1932 about rules followed from memory rather than enumerated. This is a stored number
in the document that explains the mechanism for keeping stored numbers honest.

## Why it is Low and not Medium

Nothing enforces the table, so a stale value misleads a reader but cannot make the guard wrong — the
guard reads the real files. The cost is that the one document a person consults to understand the
system can quietly disagree with the system.

## Acceptance criteria

- [ ] **Derive the table's values rather than restating them.** Cheapest shape: a test that runs the
      registry's own measurers and asserts each row in `RATCHETS.md` matches — the same style as
      `sectionDocs.test.ts` and the CPE-1928 Rust→TS derivation guard, both of which already live in
      this repo. Generating the table is also acceptable; asserting it is probably better, because
      the prose around each row is human-written.
- [ ] **Red-proof it both ways**: change a stored value and confirm the test reds naming the row; move
      a real baseline and confirm it reds until the table is updated. A guard only ever seen passing
      is the defect this whole family is about.
- [ ] Include the **not-gated** rows. `manual-test-mvd` is enumerated but deliberately ungated, and it
      is the one that went stale first precisely because nothing gates it.
- [ ] While there: check whether any **other** row is already stale. Recount each from the registry
      rather than trusting the table (CPE-1932). Report what you find even if it is nothing.
- [ ] Consider whether the count of rows should be pinned too — a baseline added to the registry and
      not to the doc is the same defect, one level up, and the registry's completeness test already
      knows how to ask that question.

## Notes

Filed 2026-08-27 by the sprint Foreman after correcting the drift by hand. Found by the merge-order
interaction between PR #1052 and PR #1055 — flagged in advance by both PRs' authors, which is why it
was caught immediately rather than months later.

Family: **CPE-1934** (the guard this documents), **CPE-1933** (provenance claims untested by
construction), **CPE-1932** (enumerate, do not recall), **CPE-1929** (guards that cannot go red).
Related: **CPE-1922** (the legitimate lowering that exposed it).

## Work Log

### 2026-08-27 — measured, then asserted

**The drift, measured first (AC 4).** Recounted all twelve baselines from `REGISTRY` — not from the
table, since the table being wrong is the premise. **One row was already stale:**

| baseline | table said | measurer says | delta |
|----------|-----------|---------------|-------|
| `bidi-render-registry` | 1552 | **1553** | +1 |

The other eleven matched exactly, `manual-test-mvd` (14) included. So the doc had drifted **twice in
one day** — CPE-1922's `manual-test-mvd` (fixed by hand on `main`) and now this one.

**Where the +1 came from, and why nothing shouted.** PR #1056 (CPE-1928) recorded one new render
site, `text:blockedRemedy` in `MacroRunConfirm.svelte`: a real, legitimate raise. The `ratchet-guard`
job never judged it — #1056's last CI run was 16:29Z, the guard landed on `main` at 17:42Z, and
#1056 merged at 18:36Z on those stale checks. Not a hole in the guard's logic: a guard is only as
live as the newest run of the PR it is meant to judge. Recorded in `RATCHETS.md` because the fix
(required, up-to-date checks in branch protection) is not this ticket's.

**Shape chosen: assert, not delete (AC 1).** Two shapes were available. *Delete the numbers and point
at `node scripts/ratchet-baselines.mjs print`* is honest and costs no guard, but the numbers are the
reason anyone opens the page — the *scale* of a debt is what tells you whether an allowlist is a
rounding error or a project, and 1553 vs 0 is not a detail a reader should have to shell out for.
*Assert them* keeps the page useful and costs one test. Asserted. The argument is written into the
document itself so the next person does not have to re-derive it.

**Anchored on structure, not prose (AC 3).** `parseEnumerationTable` in
`scripts/ratchet-baselines.mjs` finds the table by its **exact header row** (refusing 0 or 2+
matches), requires a separator row, and matches every `today` cell **whole** — a bare integer, or a
bare integer plus the literal ` — **enumerated, not gated**` marker, nothing else. Concretely
red-proofed against three ways a lazier scanner passes: a number quoted in a surrounding paragraph
(`hex-files ... stands at 999 today`), a row of the raise ledger further down the same file, and —
the one that is not hypothetical — the cell this table actually carried,
`14 — **enumerated, not gated** (13 → 14 on 2026-08-27, CPE-1946)`, whose leading `14` a
first-number-wins scanner would have read while the tail went unasserted forever. That cell is now
just `14 — **enumerated, not gated**`, and the marker itself is asserted against `unenforced` in both
directions (AC 3 on the not-gated row).

**Row count pinned (AC 5).** The id column is compared to `REGISTRY.map(id)` as an **ordered array**,
so a baseline registered and never documented is a red, as is one documented and never registered.

**Red-proofed both ways (AC 2).**

1. *Stored value wrong, real value right.* Set the table's `bidi-render-registry` back to 1552 →
   `docs/design/RATCHETS.md:113 bidi-render-registry: the table says 1552, the measurer reports 1553`.
   Names the row, the file:line, and both values.
2. *Real value moved, table right.* Bumped `BASELINE_FILES_WITH_HEX` 85 → 86 in `src/app.css.test.ts`
   → `docs/design/RATCHETS.md:105 hex-files: the table says 85, the measurer reports 86`. Green again
   only after the table was corrected to 86.

Both reverted; `git status` clean of them.

**Licence-row mechanism verified intact (AC: do not break it).** With the same 85 → 86 bump in the
tree: `node scripts/ratchet-baselines.mjs compare origin/main` exited **1** naming the undeclared
raise; adding `| hex-files | 85 -> 86 | CPE-1948 | ... |` to the raise ledger in the working tree
alone made it exit **0** ("No ratchet baseline was raised without being declared"). Unit-level too:
the two tables in one document do not read each other, and a row already present at the base is
still refused as a spent licence. All reverted.

**Not a new ratchet.** `src/lib/ratchetsDoc.test.ts` declares nothing ratchet-shaped, so
`ratchetBaselines.test.ts`'s completeness scan (67 tests, green) needs no new registration.

**Also removed:** the two duplicate copies of `hex-files`/`hex-occurrences` in the "Recount" prose
below the table — a second unchecked copy is the exact defect this ticket is about.
