# workshifts_rotating_logs — Round-Robin Log Files

Run `/workshift` in repeated batches, appending each batch's line to the **next log file in a rotation**, so
no single file grows without bound over a long run.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `log_files` | `["log1.txt", "log2.txt", "log3.txt"]` | Rotation targets, resolved under `.claude/workshift-metrics/` |

## Procedure

1. `idx = 0`.
2. Loop until a family stop condition fires:
   1. `run_workshift_batch(batch_size)`.
   2. `lf = log_files[idx]`.
   3. **Append** to `.claude/workshift-metrics/<lf>`: `Completed batch of <batch_size>`
   4. Report `Logged batch to <lf>. Restarting...` plus a plain-language line on what shipped.
   5. `idx = (idx + 1) % len(log_files)`.

## Notes

- **Append-only, always** — `Add-Content` / `>>`, with `-Encoding utf8`. Never `Set-Content` or `>`. This is
  a rotation, not a truncation: file 1 is reused on the fourth batch and its earlier lines must survive.
- **This rotation does not cap total size.** Round-robin across three files spreads the writes; it never
  deletes anything. If the goal is bounded disk use, that needs a real retention rule (truncate the oldest
  on wrap, or cap by size) — say so rather than assuming rotation implies pruning.
- The spec's line carries no timestamp, which makes an interleaved rotation hard to reassemble. Prefix each
  line with a wall-clock stamp anyway; the spec's text is the guaranteed content, not the whole line.
- Also record which file the run is currently on, so a resumed run doesn't restart the rotation at index 0
  and interleave out of order.
- An empty `log_files` is an error, not a silent skip of the logging step.
- For a single log target, use `/workshifts_log`.
