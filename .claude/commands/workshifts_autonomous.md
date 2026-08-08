# workshifts_autonomous — Error-Absorbing Batch Loop

Run `/workshift` in repeated batches inside an error guard: a batch that fails is **reported, then recovered
from**, and the loop continues. One bad batch never ends the shift.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |

## Procedure

Loop until a family stop condition fires:

1. **Try** `run_workshift_batch(batch_size)`.
   - On success: report `Completed batch of <batch_size>. Restarting...` plus what shipped.
2. **On error**: report `[ERROR] <what actually failed>. Auto-recovering...` — the real error, in plain
   language, never a generic "something went wrong" — then apply the recovery policy below and continue.

## Recovery policy — a deliberate deviation from the literal spec

The spec's `except Exception as e: print(...)` with no delay would, against a *permanent* failure (a red
base, a lost merge lock, an exhausted budget), spin the loop at full speed forever — burning the entire
budget while printing the same error. That is not autonomy, it is a hot loop.

So the guard is bounded ([[circuit-breaker-for-retryable-errors]]):

- **Retryable** (API 429/529/5xx, network blips, a stalled agent, a flaky CI run): exponential backoff
  scheduled via `ScheduleWakeup`, *and* reduced concurrency — don't hammer what is already overloaded.
- **Permanent** (base won't build, lock held by another shift, budget wall, repeated identical failure):
  stop retrying and **escalate to the user** with what failed and what was tried. Escalation is the
  circuit-breaker's whole point.
- **Consecutive-failure cap.** After N consecutive failed batches (default 3) with no successful batch
  between, halt and escalate. A loop that has failed three times running will not fix itself on the fourth.
- **Never mask a merge failure.** A batch that fails *after* merging must not be retried blindly — re-check
  `main` state first, or the retry duplicates work on a tree that already moved.

Every caught error is recorded to `.claude/workshift-metrics/ledger.jsonl` and surfaced in the wrap. An
error that is absorbed but never reported is a silently-dropped failure, and the wrap must state the count.

## Notes

- Difference from `/workshifts_autorecover`: this one **reports every error and moves to the next batch**;
  autorecover retries the *same* batch and stays quieter about it.
- "Auto-recovering" describes the loop surviving, not the underlying problem being fixed. Never report a
  recovered batch as a successful one.
