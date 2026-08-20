---
id: CPE-1799
title: back-to-back merges cancel main's post-merge verification, so the escaped-defect net never runs
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

`cancel-in-progress` is set on the workflows' concurrency group. On a PR branch that is correct and
deliberate (CPE-1266) — a new push supersedes the old run. On **`main` it means a merge cancels the
previous merge's verification run.**

Observed during the batched sprint of 2026-08-19/20, where merges landed minutes apart:

```
GUI smoke on main:  554b1358 cancelled
                    2b5e969d cancelled
                    7bf52a11 cancelled

CI on main:         2b5e969d in_progress
                    7bf52a11 cancelled     <- superseded 39 seconds later
                    41a9163e success
```

Three consecutive GUI-smoke runs on `main` cancelled, and one CI run cancelled 39 seconds after
starting. So for several merged commits there is **no completed post-merge verification at all**.

## Why it matters

Each PR's own CI was green before merge, so this is not evidence that anything is broken. The problem
is the **safety net**, not the code:

- The post-merge run on `main` is the only thing that catches an **interaction** between two PRs that
  were each green in isolation — the parallel-import duplicate-symbol class this repo has hit before,
  where two branches are individually MERGEABLE and individually green and break once combined.
- It is also the signal the sprint ledger's `post_merge_defect` field depends on. The Foreman
  back-annotates a merged row to `ci-red` when `main` goes red on that commit. If the run is cancelled
  rather than completed, that annotation can never happen, and the metric silently reads clean.

So the faster the crew merges, the less post-merge checking actually happens — exactly backwards.

## What to do

- Make `main`'s runs **non-cancellable** while leaving PR-branch behaviour alone. The concurrency group
  is shared today; give `main` its own group, or set `cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}`.
  Read the CPE-1266 reasoning first — the PR-branch cancellation is deliberate and should stay.
- Weigh the runner cost honestly. Serialising `main`'s runs means several full matrices queued during a
  merge burst, and this repo has already found CI capacity to be its throughput ceiling. A cheaper
  option worth considering: let the *frequent* workflows cancel as now, but protect the one that
  actually catches interaction bugs. Say which you chose and why.
- Consider whether a scheduled verification of `main` — once an hour, or after a quiet period — is a
  better fit than serialising every merge. That would bound the cost while still guaranteeing a
  completed run exists for whatever the tip happens to be.
- **Red-proof it**: merge two commits in quick succession on a branch that mimics `main`'s config and
  show the first run surviving, where today it is cancelled.

## Notes

Filed by the Foreman during the batched sprint, 2026-08-20, after checking `main`'s health following a
run of merges and finding the verification runs were cancelled rather than green. Not a defect in any
merged change — a gap in the process that is supposed to catch defects.

Related: **CPE-1266** (which introduced `cancel-in-progress`, correctly, for PR branches),
**CPE-1781** (the `paths-ignore` work on the same triggers).
