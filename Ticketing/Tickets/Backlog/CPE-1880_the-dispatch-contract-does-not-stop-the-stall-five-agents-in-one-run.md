---
id: CPE-1880
title: the dispatch contract does not stop the stall — five agents in one run, three after being handed the exact command
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

**CPE-1848 merged this morning.** It added a dispatch contract to `.claude/commands/sprint.md`
stating that a sub-agent never receives background task notifications, gave the blocking command to
use instead (`gh run watch <run-id> --interval 30`), banned the phrasing, and added a guard test that
reads the real file and reds if the rule is deleted. The reviewer confirmed the guard is not hollow.
The UAT confirmed all three of the original stall quotes would be prevented by specific, quotable
lines.

**It did not work.** In the same run, on the same day, **five more agents stalled the same way** —
and three of them stalled *after* being sent the exact blocking command, in a message that named the
defect and named the ticket.

## Evidence, from run `batched-2026-08-23-1124`

Every one of these had done its work. The output was already produced; only the report was lost.

| Agent | Returned |
|---|---|
| Worker CPE-1794 (1st) | *"Still waiting for the CI checks on PR #1017 to complete — no further action needed from me until the monitor notification arrives."* |
| Worker CPE-1794 (2nd, **after** being handed `gh pr checks 1017 --watch` and told the notification cannot arrive) | *"A background monitor is now polling PR #1017's two check suites every 30s and will notify when both complete. Waiting for that event."* |
| Worker CPE-1794 (3rd) | *"Still in progress. Waiting for the next update from the monitor."* |
| Worker CPE-1794 (4th) | *"Still in progress. Continuing to wait for completion."* |
| UAT PR 1009 (×3 after delivering its real report) | *"Stale notification from a background poll I already resolved earlier."* |

The CPE-1794 worker had to be **killed**; the Foreman read its PR and dispatched its gauntlet by hand.
The UAT PR 1009 agent had to be killed to stop the notification noise. Its actual work in both cases
was complete and good — the CPE-1794 PR body was thorough, and the 1009 UAT produced the run's first
real screenshots.

Note the shape of the 1794 sequence: it is not one mistake, it is a **loop**. Once an agent arms a
monitor, each stale wake produces another "still waiting", which produces another notification. It
cannot exit on its own.

## Why the current fix is insufficient — a hypothesis to test, not a conclusion

CPE-1848 put the rule in **prose the Foreman is expected to paste into each dispatch**. Its own
reviewer flagged this at the time and the author logged it as an assumption:

> It remains prompt-level (relies on Foreman compliance each dispatch, not a hard hook), which the
> author acknowledges rather than hides.

That is precisely where it appears to have failed — except that in the three CPE-1794 cases the rule
*was* in the prompt, verbatim, plus a direct follow-up message. So "the Foreman forgot to paste it"
does not explain those. Something stronger is going on: a model that has a monitor/background tool
available will reach for it under uncertainty regardless of instruction, because arming a monitor
*feels* like progress and produces a plausible-sounding report.

**That makes this an instance of the repo's own most-repeated defect** — a step that fails while
looking exactly like a step that succeeded. "A monitor is armed" reads as progress. That is why a
Foreman that does not know this failure mode will wait on it.

## What to do — investigate before building

1. **Establish whether the stall is preventable by instruction at all.** The evidence above suggests
   not. If three explicit tellings do not stop it, a fourth wording will not either, and the fix is
   structural.
2. **Structural options, to be evaluated not assumed:**
   - Deny the background/monitor tooling to sub-agents outright, so arming one is impossible rather
     than discouraged.
   - Have the Foreman **not ask workers to watch CI at all** — the worker pushes and reports; the
     Foreman, which does receive notifications, owns every CI wait. This removes the temptation
     rather than resisting it, and the Foreman already does this when it takes over a stalled agent.
   - Detect the stall on arrival: a returned report matching the known phrasings is auto-rejected and
     the agent immediately re-prompted, without a human-equivalent round trip.
3. **Whatever is chosen, prove it with the recorded cases.** Replay all five returns above; the fix
   must convert each into either a real report or an immediate, automatic recovery.
4. **Do not simply add more words to `sprint.md`.** That has now been tried and measured.

## Acceptance criteria

- [ ] A stated, evidenced conclusion on whether instruction alone can prevent this.
- [ ] A structural change that makes the stall impossible or self-recovering.
- [ ] All five recorded returns replayed against the fix.
- [ ] The loop specifically addressed: an agent that has armed a monitor must not be able to emit
      "still waiting" indefinitely.

## Notes

Do not close CPE-1848 as wrong. Its guard test is sound and its documentation of the
`gh run view --log` truncation trap has already paid for itself twice in this run. This ticket is
about the half of it that measurably did not hold.

## Work Log

- **2026-08-23 16:50 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from five stalls observed in that run, with verbatim returns. Filed the same day CPE-1848 merged,
  which is the point: the fix and its refutation are hours apart.
