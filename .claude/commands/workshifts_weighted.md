# workshifts_weighted — Weighted Batch Classes

Run `/workshift` in repeated batches, each one drawn at random from a set of **named batch-size classes**.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `weights` | `{"small": 5, "medium": 10, "large": 20}` | Class name → batch size |

## Procedure

Loop until a family stop condition fires:

1. `k = random choice from the class names`.
2. `bs = weights[k]`.
3. `run_workshift_batch(bs)`.
4. Report `Completed weighted batch '<k>' (<bs>). Restarting...` plus a plain-language line on what shipped.

## What `weights` actually holds — read this before implementing

Despite the name, **the values are batch sizes, not selection probabilities.** The spec picks a class with
`random.choice(keys)`, which is **uniform over the class names** — `small` is as likely as `large`; the
`5`/`10`/`20` decide only how big the batch is once a class is chosen.

Implement it exactly that way: uniform pick over names, value as the size. Treating the numbers as
probability weights would change which batches run and how big they are, which is a different skill wearing
this one's name. If genuine probability weighting is wanted, that is a new parameter, not a reinterpretation
of this one.

- Draw uniformly with a real entropy source (`Get-Random`) in the foreground turn — not inside a `Workflow`
  script, where `Math.random()` throws.
- Log the chosen class **and** its size every batch; `'large'` alone doesn't say what ran.

## Notes

- Validate: `weights` non-empty, every value an integer `>= 1`. An empty map is an error, not a no-op loop.
- The class name is the useful part — it makes a long run's log readable at a glance in a way bare integers
  never are. Keep names meaningful if they're customised.
- For a deterministic cycle instead of random selection, use `/workshifts_rotate`; for a continuous random
  range rather than named classes, `/workshifts_randomized`.
