---
id: CPE-1934
title: every ratchet's baseline can be raised from inside the same diff that violates it — the gate has no gate
type: task
priority: Medium
status: In Progress
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
