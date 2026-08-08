# workshifts_checkpoint — Checkpointed Batch Loop

Run `/workshift` in repeated batches, persisting a **resumable checkpoint** to disk after every batch, so a
crashed or reset session picks up exactly where it left off instead of restarting from zero.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `checkpoint_file` | `checkpoint.json` | Resolved under `.claude/workshift-metrics/` |

## Procedure

1. **Load state.** If `.claude/workshift-metrics/<checkpoint_file>` exists, read it. Otherwise start from
   `{"completed_batches": 0}`. A corrupt or unparseable file is **not** silently reset — report it and ask
   before overwriting, because it is the only record of prior progress.
2. **Announce the resume point.** State the wall-clock time and which batch number is starting. Resuming at
   batch 7 must look different from starting fresh.
3. **Loop** — until a family stop condition fires (see the standard):
   1. `run_workshift_batch(batch_size)`.
   2. Increment `completed_batches`.
   3. **Write the checkpoint before anything else.** Write to a temp file in the same directory, then
      rename over the target, so a crash mid-write can't leave a truncated checkpoint.
   4. Report: `Completed batch #<n> of size <batch_size>. Restarting...` plus a plain-language line on what
      actually shipped and the next-wake time.

Persist alongside the counter — a bare count can't reconstruct a shift:
`last_batch_receipt`, `tickets_completed_total`, `last_ticket_ids`, `started_at`, `updated_at`.

## Notes

- The checkpoint is **run state**, not work product: gitignored, and never a substitute for the ticket
  files, which remain the source of truth for what is Done.
- The checkpoint write is the resume contract. If a batch merged PRs but the checkpoint write failed, the
  next run re-reads a stale count — so a failed checkpoint write is a **hard stop**, not a warning.
- This is the mode to reach for when a shift will outlive its session. Pairs with the sub-agent budget
  reset: quiesce → checkpoint → hand off → resume here.
