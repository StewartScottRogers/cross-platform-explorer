---
id: CPE-1935
title: one unwritable file aborts a whole extraction, leaving a half-extracted folder and a single error string with no record of what landed
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Extracting an archive on top of a **read-only file** that already exists at the destination — a very
ordinary case, e.g. re-extracting over files you had locked down — aborts the **entire** extraction.

Measured 2026-08-27 by PR #1050's independent UAT, on a 27-entry fixture:

    Err: "...the path component \"existing.txt\" could not be opened for writing
          (Access is denied. (os error 5))..."

    -> 23 of 27 entries were already on disk
    -> no per-entry report of what succeeded

So the user is left with a **half-extracted folder**, one error string naming a single file, and no
way to tell what did or did not land. Re-running is the only recourse, and nothing tells them that.

## This is pre-existing, not a CPE-1913 regression

The UAT went out of its way to establish this. The surrounding code carries an explicit, pre-existing
rule — *"a refusal is a skip when it is a verdict and an abort when it is an I/O failure"* — which
predates CPE-1913 and is unchanged by it. The old `fs::File::create` would have failed identically.
It does not count against that PR and did not block it.

## Why it is worth fixing

Compare the **sibling legs**, which this repo has spent the night making honest:

- **Backup** reports per-entry `OpResult`s, so a refused file is one loud row among many successes.
- **Revert** (CPE-1881) groups its refusals, keeps a short line per path, and distinguishes transient
  from permanent so the user knows whether re-running helps.
- **Transfer** (CPE-1881) returns a `skipped` list rather than staying silent.

Extraction is now the odd one out: all-or-nothing on an I/O failure, with the "nothing" part untrue —
23 files really were written. That is the **silent-partial-success** shape, which is a close relative
of the silent-success shape CPE-1896 and CPE-1913 exist to eliminate.

## Acceptance criteria

- [ ] Decide, and record, what the contract should be: **abort-and-roll-back** (leave nothing), or
      **continue-and-report** (a per-entry result set like backup's). Do not leave it as
      abort-and-leave-the-mess. Weigh it against the existing "verdict = skip, I/O failure = abort"
      rule rather than around it — if that rule is right, then the abort must clean up after itself.
- [ ] Whichever is chosen, **the user must be able to tell what landed.** If the run continues, give
      it a per-entry report in the shape CPE-1881 established for revert and transfer. If it aborts
      and rolls back, say so explicitly in the message so "nothing was extracted" is true.
- [ ] Distinguish **transient from permanent**, as revert now does. A read-only file and a file
      locked by another process are both retryable after user action; a malformed archive is not.
      Telling the user which decides whether re-running is worth anything.
- [ ] Cover **all** the extraction legs, not just zip: `tar_unpack_with` and `extract_7z_safe/_stream`
      share the shape. Note CPE-1913 deliberately left those two untouched for a different reason
      (they need a third-party unpacker replaced) — check whether that blocks this too, and say so.
- [ ] Pin it with a test that goes red on a half-extracted folder with no report. This repo's
      recurring defect is guards that prove nothing; an assertion on the `Err` string alone would be
      one of them — assert on **the filesystem**.


## Claim narrowed 2026-08-27 — "pre-existing" is true for a read-only or directory occupant, NOT for a link

This ticket was filed asserting the whole-run abort is pre-existing. PR #1050's Security Auditor
measured both versions and found that is **only partly true**:

    main   [zip junction->outside]  Ok((done 1, skipped 1, [...is a link...]))   second entry delivered = true
    branch [zip junction->outside]  Err("refusing to write ... could not be opened for writing")  delivered = FALSE

A **link** at an entry's name went from `Ok` + per-entry skip to `Err` + half-extracted folder — a
genuine regression from CPE-1913, not pre-existing. A **read-only file** or a **plain directory** at
the leaf aborts on both, which is the case this ticket was filed for and which remains pre-existing.

**The link half has already been fixed in PR #1050** (round 2), by classification rather than errno:
on a leaf-open failure the walk makes one more `NtCreateFile` as a directory and asks
`name_surrogate_at`. Its author also noted this restored **parity with the Unix arm**, which has
always classified through `link_at` — so it was a per-platform divergence, not only a regression.

The plain-directory case was **deliberately left aborting**, because `main` aborts for that too and
widening it would have exceeded the regression being fixed. So this ticket's scope is now precisely:
**a read-only or otherwise unwritable occupant, and a plain directory occupant** — the cases where
one entry aborts the run and leaves a half-extracted folder with no per-entry report.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1050's UAT, which added this check beyond its brief
and then read the code to establish it was pre-existing before reporting it. Related: **CPE-1913**
(where it was observed), **CPE-1881** (the per-entry reporting shape to copy), **CPE-1896** (the
silent-success family this belongs to), **CPE-1779** (a partially-written extraction that Agent Watch
never records — same leg, adjacent symptom).
