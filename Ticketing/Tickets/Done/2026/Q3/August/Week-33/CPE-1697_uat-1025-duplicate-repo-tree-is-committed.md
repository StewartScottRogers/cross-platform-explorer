---
id: CPE-1697
title: A whole duplicate copy of the repo — 3,186 files — is committed under .claude/uat-1025/
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-12
closed: 2026-08-13
---

## Problem

`git ls-files | grep -c '^\.claude/uat-1025'` returns **3,186**. An entire second copy of the working
tree — `PURPOSE.md`, `README.md`, `RELEASING.md`, `src/`, `crates/`, the lot — is tracked in git under
`.claude/uat-1025/`. Of the 3,244 tracked files under `.claude/`, 3,186 are this one accidental tree.

It arrived from a UAT scratch directory that was committed by mistake during an earlier sprint (the same
incident recorded in [[janitor-never-rmrf-active-worktrees]]).

## Why it is worth fixing rather than ignoring

It is not inert. It actively distorts work:

- **Every repo-wide code sweep has to know to exclude it.** The CPE-1692 worker's sweep hit 292
  candidate sites and had to explicitly exclude "`.claude/`'s accidentally-tracked duplicate trees" to
  get a true count. A sweep that forgets is a sweep that double-counts, or that "fixes" a dead copy and
  reports the bug closed. Given that the stat-collapse bug family (CPE-1678 / 1687 / 1692 / 1696) has
  now recurred **four times** specifically because sweeps under-covered or mis-scoped, a 3,186-file
  decoy in the search path is a real hazard, not untidiness.
- Grep, IDE search, and `Explore`-style agents all return doubled results.
- It inflates every clone and every checkout.

## Scope

Remove the tracked tree and make sure it cannot come back.

## Acceptance criteria

- [ ] `.claude/uat-1025/` is removed from tracking (`git rm -r --cached` then delete, or a plain
      `git rm -r` — decide and say which, and why).
- [ ] `.gitignore` covers the UAT scratch pattern (`.claude/uat-*`) so the next accident is caught by
      git rather than by a reviewer months later.
- [ ] **Check for siblings before declaring this done.** The CPE-1692 worker referred to duplicate
      "trees", plural. Enumerate everything tracked under `.claude/` that is not a real, intended part
      of the repo (commands, sprint-metrics substrate, research library, qa-architecture are intended)
      and state the exact scope of that enumeration.
- [ ] Confirm nothing real depends on a path inside the duplicate tree — grep the repo for
      `uat-1025` references before deleting, and paste the result.
- [ ] CI green afterwards. A 3,186-file deletion is exactly the kind of change that trips a guard test
      nobody remembers (file-count assertions, docs registries, layout guards), so verify rather than
      assume.
- [ ] Do **not** rewrite history. The files leave the working tree going forward; rewriting published
      history is out of scope and would break every existing clone.

## Notes

Filed by the Foreman from the PR #874 work, 2026-08-12. The CPE-1692 worker flagged it as debris worth
its own cleanup rather than touching it mid-PR — correct call.

Related: **CPE-1692** / **CPE-1696** (the sweeps this tree distorts).
