# workshifts_rotate — Rotating Batch Sizes

Run `/workshift` in repeated batches whose size **cycles through a fixed list**, round-robin. Small batches
give frequent safe stopping points; large ones amortise the dispatch overhead. Rotating gets both.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_sizes` | `[5, 10, 20]` | The rotation, in order |

## Procedure

1. `idx = 0`.
2. Loop until a family stop condition fires:
   1. `bs = batch_sizes[idx]`.
   2. `run_workshift_batch(bs)`.
   3. Report `Completed batch of <bs>. Restarting...` plus a plain-language line on what shipped.
   4. `idx = (idx + 1) % len(batch_sizes)`.

## Notes

- The rotation is **deterministic and in order** — 5, 10, 20, 5, 10, 20, … Do not shuffle, sort, or skip;
  a size that keeps coming up short is information, not a reason to reorder. For randomised sizing use
  `/workshifts_randomized` or `/workshifts_weighted`.
- An empty `batch_sizes` is an error, not an infinite no-op loop — say so and stop.
- Position in the rotation is **not** persisted across sessions; a resumed run restarts at index 0. Combine
  with `/workshifts_checkpoint` if the position needs to survive a reset.
- Rotation position is cosmetic when the queue is shallow — a batch of 20 against 6 ready tickets is a short
  batch of 6, and gets recorded as such rather than padded with speculative work.
