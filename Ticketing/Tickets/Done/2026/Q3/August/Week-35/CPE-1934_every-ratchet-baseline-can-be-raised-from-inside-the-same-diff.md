---
id: CPE-1934
title: every ratchet's baseline can be raised from inside the same diff that violates it — the gate has no gate
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

This repo uses **one-way ratchets** to stop a defect class from growing — the hard-coded-hex ratchet
(CPE-1534) is the clearest example, and the `gui-smoke` known-failing list is another. Each stores a
baseline as a plain literal **in the same file it guards**:

    src/app.css.test.ts:267-268
      BASELINE_FILES_WITH_HEX = 85
      BASELINE_TOTAL_HEX_OCCURRENCES = 276

Nothing recomputes an independent "true" baseline to compare against. There is **no `CODEOWNERS`**
file in the repo. No CI step diffs those constants or asks for a justification when they move.

**So a PR that adds a hard-coded colour and raises both numbers in the same diff passes trivially.**
The only backstop is a human reviewer noticing that a number went **up** in a diff that also added
the thing the number counts. That is precisely the move a one-way ratchet exists to prevent, and the
ratchet cannot see it.

Found 2026-08-27 by PR #1049's independent UAT, which verified the ratchet works correctly in both
directions and then asked the next question: *would you notice if this guard broke?*

## Why this is worth fixing rather than trusting review

Two things observed in one night make the "a reviewer will spot it" answer weak:

1. **A raised baseline is the path of least resistance when the failure message names only a
   number.** PR #1049's message said `expected 86 to be less than or equal to 85` — no file, no line.
   A developer under time pressure has to go hunting to fix the real cause, or edit one digit to make
   it pass. (Naming the file is being fixed in #1049; that reduces the temptation but does not close
   the hole.)
2. **The same night produced two PRs blocked by a *false* positive in this very ratchet** (CPE-1931).
   A guard with a history of crying wolf trains people to reach for the baseline.

## Acceptance criteria

- [ ] Enumerate every ratchet-style baseline in the repo — a stored count or allowlist that is
      supposed to only ever shrink. Start with `src/app.css.test.ts`, `gui-smoke/known-failing.json`
      and the token allowlists, then **enumerate rather than recall** (see CPE-1932).
- [ ] Pick a mechanism that makes raising one **loud rather than silent**, and apply it consistently.
      Options worth weighing, cheapest first:
      - a CI step that fails when a baseline constant **increases** relative to the merge base, with a
        message saying the fix is the defect, not the number;
      - `CODEOWNERS` on the files holding baselines, so a raise needs a second pair of eyes by
        construction;
      - moving baselines into a data file whose diff is unmistakable in review.
      A raise must still be **possible** — occasionally it is legitimate — just never quiet.
- [ ] Red-proof it: a PR that raises a baseline must fail or require the extra approval; a PR that
      lowers one must sail through. Both directions, or it is not a ratchet-guard.
- [ ] While there: check whether any baseline in the tree is **already** higher than it needs to be —
      a raise that already happened quietly. Recount each from scratch rather than trusting the
      stored number (CPE-1922 is open on exactly that drift, in the manual-test burndown).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1049's UAT. Explicitly scoped **out** of CPE-1931 so
that PR could land.

Family: **CPE-1929** (a guard that cannot go red because an earlier one answers first), **CPE-1932**
(a rule followed from memory rather than enumerated), **CPE-1933** (a provenance claim that is
untested by construction), **CPE-1931** (a guard matching outside the position that matters). All of
them are the same thing: **a check that looks stronger than it is.** This one is the check that
guards the checks.

## Work Log

