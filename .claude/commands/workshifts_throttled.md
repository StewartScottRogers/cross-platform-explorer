# workshifts_throttled — Cooldown Between Batches

Run `/workshift` in repeated batches with a **fixed cooldown after each batch**, deliberately leaving the
machine and the API idle between bursts.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `cooldown_sec` | `5` | Idle time **after** each batch completes |

## Procedure

Loop until a family stop condition fires:

1. `run_workshift_batch(batch_size)`.
2. Report `Cooldown <cooldown_sec>s...` plus a plain-language line on what shipped, **and the wall-clock
   time the next batch starts** ([[loop-behavior-needs-timestamps]]).
3. Wait `cooldown_sec`, then loop.

## How the wait actually happens

**Never call a foreground `sleep`.** It is blocked in this harness, and blocking the turn would stall the
heartbeat and hide the shift's state for the whole cooldown.

- Use **`ScheduleWakeup`** with the cooldown as the delay, then yield the turn.
- The runtime clamps a scheduled wake to **60s minimum**. The spec's default `cooldown_sec=5` is below that
  floor: a cooldown under 60s is honoured as the next practical boundary, and that is stated in the tick
  rather than pretended away. Sub-minute throttling is not a thing this loop can do precisely.
- Do not spawn work during the cooldown. An idle window with agents running is not a cooldown.

## Notes

- **Cooldown is measured after the batch ends**, so the cycle is `batch + cooldown` and total period drifts
  with batch duration. For a fixed *period* between batch **starts**, use `/workshifts_scheduled`.
- This is the mode for deliberately backing off — sharing the machine, staying under a rate limit, or
  leaving the desktop responsive ([[run-commands-without-taking-desktop]]). It trades throughput for
  politeness, on purpose.
- Throttling is not error handling. For backoff triggered by failures, that belongs to
  `/workshifts_autonomous` or `/workshifts_autorecover`; the two can be combined.
