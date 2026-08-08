# workshifts_gpu — Named-Lane Batch Loop

Run `/workshift` in repeated batches on a **single named lane**, tagging every batch and every report line
with that lane's id.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## What `gpu_id` actually is — read this first

**There is no GPU acceleration in this pipeline, and this skill does not add any.** A workshift batch is
sub-agents editing files, running `cargo`/`npm`, and merging PRs — all CPU and I/O bound. Nothing in it
dispatches to a GPU, and `gpu_id` selects no hardware.

What `gpu_id` genuinely does: it is a **lane label and a concurrency slot**. It names one independent lane so
its batches, logs, and worktrees are attributable, and so several lanes (`gpu_id=0`, `gpu_id=1`, …) can run
side by side without confusing their output.

This is stated plainly rather than implemented as a fiction: a skill that printed `[GPU 0]` while implying
acceleration would misreport what the machine did. If real GPU work ever lands here (local model inference,
say), this is the seam it would attach to.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `gpu_id` | `0` | Lane label / concurrency slot |

## Procedure

Loop until a family stop condition fires:

1. `gpu_run_workshift_batch(batch_size, gpu_id)` — a normal batch, with every line, worktree name, and
   ledger row tagged `[GPU <gpu_id>]`.
2. Report `[GPU <gpu_id>] Completed batch of <batch_size>. Restarting...` plus a plain-language line on
   what shipped.

## Notes

- Run several of these concurrently for multi-lane work, or use `/workshifts_parallel`, which manages the
  lanes and the join for you. The disjoint-slice and serial-merge-lock rules there apply here too.
- Keep `gpu_id` stable across a resumed run — it is what ties this lane's logs and worktrees together.
