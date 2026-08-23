---
id: CPE-1848
title: workers stall waiting on background notifications they can never receive
type: task
priority: High
status: Done
tags: ready
estimate: S
created: 2026-08-21
closed: 2026-08-23
---

## Problem

A worker sub-agent does **not** receive background task notifications. When one backgrounds a CI
watch, arms a monitor, or otherwise defers to an asynchronous signal, it returns a stub and waits
forever for a wake-up that will never arrive.

Observed **three times in a single batch**, all on 2026-08-21, each from a different worker on a
different ticket:

- *"Server-crates legs are queued. The monitor will report each as it lands."*
- *"CI is running on PR #989... A monitor is armed and will report each check as it lands. Holding for
  those results before I close out."*
- *"I'll wait for the background CI watch task to finish before reporting results."*

Every one had done the actual work. Every one returned nothing usable. Each cost a full round-trip to
recover, and each recovery required the Foreman to re-enumerate the report it wanted — because the
worker's context still held the findings, but nothing had asked for them.

## Why High

It is cheap to fix and it wastes an entire dispatch every time it fires. Worse, the stub reads like
progress: *"a monitor is armed"* sounds like the work is proceeding, so a Foreman that does not know
this failure mode will wait on it. That is the same **fails-by-succeeding** shape this repo keeps
closing everywhere else — CPE-1806, CPE-1814, CPE-1815, CPE-1780. A stalled worker is a silent skip
wearing the costume of a pass.

It is already recorded in the sprint memory (*"sub-agents run work synchronously"*), which means the
knowledge exists and is not reaching the dispatch. Memory that does not reach the prompt is not a
control.

## Acceptance criteria

- [ ] Every worker/reviewer/UAT dispatch template states, in the prompt itself, that the agent receives
      no background notifications and must run everything synchronously in the foreground — with the
      bounded-poll idiom given inline so the agent has a ready alternative rather than only a
      prohibition.
- [ ] The instruction says what to do when a wait is genuinely long: poll in a bounded foreground loop
      and report, or return with findings plus an explicit "CI still pending on <SHA>" so the Foreman can
      take it over. Never return a stub.
- [ ] Add the same line to the reporting contract, so a report that names a pending background task
      instead of results is recognisable as incomplete.
- [ ] Check whether the `/sprint` and `/sprint-batched` skills should carry it too, rather than only the
      per-dispatch prompts — a skill-level statement survives Foreman drift; a prompt-level one does not.
- [ ] Sweep for the inverse hazard while here: any place the Foreman assumes a worker WILL be woken.

## Notes

Filed mid-run after the third occurrence. The three recoveries all succeeded — no work was lost — but
each needed an explicit "you cannot wait, report now, here is what I want" message naming every field
of the expected report.

One thing to preserve when fixing: a worker polling CI in the foreground must re-check by **SHA**, not
by PR number alone. `gh pr checks --watch` exits 0 when the branch moves under it, which is a separate
recorded trap and would otherwise be reintroduced by the very idiom this ticket recommends.

## Work Log

### 2026-08-23 — fixed, branch `cpe-1848-harness-stall-and-log-truncation` (built alongside CPE-1868)

Added a `### Dispatch contract` subsection to `.claude/commands/sprint.md` → "The crew", immediately
after the existing "spawning sub-agents is pre-authorised" paragraph. It states plainly that a dispatched
Worker/Reviewer/UAT sub-agent never receives a background task notification (that capability belongs to
the Foreman's own harness loop, not to a sub-agent), gives the bounded-poll idiom inline (`gh run watch
<run-id> --interval 30` / `gh pr checks <pr> --watch`, both blocking), preserves the re-check-by-**SHA**
requirement from this ticket's Notes (not PR number alone — the `gh pr checks --watch` exits-0-on-moved-
branch trap), and tells the agent what to do on a genuinely long wait: poll bounded and report, or return
now with an explicit `CI still pending on <SHA>` line. It explicitly bans returning a stub or saying a
monitor is "armed."

Added a matching bullet to the `## Reporting` section (the reporting contract) that names the exact
phrases from this ticket's three observed incidents ("a monitor is armed," "the background watch will
report," "I'll wait for the notification") as recognisable **stalls**, not status updates, and tells the
Foreman to re-invoke the same agent with the contract restated rather than wait on it.

**AC: check `/sprint-batched` too** — added an explicit reinforcement paragraph in
`.claude/commands/sprint-batched.md` (right after the intro) pointing at the `sprint.md` dispatch
contract and noting a batched run is the highest-risk case (unattended overnight, nobody watching a
stalled sub-agent freeze the `K/N` counter).

**AC: sweep for the inverse hazard** (the Foreman assuming a worker WILL be woken) — swept `sprint.md` and
`sprint-batched.md` for every mention of "background"/"watch"/"monitor"/`ScheduleWakeup`. The only
worker-facing mention was the heartbeat's "Background agents re-invoke you," which correctly describes
what happens to the **Foreman** (the harness wakes the standing supervisor when ITS own dispatched agents
return) — not an instruction telling a sub-agent to expect a wake-up of its own. No inverse-hazard
instance found; called out explicitly in the new dispatch-contract paragraph so the distinction is spelled
out rather than left implicit.

**Guard, and the proof it can fail:** `src/lib/sprintDispatchAndCiLogGuards.test.ts` (shared with
CPE-1868) reads the real `.claude/commands/sprint.md` / `sprint-batched.md` text and asserts the
load-bearing phrases above are present — this repo has no other automated check over prompt-file content,
so this is what stops the fix from silently rotting on a future rewrite. Proof: reverted the new
"Dispatch contract" section locally and re-ran the suite — 7 of the 14 tests in the CPE-1848 half went red
(`AssertionError: expected ... to match /receive[s]? NO background task notifications/i`, etc.); restored
the section (`git checkout -- .claude/commands/sprint.md`) and all 14 passed again. Full 3-OS `npm run
check` / `cargo` suites don't apply — no application code changed for this ticket.

**Assumption logged:** no existing per-dispatch template file exists in this repo (Worker/Reviewer/UAT
prompts are composed ad hoc by the Foreman from `sprint.md`'s "The crew" prose, not from a separate
template file) — so "every dispatch template states this" is satisfied by putting the contract in the one
place all those prompts are drawn from, `sprint.md` itself, rather than inventing a new template file.
