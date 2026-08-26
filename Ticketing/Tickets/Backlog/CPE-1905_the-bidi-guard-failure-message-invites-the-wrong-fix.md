---
id: CPE-1905
title: the bidi guard's failure message invites the wrong fix, and misdescribes a duplicate-count drop as a removal
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1885 fixed the bidi-escape guard's registry keying and its mechanism is sound — reformatting no
longer breaks it, new offenders and stale entries both red correctly, and the multiset genuinely
catches a duplicate count changing. Two problems are left in what it *says* when it fires.

**1. The message offers the wrong fix as an equal option, and never says why it matters.**

On a new raw render the guidance reads: *"wrap a genuinely new offender in
`displaySafeName`/`displaySafePath`, **or** update `REGISTRY` here…"* — two remedies, equally weighted,
and nothing anywhere explains the threat.

The threat is real and specific: a filename carrying bidirectional-text control characters can *display*
as something other than what it is. That is the entire reason this guard exists. A developer who has
never met it, under time pressure, reading two options where one turns the test green in five seconds,
will take the five-second one — **registering an actual vulnerability into the allowlist** and calling
it done. The guard would then be green, correct by its own rules, and protecting nothing at that site.

CPE-1885's own ticket text raised this as a "Consider" rather than an acceptance criterion, so its UAT
correctly passed the PR while flagging it. It is worth closing properly.

**2. A duplicate-count drop is reported as a removal, which is false.**

Some components render the same expression twice (TrashView has two `$t("trash.moreActions")`). Delete
**one** of the two and the guard reds with:

    TrashView.svelte: STALE recorded expression(s), no longer rendered raw: $t("trash.moreActions")

The expression is still rendered — once, at line 360. The message says it is *"no longer rendered
raw"*, which is simply not what happened. To work out that the count went 2→1 rather than 1→0, a
developer has to hand-diff the `found` array against the `recorded` array inside a wall of roughly 28
other expressions.

The mechanism is right; the wording actively misleads about which situation you are in — and the two
situations call for opposite fixes (delete one registry line, versus delete both).

## Acceptance criteria

- [ ] Lead the new-offender message with the **why**, in one sentence, before either remedy: a raw
      render lets a bidi-spoofed filename display as something it is not. Then present wrapping as the
      default and registry-update as the exception that needs a stated reason.
- [ ] Make registering a new offender cost something deliberate — require a comment, a ticket
      reference, or a dated note beside the entry, so "make it green" is not the path of least
      resistance. Decide the shape and record why.
- [ ] Distinguish a count change from a removal in the message itself: say that the expression is still
      rendered N times but recorded M times, and name both numbers. Do not make the reader diff two
      arrays.
- [ ] Red-proof all three message paths and paste the new text: new offender, full removal, and
      duplicate-count drop. The test for this is whether someone reading only the message can tell the
      three apart — say so in the Work Log rather than only asserting on substrings.
- [ ] Do not weaken the mechanism while improving the prose. CPE-1885's multiset comparison and its
      expression-text keying stay exactly as they are; re-run its acceptance cases afterwards.

## Notes

Filed 2026-08-26 from CPE-1885's independent UAT, which passed the PR (all five acceptance criteria
genuinely met, verified case by case) and flagged both of these as usability sharp edges rather than
correctness failures. That framing is right — this is not a defect in CPE-1885's work, it is the next
increment.

Related: **CPE-1885** (the re-keying), **CPE-1712** (`cpe-1712-bidi-filename-spoof`, the underlying
threat), **CPE-1757** (`cpe-1757-bidi-guard-test`), **CPE-1771** (manifest mojibake guard).

Worth pairing with a look at `src/docs/03-explorer.md`'s "Not yet covered" list, which the current
message already points at — if a developer is being told to disclose a gap there, the docs side of that
flow should be as clear as the test side.
