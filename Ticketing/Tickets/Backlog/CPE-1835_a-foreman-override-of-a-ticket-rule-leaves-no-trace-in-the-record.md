---
id: CPE-1835
title: a Foreman instruction that overrides a ticket's stated rule leaves no trace anyone else can check
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

During a sprint the Foreman dispatches a Worker with a written brief. That brief sometimes **overrides
an instruction the ticket states explicitly** — for good reason, but the brief itself lives only in the
agent conversation. It is in no PR, no commit, no ticket, and no worktree.

Measured instance, 2026-08-20, CPE-1788 / PR #976. The ticket said to red-proof by writing a real file
through PowerShell's `Out-File` and `Set-Content` defaults, citing Evidence Rule 2 in
`Ticketing/wiki.md` ("verify through the channel that will carry the message"). The Foreman's dispatch
said the opposite — build the bytes synthetically in python and do **not** let PowerShell touch a file
— because PowerShell writes have corrupted repo files here before and once blocked a release.

The Worker followed the brief and recorded it honestly as *"per the Foreman's explicit override."* The
independent Reviewer then went looking for that authorization — PR comments, reviews, `git log`,
worktrees — found **nothing**, and blocked the merge. Its reasoning was correct and is the point of
this ticket:

> a self-declared exception to a written rule, citing an authority nobody else can see, is exactly the
> shape that rule exists to prevent — independent of whether the technical result happens to be right.

It cost a review round to resolve, and it resolved only because the Foreman was still in the
conversation to paste the original instruction. Nothing in the repo would have recovered it later.

## Why it matters

Two failure modes, and the second is worse:

1. **A real override reads as a fabrication.** The Reviewer was right to challenge it; the Worker was
   right to report it. Both did their jobs and the round was still wasted.
2. **A fabricated override would read as real.** If a Worker ever wrote "per the Foreman's override"
   without one, today there is no way for a Reviewer to tell the two apart — the honest case and the
   invented case look identical from outside. That is a hole in the gauntlet, not a paperwork nit.

## Acceptance criteria

- [ ] Decide and record the mechanism. Options, roughly in order of preference:
      **(a)** the Foreman appends the override — the verbatim instruction, what it overrides, and why —
      to the **ticket's Work Log at dispatch time**, so it is in the repo before the work starts;
      **(b)** the Worker is required to quote the instruction verbatim rather than referring to it, so
      the Reviewer can at least read what was claimed;
      **(c)** overrides are posted as a PR comment by the Foreman.
      (a) is the only one that produces a record *before* the disagreement, which is when it is worth
      having.
- [ ] `.claude/commands/sprint.md` states the rule, in the escalation/decide-and-log section — the
      existing "log the assumption in the ticket work log" instruction covers the Foreman's own
      judgement calls but not an instruction that contradicts the ticket.
- [ ] `Ticketing/wiki.md`'s Evidence Rules say how a legitimate exception is recorded, so a Reviewer
      encountering one knows what a valid override looks like and can check it.
- [ ] The Reviewer brief (wherever it is templated) tells reviewers to challenge an uncorroborated
      override — that behaviour was correct and should be the documented expectation, not an instinct.

## Notes

Filed by the Foreman from the CPE-1788 review, where the override in question was the Foreman's own.
The Reviewer's final position is the right standard and should survive into whatever mechanism lands:
the override was accepted once its provenance was recorded **in the diff**, not once it was explained
in conversation.

Worth checking whether any Work Log already in `Ticketing/Tickets/Done/` cites an override with no
recoverable source — if so, those are the cases where nobody happened to look.
