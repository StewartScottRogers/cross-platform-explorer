---
id: CPE-1952
title: the catalog staging dir is a predictable path outside the project, and `create_dir_all` succeeds straight onto a pre-existing junction
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`do_fetch_catalog` stages downloaded catalog content in `temp_dir()/cpe-catalog-stage-<pid>`. Three
properties compound:

- **predictable** — `temp_dir()` plus a pid, both guessable by a local process;
- **outside the project**, so none of the containment work of CPE-1896 / CPE-1913 / CPE-1937 covers it;
- **`create_dir_all` succeeds onto a pre-existing junction**, so a local attacker who plants one at
  that path before the fetch has the staged writes land wherever it points.

Raised by PR #1058's Security Auditor as pre-existing and untouched by that PR, and re-raised by PR
#1063's worker, which judged it out of scope there because it is a **different attacker model** —
local filesystem write, no signing key — and the remedy is containment machinery rather than a
validation rule.

## Two corrections to the obvious plan

The Foreman initially suggested reaching for `open_beneath`. PR #1063's worker checked and both halves
of that suggestion were wrong at the time:

1. **`open_beneath` is `pub(crate)` inside `cpe-server`.** `do_fetch_catalog` lives in `src-tauri`, so
   it **cannot reach it at all** without an export decision that is its own piece of design.
2. **`remove_file_beneath` did not exist** when that was suggested — it lands with CPE-1937 / PR #1059.

So this is not a five-line change. Whoever takes it should decide the seam first: export a narrow
containment API from `cpe-server`, move the staging logic into `cpe-server`, or contain it without
`open_beneath` at all.

## Acceptance criteria

- [ ] **Demonstrate it first.** Plant a junction at `temp_dir()/cpe-catalog-stage-<pid>` before a
      fetch and show the staged bytes landing outside. Assert on the **filesystem** — where the bytes
      ended up — not on a verdict. If it turns out something upstream already prevents this, record
      that and close the ticket honestly rather than fixing an imaginary bug.
- [ ] Decide the seam, and record it: does `src-tauri` get a narrow containment API exported from
      `cpe-server`, does the staging move into `cpe-server`, or is it contained another way? This
      decision is most of the ticket.
- [ ] Prefer a **freshly created, exclusively owned** staging directory over a predictable one —
      create-new-or-fail rather than `create_dir_all`, so an existing entry at that path is a refusal
      rather than something to write through.
- [ ] Clean up on every exit path, including refusal. The current code `remove_dir_all`s staging on a
      verify failure; keep that property, and make sure the cleanup cannot itself follow a link out
      (`remove_dir_all` on a junction is exactly the CPE-1937 family).
- [ ] **Red-proof by racing it**, not by reading it. The containment work this belongs beside
      (CPE-1896, CPE-1913, CPE-1937) all found that static fixtures understate the problem by one to
      two orders of magnitude — CPE-1937's static case showed 1 destroyed file where the race showed
      141 per 200 trials.
- [ ] Check whether any **other** temp-dir staging path in the app has the same shape. Enumerate
      rather than recall (CPE-1932).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1058's Security Auditor (residual 3) and PR #1063's
worker, which supplied both corrections above.

Family: **CPE-1896** (the handle-gate origin), **CPE-1913** (the containment gates), **CPE-1937**
(`remove_file_beneath`, PR #1059), **CPE-1940** / **CPE-1949** (the catalog trust engine this staging
serves).
