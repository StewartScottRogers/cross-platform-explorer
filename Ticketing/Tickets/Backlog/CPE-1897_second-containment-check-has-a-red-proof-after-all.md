---
id: CPE-1897
title: copy_one_verified's second containment check has a red proof after all — land the 40-deep create_dir_all race probe
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1889's author stated that the second containment check in `copy_one_verified` could not be
red-proofed: it can only differ from the first if the tree changes between them, "a race no
deterministic test can stage". Its reviewer verified the claim — deleting check (2) reddened nothing
across the full 2,392-test suite — and passed it as disclosed defence-in-depth.

**Its Security Auditor then fired it 251 times out of 400.**

The first half of the claim is true. The conclusion was too pessimistic, because the window is not one
operation wide. For a plan entry whose parent is N levels of not-yet-existing directory,
`std::fs::create_dir_all` issues **N failing `mkdir`s descending the recursion before the first
successful one**. At N = 40 the window between check (1) and `mkdir(dst/L1)` is roughly **400 us** —
comfortably wide enough to lose a race into on purpose.

The detector is neat and worth preserving: checks (1) and (2) emit the *same* refusal string, so the
verdict cannot tell them apart — but the filesystem can. If check (2) fired, `create_dir_all` had
already run through the planted junction and left `OUTSIDE/L2` behind. If check (1) fired, nothing
outside exists.

    RACE SUMMARY over 400 trials: refused=400, allowed=0, outside-debris=251, CHECK-(2)-FIRED=251

Every trial refused; none escaped. Check (2) is load-bearing 63% of the time under this shape. The
code is more right than it claims — this ticket is about making that provable rather than asserted.

## Acceptance criteria

- [ ] Land the recipe as an `#[ignore]`d probabilistic test so check (2) stops being untested code.
      Use the deep-`create_dir_all` window (N around 40) and the filesystem-debris detector above, not a
      verdict-string comparison — the two checks deliberately share their message.
- [ ] Assert the safety property, not the race outcome: **every** trial must refuse and none may
      escape. The 63% figure is the probe's sensitivity, not a threshold to assert on — a flaky
      assertion on "check (2) fired at least once" would be exactly the kind of test this repo keeps
      finding and deleting.
- [ ] Confirm the probe still detects check (2) after CPE-1896's atomic rewrite, or retire it
      deliberately with a note if that fix makes the second check redundant.
- [ ] Correct CPE-1889's PR description claim while you are here: it says without qualification that
      "no refusal leaves directory debris outside the root". Under the check-(2) path that is false by
      construction — check (2) refuses *because* `create_dir_all` already built directories through the
      junction, and all 251 winning trials left empty `OUTSIDE/L2/...` directories behind. No bytes, but
      not nothing. The function's own doc bullet is correctly scoped; it is the summary that overreached.

## Notes

Filed 2026-08-26 from CPE-1889's independent security audit. This is a rare shape worth noting for the
sprint's own record: a worker honestly disclosed a limit, a reviewer confirmed the limit as stated, and
a third independent agent disproved the limit by widening the window rather than accepting the framing.

Related: **CPE-1889** (merged, PR #1031), **CPE-1896** (the residual race that does still escape).
