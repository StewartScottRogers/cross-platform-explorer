---
id: CPE-1960
title: `macro-param-prompt.smoke.ts` fails intermittently — the context-menu flyout row never appears within 5 s
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

    NEW GUI REGRESSION: "macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro
      opens MacroParamPrompt before any dry-run confirm"
    element (".ctx .flyout .row") still not existing after 5000ms

Observed on **two independent branches** on 2026-08-27 — job `98697809924` (sha `373ee259`) and job
`98705756557` (PR #1068) — with byte-identical output: `14/14 spec file(s) reported, 26 case(s) —
23 passed, 1 failed, 2 skipped/pending`.

It is **not** listed in `gui-smoke/known-failing.json`, and it is **intermittent** — other shard-2
runs the same day reported 14/14 with no failure.

## Why nobody saw it until now

Shard 2 had **two independent failure modes on 2026-08-27**, and they masked each other:

1. **A transport death (CPE-1955)** that killed the shard at spec #2 and reported *"0 new failing
   cases"*. `macro-param-prompt` is spec #6, so on those runs the app was already gone and the spec
   never really ran — it appears in those logs only as cascade noise
   (`newFile → resetFailedRestartingSession` in ~3 ms with a `WebDriverRequestError`).
2. **This genuine failure**, on the runs that *survived*.

Runs that died reported nothing actionable and were re-run. Runs that survived reported **this**, and
were re-run too. So the re-run reflex the CPE-1955 ticket worried about was discarding a **legible,
named regression the ratchet had correctly reported** — not only evidence that had never been written.

**Correction to an earlier claim:** the Foreman first supposed this failure had been hidden *inside*
CPE-1955's swallowed thirteen. It had not — `grep -c 'ctx .flyout .row'` on job `98646323315` is
**0**. PR #1068's worker established that rather than accepting the hypothesis. The two defects are
adjacent, not nested.

## Acceptance criteria

- [ ] **Reproduce it before fixing.** It is intermittent, so run the spec repeatedly and report a rate,
      not a single observation. If it will not reproduce locally, say so and work from CI logs — but do
      not fix on a guess.
- [ ] Establish what `.ctx .flyout .row` is waiting for and why it sometimes does not arrive within
      5 s. Candidates worth ruling in or out: the flyout is opened but empty; the context menu opens on
      a different element; a render the spec does not wait for; or the **CPE-1728 slow-renderer**
      family, which is the same shape that triggers CPE-1955's reset failure two specs earlier.
- [ ] Decide whether the defect is in **the app** or **the spec**, and say which. A spec that waits for
      the wrong thing is a real defect too, but a different one — and this repo has a standing rule
      that a fixture that happens to reproduce is the same defect class as the bug.
- [ ] **Do not add it to `known-failing.json` as the fix.** It is a real intermittent failure in a
      surface users touch. If it genuinely must be deferred, the entry needs a ticket and a reason, and
      the deferral should be argued rather than assumed.
- [ ] Red-proof: whatever the cause, show the failing condition and show it gone, at a rate comparable
      to the reproduction.
- [ ] While there: check whether `macro-param-prompt`'s neighbours in shard 2 share the wait pattern —
      the two `skipped/pending` cases in the same run are pre-existing and unexplained.

## Notes

Filed 2026-08-27 by the sprint Foreman. Surfaced by **CPE-1955** / PR #1068: the attribution fix
turned an illegible `SUITE DID NOT COMPLETE` into `14/14 reported` with a named failing case on its
first CI run. Deliberately **not** exempted to let that PR go green — never exempt the thing your tool
just found in order to land the tool.

Related: **CPE-1955** (the transport death and the attribution fix, PR #1068), **CPE-1728** (the
slow-renderer family), **CPE-1753** (the `incomplete=true ⇒ RED` verdict job), **CPE-1171** (the
gui-smoke harness).

## Raised to High 2026-08-27 — and it may not be intermittent at all

A third occurrence, on **PR #1066** (job at 23:47Z), byte-identical again: `14/14 spec file(s)
reported, 26 case(s) — 23 passed, 1 failed, 2 skipped/pending`, same case, `incomplete=false`.

So it has now been seen on **three unrelated branches** — `373ee259`, #1068, #1066.

**Reconsider the "intermittent" framing.** Every shard-2 run that *actually completed* has reported
this failure. The runs that appeared clean are the ones that died at spec #2 and never reached spec
#6 (CPE-1955's transport death), which reported `0 new failing cases` and were re-run. If that holds,
the spec is failing **consistently** and was simply never visible — which makes the reproduction step
easier, not harder, and changes the diagnosis.

**Check that first**: find any shard-2 run with `14/14 reported` and **no** `macro-param-prompt`
failure. If none exists, the word "intermittent" should come out of this ticket.

**It now blocks the merge queue.** With CPE-1955's attribution fix surfacing it on every complete run,
this is a permanent red on the `gui-smoke-linux-verdict` job for every PR — so it is no longer a
background defect, it is the thing standing between the queue and green.
