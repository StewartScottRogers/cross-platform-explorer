---
id: CPE-1956
title: `ci.yml`'s five test jobs all sit behind `lockfile-preflight` with no `if:`, so one preflight failure silently skips the entire test suite
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1064's worker while enumerating every `needs:`-chained job in the repo (CPE-1932 —
enumerate, do not recall). It is the **second instance** of the shape that starved the agent catalog
for 33 days, in a different workflow.

`ci.yml`'s five jobs — `backend`, `crates`, `net-e2e`, `sidecar`, `msrv` — all declare
`needs: lockfile-preflight` with **no `if:`**. If the preflight fails, all five are **skipped**, not
failed. The whole test suite silently does not run.

## Why this is not the same severity as CPE-1953, and why it is still worth fixing

Nothing here publishes, so the blast radius is smaller than a catalog that stops reaching users.

But there is a specific hazard: **GitHub counts a skipped required status check as satisfied.** If any
of those five is (or becomes) a required check, a preflight failure could let a PR read as *mergeable*
with its test suite never having run.

**Important mitigating fact, verified independently this week:** this repo currently has **no branch
protection at all** — PR #1052's reviewer measured `branches/main/protection` → 404 "Branch not
protected" and `rulesets` → `[]`, with a majority of recent `main` commits being direct pushes. So
nothing is required today and the hazard is **latent**. That is exactly why it should be fixed now,
cheaply, rather than after someone turns protection on and inherits a silent hole.

Note also that a Foreman merging on a `ci-poll.mjs` verdict reads `pending`/`failure` counts — a
**skipped** job is neither, so this shape is invisible to the merge gate this crew actually uses.

## Acceptance criteria

- [ ] Add a terminal `if: always()` verdict job over the five that **fails** when any of them did not
      run, in the shape `gui-smoke-linux-verdict` (CPE-1753) already uses — that job exists precisely
      because "everything else happened to pass" is not the same as "everything ran".
- [ ] **Red-proof it**: force `lockfile-preflight` to fail and confirm the verdict job goes **red**
      rather than grey, and that the five skipping is visible in its message. Both directions — a
      genuine all-green run must still pass.
- [ ] Decide whether `needs: lockfile-preflight` is a real **data** dependency or just ordering. PR
      #1064 kept `catalog`'s `needs: release` because it genuinely needs the release object
      `tauri-action` creates; if the preflight produces nothing the five consume, decoupling may be
      simpler than a verdict job. Record which and why, in the workflow rather than only the PR.
- [ ] Extend the `needs:`-chain ratchet PR #1064 added so this instance is **recorded with a verdict**
      rather than left unclassified. That ratchet derives all 11 chains at run time; it should red if a
      new unguarded chain appears.
- [ ] While there: confirm the other two chains PR #1064 marked **accepted** (`release-sidecar`,
      `gui-smoke-linux`) are still correctly accepted after this change.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1064's enumeration, which found it and deliberately
did **not** fix it there — different workflow, different blast radius, and folding it in would have
made a release-plumbing PR touch the whole CI suite.

Related: **CPE-1953** (the same shape starving the catalog for 33 days), **CPE-1753** (the
verdict-across-all-shards job this should copy), **CPE-1932** (enumerate, do not recall — the sweep
that found it), **CPE-1934** (the ratchet registry).
