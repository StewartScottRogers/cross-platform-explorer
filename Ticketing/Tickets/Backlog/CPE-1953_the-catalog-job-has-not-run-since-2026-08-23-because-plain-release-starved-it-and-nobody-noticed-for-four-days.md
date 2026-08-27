---
id: CPE-1953
title: the catalog job has not run since 2026-08-23 — plain Release starved it for 23 consecutive runs, and the "fail loudly" guard that was supposed to catch this is untested
type: bug
priority: High
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

**No agent catalog has been published since `v0.57.33` on 2026-08-23.** Every release since then
skipped the catalog job entirely.

Measured by PR #1063's worker across all 60 `release.yml` runs. The `catalog` job is
`needs: release`, and its conclusion tracks the release job **exactly** — no exceptions:

    v0.57.33          run=success    catalog=success   <- last one that ran
    v0.57.33-sidecar  run=cancelled  catalog=cancelled
    v0.57.35 … v0.57.69   run=failure    catalog=skipped   (23 consecutive runs)

On that last successful run, `Detect catalog signing key` → **success**, `Build + sign` → success,
`Upload catalog assets` → success.

**So the signing key was never the problem, and the job is not succeeding at zero work.** This is
**CPE-1917**'s plain-Release breakage starving the catalog downstream — a second, larger consequence
of that bug than the one CPE-1917 was filed for.

## Why this went unnoticed for four days

`release.yml:395` claims CPE-1893 made this job *"run and FAIL LOUDLY at `gh release upload` rather
than skipping quietly."* Every `skipped` above **predates** that change — CPE-1893 landed
2026-08-26 20:25, the last release run was 2026-08-23 — so the claim is **untested**, not false. No
tag has been cut since it landed.

That is worth stating plainly: the guard designed to make exactly this failure loud has never once
been exercised against a real release.

The independent backstop **did** fire: `catalog-freshness.yml` has exactly one run,
`2026-08-27T19:14 schedule failure`. It is flagging this right now.

## Also correct the record

The asset scan says `v0.57.32` is the newest release **carrying** a catalog index — but `v0.57.33`
**produced** one and that release has since been deleted. **Use v0.57.33 as the last good publish.**
(Separately, `v0.57.32` is a *draft*, so no client ever fetched it — see CPE-1941's review.)

## Acceptance criteria

- [ ] **Confirm the chain is actually repaired.** CPE-1917's fix merged today as PR #1048 but **no tag
      has been cut since**, so nothing has proven the plain Release path works end to end. Cut a
      release, or exercise the path deliberately, and verify the catalog job runs **and uploads**.
- [ ] **Exercise CPE-1893's fail-loudly guard for real.** It has never run. Force the condition it
      guards (no signing key on a tag build) and confirm the job fails rather than skipping. An
      untested guard on the release path is the shape this repo has been finding all week.
- [ ] **Decide whether `needs: release` is the right coupling at all.** A broken installer build
      silently stops agent-catalog updates for every user — two failures with different blast radii
      chained to one condition. Either decouple them, or make the skip **loud** (a failing status, not
      a grey one), and record which and why.
- [ ] **Check `catalog-freshness.yml` is reaching someone.** It is failing on schedule right now and
      that failure sat unread. A backstop nobody reads is not a backstop — verify the notification
      path, or say what it should be.
- [ ] Report how long the gap actually was, in days and in releases, once the chain is repaired, and
      whether any client-visible state needs correcting after the resumption.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1063's worker, which measured the run history while
answering an incidental question about published catalog indexes.

Related: **CPE-1917** (the plain-Release breakage that starved it, PR #1048 — merged today, unproven),
**CPE-1893** (the fail-loudly guard, untested), **CPE-308** (the catalog pipeline), **CPE-1951** (the
publish-side lower-bound check, which must handle `latest` having no catalog index at all — which is
true today because of this), **CPE-1941** (the versioning change).
