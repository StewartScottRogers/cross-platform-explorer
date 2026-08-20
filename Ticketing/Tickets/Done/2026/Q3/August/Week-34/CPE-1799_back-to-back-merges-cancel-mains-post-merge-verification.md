---
id: CPE-1799
title: back-to-back merges cancel main's post-merge verification, so the escaped-defect net never runs
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-20
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

## Resolution

`cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}` in **both** `ci.yml` and `gui-smoke.yml`.
The concurrency *group* is left as `<workflow>-<ref>` deliberately — see below.

**Shape chosen: option 1 (the expression), applied to both workflows.** The two alternatives were
rejected on the following grounds, both of which turn on a measurement rather than a preference.

- *Option 2, protect only `ci.yml`.* Rejected because the duplicate-symbol interaction is the
  **example**, not the boundary. `ci.yml` catches interactions that break the compile; the
  `gui-smoke` Linux legs (the blocking gate since CPE-1594) are the only thing that catches an
  interaction which compiles cleanly and misbehaves at runtime in the real built app. Splitting the
  net leaves a hole shaped exactly like the defects hardest to spot by reading a diff. The savings
  did not justify it once the true cost was measured (next bullet).
- *Option 3, a scheduled / quiet-period run of `main`.* Rejected on three counts. (a) Its whole
  premise is bounding cost, and the pending-supersession mechanic already bounds option 1 to ~2
  executed runs per burst, so it buys little. (b) `schedule:` triggers **ignore `paths-ignore`
  entirely**, so a nightly/hourly run would fire against a bookkeeping-only tip — reintroducing
  precisely the waste CPE-1781 removed. (c) It needs its own concurrency group or the very merges it
  backstops cancel it, so it is not actually simpler.

**Runner cost, honestly.** `cancel-in-progress: false` does not mean every push executes. GitHub keeps
at most **one pending run per concurrency group**: a newly queued run cancels any previously *pending*
one, which consumes no runner minutes. A burst of N merges landing inside one CI duration therefore
costs **two** executed runs, not N — the one already running plus one coalesced run for the tip. With
the `Server crates (windows-latest)` leg at ~55 min, essentially any realistic burst fits inside one
CI duration. Accepted trade: intermediate commits get no run of their own, so a red on the coalesced
run does not by itself say which merge caused it — strictly better than the status quo, where neither
they nor the tip get a completed run.

Keeping one group per ref (rather than giving `main` a separate never-cancelling group) is what
preserves CPE-1266's anti-pile-up property on `main`: merges coalesce instead of launching dozens of
parallel matrices. PR-branch behaviour is untouched — `github.ref` is `refs/pull/<n>/merge` there, the
expression is `true`, CPE-1266 applies unchanged. No trigger was modified, so CPE-1781's
`paths-ignore` is intact and still does its job.

## Work Log

- 2026-08-20 — **Scale of the problem, measured.** Of the last 20 `CI` runs on `main` before the fix,
  **12 of the 19 completed ones ended `cancelled`** (e.g. `32366103621`, `32355008851`, `32342553828`,
  `32330120314`, `32326185008`, `32322817124`) — the escaped-defect net was absent for roughly two
  thirds of merged commits, worse than the three consecutive runs this ticket was filed on.

- 2026-08-20 — **Live red-proof on branch `cpe-1799-proof`**, cut from `main` and pushed as a
  four-commit burst. Three workflows ran side by side on every commit, differing *only* in the
  `cancel-in-progress` value, so timing and load are held constant:
  **A** = `true` (today's config on `main`), **B** = the shipped expression with `cpe-1799-proof`
  substituted for `main` so the branch mimics `main`'s protected position (evaluates **false**),
  **C** = the same expression with the comparison **inverted** (evaluates **true**).

  | commit | A (literal `true`) | B (carve-out, false) | C (inverted, true) |
  |---|---|---|---|
  | 1 `7fac4f3b` | `cancelled` | **`success`** | `cancelled` |
  | 2 `9b935d70` | `cancelled` | **`success`** | `cancelled` |
  | 3 `57c9790d` | `cancelled` | `cancelled` **while pending** | `cancelled` |
  | 4 `32fb2316` | `success` | `success` | `success` |

  Run IDs, commits 1→4:
  A `32369323828`, `32369557185`, `32369783783`, `32370010905`;
  B `32369323830`, `32369557171`, `32369783822`, `32370010823`;
  C `32369323815`, `32369557181`, `32369783744`, `32370010890`.

  What this establishes:
  1. **The defect, and the fix.** On commits 1 and 2 the control arm's verdict was destroyed while the
     carve-out arm ran to completion — same burst, same timing, one config keeps the post-merge
     verdict and the other throws it away. Arm B's commit-1 job ran `12:31:55Z → 12:36:58Z` to
     `success`; arms A and C on that identical commit were killed mid-flight.
  2. **The expression is genuinely evaluated in the `concurrency:` block** — the thing worth
     distrusting, since `concurrency` is resolved before much of the run context exists. Arm C is the
     discriminator: it is the same expression shape with the comparison flipped, and it **cancelled**.
     Had the expression been ignored, or the raw `${{ … }}` string coerced to truthy, B and C would
     have behaved identically. They did not, so the evaluated *value* is what drove the behaviour.
  3. **The cost bound is real, not inferred.** Arm B's commit-3 run (`32369783822`) was superseded
     while still `pending` and `GET /actions/runs/32369783822/jobs` returns an **empty `jobs` array** —
     it never allocated a job, so it burned zero runner minutes. That is the mechanic behind the
     "N merges cost 2 runs" claim above, observed rather than assumed.

  **What the proof does NOT establish**, stated plainly: it ran on a branch named `cpe-1799-proof`
  with that name substituted into the expression, not on `main` itself, and the jobs were 5-minute
  `sleep`s rather than the real matrices. It proves the *concurrency semantics* the fix depends on; it
  does not exercise the real ci.yml/gui-smoke job graphs under the new setting. The literal
  `refs/heads/main` comparison first takes effect on the merge commit itself and cannot be rehearsed
  ahead of that without renaming the default branch.

  Proof scaffolding (`.github/workflows/zz-cpe-1799-proof-*.yml`) and the branch were deleted after
  the evidence was captured; nothing from the proof ships.
