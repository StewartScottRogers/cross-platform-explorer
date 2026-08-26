---
id: CPE-1880
title: the dispatch contract does not stop the stall — five agents in one run, three after being handed the exact command
type: bug
priority: High
status: Doing
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

- [x] A stated, evidenced conclusion on whether instruction alone can prevent this.
- [x] A structural change that makes the stall impossible or self-recovering.
- [x] All five recorded returns replayed against the fix.
- [x] The loop specifically addressed: an agent that has armed a monitor must not be able to emit
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

### 2026-08-26 — investigated and fixed, branch `cpe-1880-foreman-owns-ci`

#### Step 1 — what the five stalls actually were, and which of the three explanations holds

The ticket offers three candidate explanations. Only one survives contact with the evidence.

| Candidate | Verdict |
|---|---|
| The agents never received the contract | **Ruled out.** Three of the five CPE-1794 returns came *after* a direct `SendMessage` that quoted the blocking command, named the defect, and named CPE-1848. |
| They received it and ignored it | **Ruled out.** Return #2 — `"A background monitor is now polling PR #1017's two check suites every 30s"` — is not defiance; `--interval 30` is CPE-1848's own prescribed interval. The agent was doing exactly what it was told. |
| They followed it correctly and stalled on something it does not cover | **This one.** And worse than "does not cover": the contract *prescribed the mechanism of the failure*. |

**The measurement that settles it.** Two facts multiply:

1. The harness's Bash tool caps one call at `timeout: 600000` ms. A command that outlives the cap is
   **auto-backgrounded, not killed** — so the agent is handed a background task by the harness, through
   no decision of its own, and then does the only thing available to something holding a background task
   it cannot be woken from.
2. `gh run watch` blocks until the run finishes. I pulled the last 100 `ci.yml` runs
   (`gh run list --workflow=ci.yml --limit 100 --json databaseId,createdAt,updatedAt,status,conclusion`,
   95 completed, window 2026-08-23T02:10Z → 2026-08-26T13:57Z) and computed wall clock as
   `updatedAt − createdAt`:

   | | min | p25 | median | p75 | p90 | max |
   |---|---|---|---|---|---|---|
   | all completed (n=95) | 0.2 m | 41.8 m | **58.9 m** | 63.7 m | 77.3 m | 97.0 m |
   | **successful only (n=71)** | **28.6 m** | — | **60.7 m** | — | — | — |

   **Zero of the 71 successful runs finished inside 600 s.** The only four sub-ten-minute runs in the
   whole window (`32689955727`, `32689226762`, `32686063824`, `32683231765`) were all `cancelled`.

   The two runs CPE-1857's worker cited check out independently: `32672218824` = 48 m 17 s,
   `32677925708` = 84 m 18 s (both `cpe-1857-overwrite-through-hardlink`, both `success`).

So CPE-1848 handed every dispatched agent a command with a **0-of-71** chance of returning inside the
cap. Instructing them harder could never have worked — which is precisely the pattern the evidence
showed: three tellings, three stalls.

**On the prior run's own retrospective.** `.claude/sprint-metrics/history.md` (line 1477) concluded
*"Hand agents the blocking command, not just the prohibition… The brief said 'you get no notifications';
it did not say `gh run watch <id> --interval 30`. Saying the latter is what stops it."* That lesson IS
reflected in the current contract text — it is the literal line CPE-1848 added — and the five stalls
post-date it by hours. The lesson was right in form and wrong in content: **a blocking command is only
safe if it is bounded below the tool cap**, and `gh run watch` is not bounded at all. Following the
retrospective is what produced the next round of stalls.

**A second, sharper finding while replacing that line.** CPE-1848's guard test
(`src/lib/sprintDispatchAndCiLogGuards.test.ts`) asserted
`expect(SPRINT_MD).toContain("gh run watch <run-id> --interval 30")` under the test name *"gives the
bounded-poll idiom inline"*. The command is not bounded, and the guard was therefore **pinning the
defect in place** — deleting the stall-causing line would have turned CI red. That is this repo's own
most-repeated finding wearing a new hat: a guard that is green while the thing it guards is broken. The
assertion is now inverted (see below).

#### Step 2 — instruction alone cannot prevent this (AC 1)

Stated and evidenced: **no.** The agents complied and complying is what stalled them. A fourth wording
of "do not wait on a notification" cannot help an agent whose call was backgrounded by the harness after
it had already returned control. The fix has to remove the unbounded call and catch the failure on
arrival.

#### Step 3 — the structural fix, and what was rejected

**Chosen (AC 2), three parts, in decreasing order of how structural they are:**

1. **`scripts/ci-poll.mjs`** — a bounded CI poll that *cannot* be backgrounded. Worst-case wall clock is
   clamped to `600 s − 120 s margin` by `clampBudgetMs()`, and the clamp is one-directional: `--budget`
   can only ever ask for *less*. There is no flag, env var, or argument that raises the ceiling, so the
   stall cannot be reintroduced by configuration — only by deleting the function, which reds four tests.
   It prints one timestamped line per tick and always ends with a single `CI VERDICT:` line carrying
   `total_count`, `pending`, `mergeable`, and the SHA. It also **mechanises the two poll traps sprint.md
   states in prose** — `total_count == 0` is reported rather than read as green (naming `CONFLICTING`
   when that is why), and `pending == 0` is only trusted once `total_count` has been stable across two
   reads — so those rules no longer depend on an agent remembering them mid-poll.
