---
id: CPE-1959
title: two guards disagree about whether a non-surrogate reparse point may be written — `fsutil` writes it, `batch_media` refuses it, and only the refusal is now pinned by a test
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

After PR #1066 (CPE-1929) the codebase states **opposite doctrines** about the same input class:

- **`fsutil::overwrite_confirmed_no_follow` writes** a non-surrogate reparse point. That is CPE-1896's
  rule: a dehydrated cloud placeholder (OneDrive, dedup, WOF) is an ordinary file the user expects to
  be written, and refusing it was the regression CPE-1896 removed.
- **`batch_media::open_output_verified` refuses** it, on the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit —
  and PR #1066's new test now **cements** that verdict.

**This is not a regression.** `batch_media` refused non-surrogates before CPE-1929 too; the reorder
changed which check says so, not the verdict. Both PR #1066's Reviewer and its Security Auditor
confirmed that independently, and the split is documented at the `batch_media` site as **deliberately
unresolved**.

## Why it needs a ticket of its own

PR #1066's site comment points at **CPE-1958** as the place to revisit this. CPE-1958 is scoped to the
`links > 1` TOCTOU race at a *neighbouring* guard — a different problem. Its Reviewer flagged the
mismatch:

> Everything else this ticket deferred got a ticket that *owns* it, with acceptance criteria
> (CPE-1957). The doctrine question gets a pointer at a ticket about something else, so a worker
> arriving via CPE-1958 is working the race and may never read this.

So this ticket exists to own the question.

## What the site comment already establishes, and what it does not

**Established:** the split is real, both halves are named, it is non-regressive, and there is one
substantive asymmetry — *a refused batch item is **skipped** and the user keeps their input, whereas a
refused restore has failed at its only job.* That argument is correctly labelled an argument.

**Not established, and this is the crux, in the comment's own words:** nobody has asked a user whose
batch output landed on a cloud placeholder what they expected to happen.

## Acceptance criteria

- [ ] **Decide which doctrine is right for `batch_media`, and record the reasoning**, not just the
      outcome. The asymmetry above is the starting argument; it is not obviously decisive.
- [ ] **Get the missing evidence, or state plainly that it was not gettable.** What does a user with a
      dehydrated placeholder at a batch-media output path actually experience today — a skipped item
      with a link-shaped message they cannot act on? That is answerable from the code and the message
      text without asking anyone, and it is the input the comment says is missing.
- [ ] If `batch_media` should match `fsutil`, narrow it to `reparse_name_surrogate` **and update the
      test PR #1066 added**, which currently pins the opposite. Do not weaken the test into vagueness;
      replace it with one that pins the new verdict just as hard.
- [ ] If the split is right, **say so at both sites** — `fsutil` currently does not mention that a
      sibling guard deliberately disagrees — and delete the "unresolved" framing so the next reader is
      not re-opening a closed question.
- [ ] Either way, **fix the follow-up hook**: the `batch_media` comment should point here, not at
      CPE-1958.
- [ ] Check whether any **third** site takes a position on this class. PR #1066 enumerated all seven
      `handle_facts` call sites; use that enumeration rather than recalling (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1066's Reviewer (round 2), which called the follow-up
hook "weaker than the rest of the PR's own standard" while approving.

Related: **CPE-1896** (the dehydrated-placeholder rule this rests on), **CPE-1929** (the reorder that
surfaced the split, PR #1066), **CPE-1957** (the three shadowed sites left unmeasured — the same guard
family), **CPE-1958** (the TOCTOU race at the neighbouring guard, which this is *not*).
