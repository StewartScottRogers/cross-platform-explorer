---
id: CPE-1906
title: ci-poll.mjs robustness gaps — a hung `gh` call still crosses the cap, an error reads as pending, and a usage error exits as "CI failed"
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1880's `scripts/ci-poll.mjs` exists so a CI poll **cannot** outlive the harness's 600 s tool cap
and get auto-backgrounded — the mechanism that stalled five agents in one run. Its deadline enforcement
was verified sound across 28 interval × `gh`-cost combinations, every one landing under the cap.

Three gaps remain in the same file, all found by CPE-1880's independent reviewer and all classified
*file, don't block*.

**1. `execFileSync` has no `timeout`, so the deadline bounds the loop but not one call.**
`ci-poll.mjs:367`. The tick loop now checks the real clock before sleeping again, which is what closed
the modelled-bound hole. But a **single** `gh` invocation that hangs is unbounded: one call costing
more than the 120 s safety margin crosses the cap, and at 300 s the run reaches ~630 s and is
backgrounded — the exact failure the script exists to make impossible.
Fix is one line: `timeout: 60_000, killSignal: "SIGKILL"`.

**2. A persistent `gh` failure is indistinguishable from a pending board.**
`ci-poll.mjs:365-369`. A bad auth token, a wrong PR number, or a network failure hits `continue` with
no failure counter, burns the entire 480 s budget, and then reports `CI still pending on unknown` with
exit 2. The caller cannot tell "CI has not finished" from "I could not ask". Bail after N consecutive
failures with a distinct exit code and say which happened.

**3. A usage error exits as "CI failed".**
`ci-poll.mjs:340`. `assertNotBackgroundable` throwing — e.g. on `--interval 0` — escapes `main()` as an
unhandled rejection and exits 1, which the file's own exit-code table (line 40) defines as *CI failed*.
It should be 64, like every other bad-usage path. Bad input reported as a red build is how someone
spends an hour debugging the wrong thing.

Related: UAT independently found that `--interval 0`, a negative `--interval`, and a nonexistent file
passed to `stall-check.mjs` all produce raw Node stack traces rather than the clean one-line usage
message every other bad-input path produces. Same class; fold it in.

**4. A comment is slightly optimistic.** `stall-check.mjs:145` cites *"the lockfile already matches, so
no further action is needed"* as a safe example. Bare, that still trips `no-further-action` (soft) — it
is clean only because the mandated handoff tail excuses it. The comment should say so, since a reader
checking the claim in isolation will find it false.

## Acceptance criteria

- [ ] Bound a single `gh` call, not just the loop. Red-proof it: stub a `gh` that sleeps past the
      margin and confirm the run still returns under the cap.
- [ ] Distinguish "could not ask" from "not finished", with its own exit code, after a bounded number
      of consecutive failures. Red-proof with a deliberately bad PR number and a broken auth path.
- [ ] Every bad-usage path exits 64 with a one-line usage message — no raw stack traces — across both
      `ci-poll.mjs` and `stall-check.mjs`. Include `--interval 0`, negative intervals, and a
      nonexistent input file.
- [ ] Correct the `stall-check.mjs:145` comment.
- [ ] Do not regress the deadline guarantee while doing any of this. Re-run CPE-1880's interval ×
      `gh`-cost matrix afterwards and confirm every combination still lands under 600 s.

## Notes

Filed 2026-08-26 from CPE-1880's independent review (rounds 1 and 2) and its UAT, all of which passed
the PR while recording these.

Also from the same review, kept here rather than given its own ticket:
`src/lib/sprintDispatchAndCiLogGuards.test.ts:60`'s negative assertion
(`not.toMatch(/To watch CI:\s*\`gh run watch/)`) is keyed to CPE-1848's **exact sentence**, not to the
command — so a re-prescription phrased any other way would pass it. The positive `Never run …`
assertion alongside it partly covers this, but the negative one is narrower than it reads. Key it to
the command.

Related: **CPE-1880** (the scripts), **CPE-1907** (the stall detector over-flagging this app's own
background vocabulary).
