# workshifts_scheduled — Fixed-Interval Batching

Run `/workshift` in repeated batches on a **fixed interval**, so batches start on a predictable cadence
rather than back-to-back.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `interval_sec` | `60` | Wait between batches |

## Procedure

Loop until a family stop condition fires:

1. `run_workshift_batch(batch_size)`.
2. Report `Next batch in <interval_sec>s...` plus a plain-language line on what shipped **and the wall-clock
   time of the next batch** ([[loop-behavior-needs-timestamps]]).
3. Wait `interval_sec`, then loop.

## How the wait actually happens

**Never call a foreground `sleep`** — it is blocked here and would stall the heartbeat for the whole
interval.

- **In-session:** `ScheduleWakeup` with `interval_sec`, then yield the turn. The runtime clamps to
  `[60, 3600]`, so the default of 60s is exactly the floor and anything shorter is honoured as 60s — stated
  in the tick, not silently.
- **Across sessions / for a genuine cron cadence:** the `schedule` skill (`CronCreate`) is the right tool —
  it survives session end, which `ScheduleWakeup` does not. Use it when "every hour" must hold overnight
  regardless of what happens to this session.

## Interval semantics — one honest caveat

The spec waits `interval_sec` **after** each batch returns, so the true period is
`batch_duration + interval_sec`, and it **drifts**: a 12-minute batch on a 60s interval yields a batch every
~13 minutes, not every minute. Implement the spec's behaviour, and report the actual period rather than
implying a fixed clock.

If a **non-drifting** cadence is wanted, that is a real scheduler — `CronCreate` with a cron expression —
and a batch overrunning its period needs an explicit policy (skip, queue, or overlap). Don't fake it in
this loop.

## Notes

- Difference from `/workshifts_throttled`: identical mechanics, different intent. Throttled backs off to be
  polite about resources; scheduled paces to a cadence. Same drift caveat applies to both.
- Long intervals mean long idle stretches — quiesce sub-agents before yielding, and don't hold the merge
  lock across a wait.
