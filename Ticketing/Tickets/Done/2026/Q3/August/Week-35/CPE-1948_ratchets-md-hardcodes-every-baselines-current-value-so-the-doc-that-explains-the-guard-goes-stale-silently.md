---
id: CPE-1948
title: `RATCHETS.md`'s enumeration table hardcodes every baseline's current value, so the doc explaining the guard goes stale the moment any baseline legitimately moves
type: task
priority: Low
status: Done
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

### Round 2 (Reviewer: APPROVE, two non-blocking fixes)

Rebased onto `origin/main` at `fd0bf183`. Main had moved four times since the branch was cut; every
one of those four commits is `.md`-only (three touch ticket files, one the sprint-metrics log), so no baseline should
have shifted — confirmed rather than assumed: `compare origin/main` prints all **12 unchanged**,
`bidi-render-registry` measuring 1553 on both sides.

**F1 — the raise ledger no longer contradicts the page. Decision: no retroactive row; the reason is
written at the site instead.** The ledger said `_(none yet)_` while the recount six sections up
recorded `bidi-render-registry` moving 1552 → 1553 under CPE-1928 — an unguarded claim going stale
inside the page about unguarded claims going stale. Both answers were defensible and the Reviewer
confirmed the row is free (`compare origin/main` exits 0 with it, 80 ratchet tests green), but a row
is a **licence**, not a history entry, and the two conditions printed directly above that table make
one meaningful only inside the diff that performs the raise. A row added now satisfies neither: the
movement already merged, so base and head both measure 1553 and the licence can never be consumed —
and it would be the only row on the page that is false by the page's own definition. It would also
cost the reading that makes an empty ledger worth anything — *no raise got past the guard* — because
once rows can appear retroactively, their absence stops meaning that. The paragraph now says all of
this immediately above the table.

The same edit stops the recount re-explaining how #1056 got past the guard and points at **CPE-1970**
instead, adopting that ticket's corrected framing: the decisive fact is that `ratchet-guard` is
**absent from `ci.yml` at #1056's head SHA** (grep 0), not that its last run was old — the run
timestamps in the previous wording were misleading, since that run finished one minute before the
merge.

**F2 — `parseEnumerationTable`'s two structural gaps now say what actually closes them.** The scan
takes consecutive table-shaped lines, so a blank line mid-table silently **truncates** the row list
and an adjacent four-column table has its rows **absorbed**. Verified here rather than taken on
trust: injecting a blank line before the `mojibake-allowlist` row cut the parse from 12 rows to 6,
reddening the **ordered id comparison** in `ratchetsDoc.test.ts` (12 expected, 6 received) with the
not-gated non-vacuity check as a second net — and the `today` assertion **not** firing, because the
six surviving rows were each still correct. Absorption also probed: it needs an intruder whose cells
are themselves row-shaped; the raise ledger's own header throws on its bare `baseline` cell. The
comment at the site now names the id comparison as the check doing the work, so the next reader does
not read the parser as airtight and delete it as redundant.

**F3 — no action, as advised.** Dropping `(13 → 14 on 2026-08-27, CPE-1946)` from the `manual-test-mvd`
cell is the correct trade: it was the load-bearing example of the defect, and the history survives in
`MANUAL-TEST-BURNDOWN.md`'s own CPE-1946 section with both tickets named in the surrounding prose.

**Gates.** `npm run check` **0 errors, 0 warnings**. `npm test` **350 files, 5016 passed, 2 skipped**.
`node scripts/ratchet-baselines.mjs compare origin/main` — 12 enumerated, all unchanged, exit 0.
Byte-wise line-ending check after the edits: `RATCHETS.md` 206 CRLF / 206 LF / 0 bare CR / no BOM
(was 191/191; +19 / -4 lines), `ratchet-baselines.mjs` still pure LF (1143/1143 with 0 CRLF),
`ratchetsDoc.test.ts` untouched at 216/216 — `--numstat` 19/4 and 12/0, not a line-ending rewrite.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1081, **fully green (25/25)**, after two rounds.

**The doc was already stale within a day of the table landing** — the second such row in 24 hours.
`bidi-render-registry` read **1552** against a measured **1553**; the other eleven matched. Its
Reviewer wrote its **own** bracket-depth scanner (a different algorithm from the measurer's) and
recounted all twelve from the guard files directly, confirming exactly one stale row and **no second**.

**Assert over delete, and the deciding argument was not the obvious one.** Keeping the numbers costs a
guard; deleting them costs nothing. The Reviewer's endorsement is the reason to prefer assert:
***delete does not remove the failure mode*** — someone re-adds the numbers within a month because the
page is worse without them, and they are unguarded again. The scale of a debt is what distinguishes a
rounding error (`warn-token-allowlist`, 0) from a project (`bidi-render-registry`, 1553), and a page
that hides that behind `node scripts/ratchet-baselines.mjs print` is honest and useless.

**The whole-cell match is load-bearing, and the proof is in the data it had to parse.** The real
pre-PR cell read `14 — **enumerated, not gated** (13 → 14 on 2026-08-27, CPE-1946)` — a
leading-digits scanner **would** have read `14` and left the qualifier unasserted forever. The Reviewer
ran **15 value-cell attacks and 7 structural ones**: bold, parenthetical, comma-grouped, `+`-prefixed,
HTML comment, zero-width space, nbsp, fullwidth digits, backticks, footnote marker, empty cell, escaped
pipes, a second same-header table (including inside a fenced block), a row split across lines — **all
refused**. Only benign normalisations accepted.

**Registered-but-undocumented tested three ways**, including **reordered** rows, confirming the id
comparison is an ordered array and not a set.

**Where the stale row came from is bigger than the row** — and became **CPE-1970 (High)**. PR #1056's
last CI run was created 16:29Z, `ratchet-guard` landed on `main` at 17:42:59Z, and #1056 merged at
18:36:20Z, so **that guard never judged it** and its legitimate 1552 → 1553 raise went in undeclared.
The Reviewer confirmed all three timestamps and found the decisive fact: **`ratchet-guard` is absent
from `ci.yml` at #1056's head SHA — grep count 0.** It *could not* have judged it. Two corrections came
with that: the "16:29Z run" actually **finished at 18:35:13Z, one minute before the merge**, so a
recency check would have waved it straight through; and a partial re-run of GUI smoke at 17:47Z did not
help, because `ratchet-guard` lives in `ci.yml`.

**Round 2 declined the retroactive licence row, and the argument is the keeper.** A row is a
**licence**, not a history entry — meaningful only inside the diff that performs the raise. The
movement had already merged, so `compare origin/main` sees **1553 on both sides**: the licence is
***unconsumable by construction***, not merely unused. And an empty ledger's entire value is the
reading *"no raise got past the guard"* — once rows can appear retroactively, their absence stops
meaning that.

**Two structural parser gaps were found and correctly attributed to what closes them:** a blank line
mid-table silently truncates the row list, and an adjacent 4-column table gets absorbed. Both are
caught — but by the **ordered id comparison**, not by the parser. Said at the site, so the next reader
does not remove the check actually doing the work.