**2026-08-27 — implementation.** Rebased on `main` at `826a27ff` (which includes PR #1049/CPE-1931,
already having moved `BASELINE_TOTAL_HEX_OCCURRENCES` 276 -> 277; the ticket body above quotes the
pre-#1049 value). Read the *current* constants rather than the ones quoted here.

**Enumeration — how, not just what.** Four independent sweeps rather than the three-item seed above
(CPE-1932's lesson): (1) the literal word `ratchet` across ts/rs/json/yml/md; (2) every
`toBeLessThanOrEqual` / `toBeLessThan` assertion in the TS tree; (3) declaration-shaped constant names
`const *(ALLOW|EXEMPT|KNOWN|LEGACY|WAIV|GRANDFATHER|BUDGET|BASELINE|IGNORE|SKIP|EXPECTED|MAX)*` across
both TS and Rust; (4) prose markers — "only ever shrink", "one-way", "must not grow", "monotonic",
"burndown". That found **twelve** baselines across nine files, against the three seeded. The Rust side
has none: every `MAX_*`/`KNOWN_*` const there is a runtime cap or a closed vocabulary, not debt owed.
Sweeps (1) and (2) were almost pure noise — the signal was in (3) and (4).

**Mechanism chosen: a CI step diffing baselines against the base revision (the option ranked first
above).** `scripts/ratchet-baselines.mjs` measures every enumerated baseline in the working tree and at
the base revision and reds on an increase. A raise stays legal but must be declared in the same diff as
a row in `docs/design/RATCHETS.md` giving the exact old/new values, ticket and reason — a row whose
numbers do not match the real movement authorises nothing.

`CODEOWNERS` was rejected deliberately: this repo has **no branch protection and no required status
checks** (documented at length in `ci.yml`'s own trigger comment), and a majority of recent commits to
`main` are direct pushes carrying no PR. A CODEOWNERS review requirement is enforced *by* branch
protection, so here it would be decoration — and it fires on *touching* the file, not on raising the
number, so it would also nag on every legitimate lowering. Moving baselines into a data file was
rejected too: it relocates the literal without making anything *check* it, and would have meant
rewriting nine guard tests for zero enforcement.

**Assumptions recorded.** (a) `github.event.pull_request.base.sha` is treated as the merge base. For a
branch cut from `main` these coincide; for a stale branch it is stricter, and a raise that landed on
`main` meanwhile carries its ledger row along with it, so the strictness cannot produce a false red.
(b) The guard runs on pushes as well as PRs, because `main` genuinely moves by direct push here.
(c) `manual-test-mvd` is enumerated but deliberately **not** gated — the MVD legitimately rises when a
QA audit discovers pre-existing unlogged debt (the ledger records a +5 shift on 2026-08-11), and gating
it would push audits toward not logging what they find. Recorded in the registry so it reads as a
decision rather than an oversight.

**Recount (last acceptance item) — nothing was already inflated.** `hex-files` / `hex-occurrences`:
both baselines temporarily set to `0` and the ratchet re-run so its own matcher reported the truth —
85 and 277, exactly the stored values, zero slack. The eight allowlists each carry a "no stale entries"
test and all eight are green, so every entry still points at debt that is really there;
`bidi-render-registry` asserts exact equality with the tree and so cannot be inflated by construction.
`gui-smoke-known-failing` (25) cannot be recounted off-CI, but its clause-2 check reds the job the
moment a listed case starts passing, so a no-longer-needed entry fails every run rather than sitting
there. `manual-test-mvd` (16) matches its own header; the drift inside that ledger's body is CPE-1922's.

**Red-proofed both ways** with real runs of the exact script CI runs — output pasted in the PR body.
`evaluate` is pure, so both directions are also driven as unit tests rather than observed once by hand.

**2026-08-27 — review round 2 (CHANGES REQUESTED on PR #1052).** Three findings, all the same shape:
**the guard went green while a baseline was raised** — the exact defect this ticket exists to close.
Rebased on `main` at `f15d9f29` first. The Reviewer independently re-derived the enumeration with a
wider vocabulary and found nothing missed, re-did the recount (85 / 277), and verified the CODEOWNERS
argument as fact rather than taking it on trust; all of that stands. What did not stand:

- **F1 — the measurers misread rather than fail, and misread in the direction that passes.**
  `const BASELINE_TOTAL_HEX_OCCURRENCES = 200 + 78;` is a real 277→278 raise; the old
  `=\s*(\d[\d_]*)` took the first integer, measured 200, and printed `277 -> 200 LOWERED` at exit 0.
  Same class on the array side: four `KNOWN_GAPS_ALLOWLIST` entries replaced by `...MORE_GAPS`
  (6 names) is a real 14→17 and reported `14 -> 12 LOWERED`. Fixed: `numericConst` requires the
  **whole** initialiser to be one integer, `splitTopLevel` throws on a spread, and a literal that is
  not the whole initialiser (`[...].concat(X)`) is refused. **Why it slipped is the lesson:** round 1
  proved the *safe* variant (a plain rename reds) and never tried the dangerous one.
- **F2 — a ledger row was a permanent, reusable licence.** Every doc said "the same diff adds a row",
  but the code read only the working-tree ledger. Commit the row at the base with no baseline change,
  raise later, exit 0 under someone else's ticket — realistic, since `hex-occurrences` went 276→277
  last week. Fixed: the ledger is parsed at the base ref too, and a row present there authorises
  nothing. The overstated claim in `CLAUDE.md` and `RATCHETS.md` is now true rather than removed, and
  a test asserts both docs and the code agree.
- **F3 — base-side unmeasurable was a silent green while head-side was red**, so a rename reset the
  ratchet. Fixed: git's own rename detection is followed at the base ref (which catches the
  Reviewer's `git mv` input and turns it back into a visible 14→17 raise), and anything still
  unresolved is an error that must be declared as `| id | new -> N | CPE-NNNN | why |`.

Cheap fixes in the same round: `main()` can no longer emit a bare Node stack trace; `git show`'s
expected "fatal: path does not exist" no longer leaks into the CI log beside real `::error::` lines;
`RATCHET_SHAPED` gained 15 more names (OFFENDER, SUPPRESS, TOLERAT, WAIVER, OPTOUT, EXEMPT, EXCLUD,
DEBT, GRANDFATHER, LEGACY_, REGISTRY, CEILING, THRESHOLD, PENDING, EXISTING) so a future
`const FOO_OFFENDERS` in a **new** file cannot escape — `APP_MARKUP_OFFENDERS` had been covered only
by the accident of sharing a file with an `*_ALLOWLIST`; the non-vacuity floor now derives from the
registry (the scan must match every registered file and every excluded file) instead of a magic 8;
and the counts-not-identities limitation moved from the PR body into `RATCHETS.md`.

Every sabotage is now a permanent fixture in `src/lib/ratchetBaselines.test.ts` under a
`SABOTAGE F1/F1b/F2/F3` block, with the Reviewer's exact input quoted in each comment — they belong
in the test table, not in a transcript. 53 tests in that file; 4745 across the suite.

**2026-08-27 — review round 3 (CHANGES REQUESTED again).** Round 2's three fixes all held under
re-run, but the narrow re-review found two more. Rebased on `main` at `70c6d7db` first.

- **R2-F1c — F1 a third time, in a different costume.** The measurers were strict about the
  *initialiser* but the declaration **search** still ran on raw source and took the first match:
  `stripComments` was only ever applied to the captured value, never before the search. `[ \t]*`
  before `const` made a `//`-commented decoy safe, but a `/* … */` block or a template literal was
  not — a real 277→278 raise read as `277 unchanged`, exit 0, with vitest fully green. Same on the
  array side (a live 5-entry array measured 2; a live 3-entry array measured 1), always in the
  direction that turns a raise into a lowering. **Two-part fix**, and the second half is the durable
  one: `maskNonCode()` blanks comment bodies and string/template interiors to same-length spaces
  before any search (so indices stay valid and only live code is visible), and `findSoleDeclaration()`
  makes **more than one matching declaration a red in itself** — that removes the question "which one
  did I pick?" rather than answering it, so it also covers decoy shapes no masker understands.
  Masking incidentally fixed a latent hazard in `recordOfArraysTotal`, where a `[` inside a KEY string
  could be mistaken for the value array's opening bracket.
- **R2-F2b — round 2's fix over-corrected.** `licence()` asked whether the base ledger *contained* a
  row for the movement, so the same `from → to` could never legitimately happen twice: a base row
  `277 -> 278 | CPE-1111` blocked a fresh 277→278 declared under CPE-2222, telling the author to do
  what they had already done and leaving deletion or falsification of the historical row as the only
  way through. That is the realistic path here — hex went 276→277 the week before. Rows are now
  **counted, not found**: authorised when the working tree holds strictly more rows for that
  `(id, from, to)` than the base did. The error text says APPEND and says the historical row is kept.
  The round-2 test that "passed" this scenario only covered a *different* `(from, to)` pair, which is
  why it missed.

Also this round: the derived non-vacuity floor could itself be made vacuous by narrowing `SCAN_ROOTS`
(the requirement was filtered by the same list), so `SCAN_ROOTS` must now provably **cover** the
registry *and* cover a non-trivial number of files — round 1's absolute floor would have caught this
head-on, which is worth remembering before replacing an absolute check with a derived one.
`RATCHETS.md` property 3 was updated **with** the fix rather than left over-claiming (the same
doc-ahead-of-code shape flagged in round 1), and the scanner's untracked **regex literals** are now
documented, with the reasoning for why that gap fails closed: an unmasked regex can only *add*
apparent entries, which over-reports debt.

**Process note worth keeping.** Twice this ticket I unwound a throwaway red-proof commit with
`git reset --hard` and took real, uncommitted fixes with it (recovered both times — once by redoing
the edits, once from the reflog). When a temp commit is used to stage a red-proof, unwind it with
`--soft` and restore only the sabotaged files.

67 tests in `ratchetBaselines.test.ts`; 340 files / 4759 across the suite.

## Closed 2026-08-27 — merged as PR #1052, after three rounds and four all-green bypasses

**Reviewer APPROVE.** The final re-review ran every sabotage from all three rounds against the
**pushed head as fetched** (`332ec6b1`, sha verified before and after each block) rather than a local
copy — because the author had disclosed losing uncommitted work twice to `git reset --hard`.

**What shipped:** `scripts/ratchet-baselines.mjs` plus a `ratchet-guard` CI job (**0.65 s**) that
measures every baseline in the working tree *and* at the base revision and reds on an increase,
unless the raise is declared in `docs/design/RATCHETS.md` with exact old/new values.
**Twelve baselines across nine files**, found by four independent sweeps — the Reviewer re-derived the
enumeration with a wider vocabulary and found **nothing missed**, and confirmed the Rust side has none.
A recount from scratch showed **zero slack** (85 / 277, identical to stored).

## The four all-green bypasses, and the pattern behind them

Every one let the guard pass while a baseline was raised. Each is now a permanent fixture quoting the
input that produced it.

| round | bypass | before | after |
|---|---|---|---|
| 1 | `= 200 + 78;` (real 278) read as `277 -> 200 LOWERED` | exit 0, 52/52 pass | exit 1 |
| 1 | a `...SPREAD` replacing four array entries counted as one | exit 0 | exit 1 |
| 1 | a ledger row **committed at the base** acted as a permanent reusable licence | exit 0 | exit 1 |
| 1 | base-side unmeasurable was green while head-side was red, so a `git mv` reset a ratchet | exit 0 | exit 1 |
| 2 | a **block-comment or template-literal decoy** outranked the live constant, because the *search* ran on raw source and took the first match | exit 0, 72/72 pass | exit 1 |
| 2 | the fix for the base-ledger bypass **over-corrected** — the same `from -> to` could never legitimately happen twice | exit 1 (wrongly) | exit 0 |

The author named the pattern better than anyone: *"every hole in this guard has been a measurer that
returned a **wrong number** rather than refusing. I strictened the initialiser twice while leaving the
**search** naive — the same mistake at one remove."*

The durable fixes are the structural ones: `maskNonCode()` blanks comment bodies and string/template
interiors before any search, and `findSoleDeclaration()` makes **more than one match a red in itself**
— which turns "which of these did I pick?" into a question that cannot arise. The Reviewer ran 24
mask breakers: **20 correct, 4 red, 0 wrong numbers**, and showed structurally why a wrong number now
needs *two* simultaneous faults (a live declaration hidden **and** a non-live one revealed).

## Two things the guard's own guards taught

- **Replacing an absolute check with a derived one can hand the check's inputs to whoever wants it to
  pass.** Round 2 replaced a hardcoded `>= 8` floor with one derived from the registry; the Reviewer
  then showed `SCAN_ROOTS = ["src/lib/preview"]` made it pass **vacuously**. Round 1's absolute would
  have caught that head-on. Both halves are now asserted.
- **The docs kept getting ahead of the code.** `RATCHETS.md` property 3 asserted "never a guessed
  number" while the decoy bypass was live. Fixed *with* each fix, and a test now reads CLAUDE.md and
  RATCHETS.md and asserts they state the base-ledger rule.

Known and documented limitation: the guard measures **counts, not identities**, so removing one
offender and adding a different one leaves the count flat. Recorded in `RATCHETS.md` under
"What this guard does *not* catch".
