---
id: CPE-1848
title: workers stall waiting on background notifications they can never receive
type: task
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
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
