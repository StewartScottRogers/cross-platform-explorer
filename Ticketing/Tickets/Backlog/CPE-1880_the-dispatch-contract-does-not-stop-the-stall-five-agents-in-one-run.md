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


---

## ROOT CAUSE FOUND — 2026-08-23 19:40, and it is structural, not behavioural

The hypothesis above ("a model with a monitor tool will reach for it under uncertainty regardless of
instruction") is **wrong**, or at best secondary. A worker on CPE-1857 — the only agent in the run to
recover from the stall unaided — supplied the measurement that explains all seven cases:

> **`gh run watch` cannot fit inside the harness's maximum tool timeout on this repo, so it is
> auto-backgrounded every time, not occasionally.** The Bash tool caps at `timeout: 600000` (10 min).
> Observed end-to-end CI here: **45 min** (run `32672218824` — 31 min queued + 14 min running) and
> **72 min** (run `32677925708` — 19 min queued + 53 min running). Both `gh run watch` calls were
> moved to background at exactly the 600 s mark (`bfr274ats`, `br86w951v`).

So the agents were not choosing to wait. **The harness backgrounded the call on their behalf**, and
they then did the only thing that follows from a backgrounded task: waited for it. Instructing them
harder could never have worked, which is exactly what the evidence showed — three tellings, three
stalls.

**Two further findings that change what the fix must be:**

1. **The obvious mitigation does not work.** A shell-level `timeout 570 gh run watch … | tail -6; …`
   wrapper was still auto-backgrounded, apparently because the harness timer spans the whole compound
   command rather than the wrapped process. **Do not document "just cap it below the tool limit"
   without testing it first** — it is the fix everyone will reach for and it did not hold.

2. **The recovery that does work** — worth shipping as the prescribed idiom, since it never parks and
   always returns output:

   ```bash
   for i in $(seq 1 17); do
     s=$(gh run view <id> --json status,conclusion -q '.status+" "+.conclusion')
     echo "$(date -u +%H:%M:%SZ) CI: $s"
     case "$s" in completed*) break;; esac
     sleep 32
   done
   ```

   Bounded to land under the cap, one timestamped line per tick (which also satisfies the
   loop-timestamp convention), re-invoked as many times as needed. It took **six** invocations to
   cover the 72-minute run — tedious, but it never stalls.

**Queue depth is the aggravating factor, not the cause.** At one point 15 CI runs were in flight
across seven sprint branches, which pushed queue time to 31 minutes on its own. A quieter repo would
hide this bug rather than fix it.

## What this means for the fix

The **"Foreman owns CI"** option listed above is no longer one of three candidates — it is the
indicated fix, and it was validated live during the run: the moment a stalled worker was told *"I own
CI, do not watch it, hand me the report you already have"*, it returned a complete, high-quality
report immediately. The worker never lacked the material; it lacked a way to stop waiting.

Concretely:
- Workers **push and report**. They must not be asked to establish CI outcomes at all.
- The Foreman — the only participant that genuinely receives completion notifications — owns every
  CI wait, and routes failures back.
- If a worker must poll for some other reason, the bounded loop above is the idiom, never
  `gh run watch`.
- Consider whether the background/monitor tooling should simply be **unavailable** to sub-agents, so
  the harness cannot background a call on their behalf in the first place.

Note the run's own instructions already drifted toward this by accident: the later dispatch briefs say
"the Foreman owns CI for this PR — do not watch, poll, or monitor it", and no worker briefed that way
has stalled since.
