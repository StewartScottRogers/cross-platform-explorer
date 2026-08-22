---
id: CPE-1845
title: a deliberate hold-back and a real failure are distinguishable only by string-matching the message
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

`OpResult` (`crates/server/src/checkpoint_store.rs:151-170`) has **no structural flag** separating a
deliberate hold-back from a genuine failure. CPE-1823's stand-down produces both through the same field.

Measured by the independent Reviewer during CPE-1823's round-4 review, on a staged checkpoint with one
unrestorable key, 200 files added since, and one legitimate restorable entry:

```
applied=1  skipped=201  held_back_deletes=200  survivors=200
```

200 of those 201 results are deliberate hold-backs carrying an **identical paragraph**. A UI can only
tell them apart by string-matching `"not deleted:"`.

## Why it matters

This repo's standing rule — CPE-1804/CPE-1805, CPE-1806, CPE-1814 — is that **a silent skip must not
read as a pass**. The inverse now applies: a deliberate, correct, fail-safe hold-back must not read as
a failure, and 200 of them must not read as 200 separate problems.

There is a second, sharper wrinkle. The recorded UI wording is *"held back, re-run after fixing"*. That
is correct for the **plan-skipped** branch. It is **wrong** for the **checkpoint-keyed** branch, where
re-running on this platform can never help — a Linux capture containing one colon-named file will never
delete-clean on Windows. The user is told to retry something that cannot succeed, and no next step is
offered.

## Acceptance criteria

- [ ] `OpResult` carries a structural discriminant — a field or enum variant — for at least: applied,
      failed, skipped-by-plan (retryable), held-back-by-checkpoint (**not** retryable on this platform).
      Whoever does the UI work needs a field, not a prefix.
- [ ] The 200-identical-paragraphs case collapses to one statement plus a count, per the pill/summary
      conventions already used elsewhere. Do not ship 200 identical rows.
- [ ] The non-retryable branch offers a real next step or explicitly states there is none on this
      platform. It must not say "re-run".
- [ ] A test asserts a consumer can distinguish the four states **without** matching on message text.
      Red-proof it by collapsing two states onto one discriminant and observing red.
- [ ] Check every existing consumer of `OpResult` for message-text matching and convert it.

## Notes

Filed from CPE-1823's round-4 Reviewer findings, which explicitly recommended a separate ticket rather
than absorbing it into a PR already four rounds deep. The Reviewer designed the stand-down being
critiqued here and confirmed the trade is worth it — this is about reporting it honestly, not undoing it.

Related: CPE-1823 (the stand-down), CPE-1806 and CPE-1814 (the same "a skip is not a pass" family).
