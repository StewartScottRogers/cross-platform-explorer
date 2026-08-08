# workshifts_until — Finite Run to a Target

Run `/workshift` in repeated batches until a **target number of shifts** has been processed, then stop.

This is the **only finite skill in the family** — every other `workshifts_*` skill rolls until the user
stops it. Use this one when the run needs a defined end (a sprint's worth of work, a bounded overnight
budget, a proof-of-life run before committing to an open-ended shift).

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order):

| Parameter | Default | Meaning |
|---|---|---|
| `target_shifts` | **required** | Total shifts to process before stopping |
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |

`target_shifts` has no default — if it is missing, ask for it rather than guessing. A finite skill with an
invented bound is just an infinite one with extra steps.

## Procedure

1. `processed = 0`.
2. While `processed < target_shifts`:
   1. `run_workshift_batch(batch_size)`.
   2. `processed += batch_size`.
   3. Report `Processed <processed>/<target_shifts> shifts...` plus a plain-language line on what shipped.
3. Report `Target reached. Stopping.` and give the full wrap: everything merged, everything left open, and
   what the natural next run would be.

## Notes

- **The counter advances by `batch_size`, per the spec — not by tickets actually completed.** So a target of
  100 with `batch_size=10` runs exactly 10 batches whatever they yield. If the real queue ran dry at batch 4,
  the remaining batches are short and the run still terminates at 10. Report both numbers at the wrap
  (`processed` vs. genuinely completed) so the difference is never hidden.
- A `target_shifts` that isn't a multiple of `batch_size` **overshoots** — the loop checks the bound before a
  batch, not during it. 25 with `batch_size=10` runs 3 batches (30). Say so up front rather than at the end.
- The family stop conditions still apply and take precedence: a user stop or hard-stop ends the run before
  the target, and that is reported as an early stop, never as "target reached".