2. **`scripts/stall-check.mjs`** — a classifier the Foreman runs over every returned report. Six pattern
   families, split `hard` (a backgrounded watcher — the offence itself) and `soft` (contentless
   deferral, which an explicit handoff line legitimately excuses). Fenced blocks and `>` blockquotes are
   stripped first, so *documenting* the rule is not *committing* the offence — without that, this
   ticket's own PR body would trip its own detector.
3. **`sprint.md` / `sprint-batched.md`: the Foreman owns CI.** Workers push and report; they are never
   asked to establish a CI outcome. `gh run watch` / `gh pr checks --watch` are now banned for
   sub-agents outright rather than prescribed, with the 0-of-71 measurement inline so the ban reads as a
   fact rather than a preference. The `timeout 570 gh run watch` mitigation is explicitly warned off —
   it was measured, it still backgrounds (the harness timer spans the compound command), and it is the
   fix everyone reaches for.

**The loop, specifically (AC 4).** `classifyReport(report, { priorStalls })` bounds the escalation at
**one** retry: first stall-shaped return → re-invoke once; **second from the same agent → kill and take
over**. Replaying the recorded CPE-1794 sequence gives `["re-invoke", "take-over", "take-over",
"take-over"]` — exactly one re-invoke, and the run never reaches returns 3 and 4. That is asserted as a
test.

**Rejected, with reasons:**

- *Deny background/monitor tooling to sub-agents outright.* The most structural option on the list and
  the one I would take if it were reachable — but it is a harness capability, not a repo one. Nothing in
  this tree can revoke it, and more to the point it would not have helped: the harness backgrounded these
  calls **on the agents' behalf** when they overran the cap. The agents never invoked a monitor tool.
  Removing the tool leaves the auto-background intact.
- *A stronger/louder contract paragraph.* Measured and refuted by the ticket itself. Retained only as the
  carrier for the other two fixes, never as the mechanism.
- *`timeout 570 gh run watch … | tail`.* Recorded as tried and failed; now documented as a trap rather
  than left to be rediscovered.
- *A reusable dispatch-prompt builder so the contract cannot be omitted.* Genuinely attractive and it is
  the right answer to "the Foreman forgot to paste it" — but that is not what happened here. The contract
  *was* pasted, verbatim, plus a follow-up message. A builder would have reproduced the bad command with
  perfect fidelity. It solves an adjacent problem this evidence does not show.
- *Making `ci-poll` a Rust binary or a `cpe-server` module.* No: it is harness tooling, not product. Node
  is already a build dependency, it costs the shipped app nothing, and PURPOSE.md's small/predictable
  tiebreaker says keep it out of the binary.

#### Step 4 — replaying all five recorded returns (AC 3)

`src/lib/sprintStallControls.test.ts` carries the five verbatim returns from run
`batched-2026-08-23-1124` and the three phrasings CPE-1848 banned by name. All eight classify as stalls;
the five all yield `re-invoke` on first offence and `take-over` on the second. **44 tests, all green.**

A benign corpus of eight must-not-trip cases runs alongside them, including the two that matter most:
the *prescribed* return (`"CI still pending on 84d20517 — total_count=19 pending=4 mergeable=MERGEABLE.
Handing CI to the Foreman."`) and `ci-poll`'s own budget-exhausted output — if the detector flagged
either, the two controls would fight each other. It also covers the product's own "watcher" vocabulary
(Agent Watch streams events in the background), `npm run test:watch`, and a future-tense
"the Performance Guard will report the size delta" that must not read as a promised notification.

**Proof the tests can fail** (three mutations, each reverted):

| Mutation | Result |
|---|---|
| every `severity: "hard"` → `"soft"` in `stall-check.mjs` | 5 of 44 red — a backgrounded watcher would be excused by a handoff line |
| `MAX_BUDGET_MS = HARNESS_TOOL_TIMEOUT_MS * 4` in `ci-poll.mjs` | red — the clamp assertions catch a re-widened budget |
| restore `To watch CI: gh run watch <run-id> --interval 30` in `sprint.md` | 1 of 29 red in the CPE-1848 guard |

#### Assumptions logged

- The 600 s figure is read from the Bash tool's own documented `timeout` maximum, not from a probe of the
  auto-background threshold. `SAFETY_MARGIN_MS` is 120 s to absorb being wrong about where exactly the
  line sits; `assertNotBackgroundable()` fires at start-up if the numbers ever drift.
- The detector deliberately **over-flags**. A false positive costs one re-invoke of an agent that
  restates a report it already has; a false negative costs a hung agent, a frozen batch counter, and a
  full Foreman round-trip. Those costs are not close, so the bias is one-sided on purpose. One known and
  accepted false positive: "I am still waiting on the reviewer" with no handoff line reads as a stall.
- CI duration is measured as `updatedAt − createdAt`, i.e. queued + running, which is the number that
  matters to a blocking watch. Queue depth is an aggravating factor (15 runs in flight across seven
  branches during the recorded run), not the cause: even the *fastest successful* run in three days,
  28.6 min, is 2.9× the cap.
