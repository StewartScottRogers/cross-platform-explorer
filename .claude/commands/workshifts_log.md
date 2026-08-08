# workshifts_log — Disk-Logged Batch Loop

Run `/workshift` in repeated batches, appending a **timestamped line to a disk log** after every batch, so
the shift leaves an audit trail that survives the conversation scrollback.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `batch_size` | `10` | Tickets driven through the gauntlet per batch |
| `log_file` | `workshift_log.txt` | Resolved under `.claude/workshift-metrics/` |

## Procedure

Loop until a family stop condition fires:

1. `run_workshift_batch(batch_size)`.
2. Take the wall-clock timestamp as `YYYY-MM-DD HH:MM:SS`.
3. **Append** — never truncate — one line to `.claude/workshift-metrics/<log_file>`:
   `<timestamp> - Completed batch of <batch_size>`
4. Report `Logged batch of <batch_size>. Restarting...` plus a plain-language line on what shipped.

## Notes

- **Append-only, always.** Use `Add-Content` / `>>`; never `Set-Content`, `Write`, or `>`. This log's whole
  value is that earlier lines are still there. Writing a file you have not read is exactly how a shift's
  history gets erased.
- Encoding: pass `-Encoding utf8` explicitly so a later reader doesn't get ANSI-mangled text.
- The spec's line format is the guaranteed prefix. Appending richer detail after it (tickets merged, PR
  numbers, duration) is encouraged — a log line that only says "10" answers nothing a week later.
- This is a **narrative** log, not a structured one. The machine-readable record stays
  `.claude/workshift-metrics/ledger.jsonl`; this file is for a human skimming what happened overnight.
- For several rotating log targets instead of one, use `/workshifts_rotating_logs`.
