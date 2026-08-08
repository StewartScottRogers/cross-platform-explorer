# workshifts_autorecover — Retry-in-Place Batch Loop

Run `/workshift` in repeated batches; a batch that fails is **immediately re-attempted** rather than skipped.
The loop only advances past work it actually completed.

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
2. **On error**: report `[RECOVERY] Error detected. Restarting...`, apply the recovery policy below, and
   `continue` — retry the same batch, skipping the completion report.
3. **On success**: report `Completed batch of <batch_size>. Restarting...` plus what shipped.

## Recovery policy — a deliberate deviation from the literal spec

The spec's bare `except: continue` is the most dangerous line in the whole family. Bare `except` also
swallows the loop's own stop signals, and `continue` with no delay retries instantly — so a permanent
failure becomes an unkillable hot loop that logs `[RECOVERY]` forever and torches the budget.

The guard is therefore bounded and selective:

- **Never swallow a stop.** A user stop, a hard-stop safety condition, or a budget wall **propagates** — it
  is not an error to recover from. Catch batch failures, not control flow.
- **Bounded backoff between retries** ([[circuit-breaker-for-retryable-errors]]) via `ScheduleWakeup`, never
  an immediate spin.
- **Per-batch retry cap** (default 3). After that, stop retrying this batch and escalate to the user with
  the failure and what was attempted. An identical failure three times running is not transient.
- **Re-derive state before each retry.** A failed batch may have merged some PRs before dying — re-read
  ticket state and `main` rather than replaying the batch blind, or the retry duplicates landed work.
- **`[RECOVERY]` is not enough on its own.** The spec's message names no cause; always append what actually
  failed. Every retry is recorded to `.claude/workshift-metrics/ledger.jsonl` and counted in the wrap.

## Notes

- Difference from `/workshifts_autonomous`: that one reports the error loudly and moves **on** to the next
  batch; this one quietly retries the **same** batch until it passes or hits the cap. Use this when batches
  must not be skipped, and autonomous when forward progress matters more than any single batch.
- A batch that only succeeds on retry is reported as **recovered**, not clean — the wrap shows retry counts.
