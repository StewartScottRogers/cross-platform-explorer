# workshifts_randomized — Random Batch Sizes

Run `/workshift` in repeated batches whose size is drawn **at random from a range** each time.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `min_batch` | `5` | Smallest batch size, **inclusive** |
| `max_batch` | `20` | Largest batch size, **inclusive** |

## Procedure

Loop until a family stop condition fires:

1. `bs = random integer in [min_batch, max_batch]`, both ends inclusive.
2. `run_workshift_batch(bs)`.
3. Report `Completed random batch of <bs>. Restarting...` plus a plain-language line on what shipped.

## Where the randomness comes from

- Draw in the foreground turn with a real entropy source — `Get-Random -Minimum <min> -Maximum <max+1>`
  (PowerShell's `-Maximum` is **exclusive**, so add one to match the spec's inclusive `randint`).
- **Not** inside a `Workflow` script: `Math.random()` and `Date.now()` throw there, because they would break
  resume. Draw the size first, pass it in.
- Log every drawn size. An unlogged random parameter makes a run impossible to reconstruct afterwards.

## Notes

- Validate `min_batch <= max_batch` and both `>= 1`; an inverted or zero range is an error, not a silently
  clamped one.
- Randomised sizing is for **avoiding lockstep**, not for tuning throughput — it keeps batch boundaries from
  landing at the same point every cycle. If you want a defined mix of sizes, `/workshifts_rotate` (fixed
  cycle) or `/workshifts_weighted` (named classes) is the better instrument.
- Expect short batches whenever the draw exceeds queue depth; report the shortfall rather than padding.
