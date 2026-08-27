---
id: CPE-1934
title: every ratchet's baseline can be raised from inside the same diff that violates it — the gate has no gate
type: task
priority: Medium
status: Open
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
