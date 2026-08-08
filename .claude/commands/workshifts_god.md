# workshifts_god — Every Knob in One Signature

The full-surface entry point: accepts **every parameter used anywhere in the `workshifts_*` family** and
forwards the lot to `/workshifts`.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by every mode.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Used by mode |
|---|---|---|
| `mode` | `checkpoint` | — (selects the policy) |
| `batch_size` | `10` | most modes |
| `workers` | `2` | `parallel` |
| `gpu_id` | `0` | `gpu` |
| `min_batch` | `5` | `random` |
| `max_batch` | `20` | `random` |
| `weights` | `{"small": 5, "medium": 10, "large": 20}` | `weighted` |
| `interval_sec` | `60` | `scheduled` |
| `queue` | `[("high", 20), ("medium", 10), ("low", 5)]` | `priority` |
| `log_files` | `["log1.txt", "log2.txt", "log3.txt"]` | `rotating_logs` |
| `sync_url` | `https://example.com/sync` | `remote_checkpoint` (**gated**) |

## Procedure

1. Report `[GOD MODE] Starting workshifts in mode: <mode>`.
2. Call `/workshifts` with **all eleven parameters forwarded**, then run whatever it dispatches to.

## What this skill does and doesn't do

It is a **signature, not a policy.** All the behaviour lives in the mode it selects; this skill adds one
banner line and a complete parameter surface. The name is the specification's, not a claim of extra power —
`/workshifts_god checkpoint` and `/workshifts_checkpoint` run the identical loop.

Two consequences worth stating before use:

- **Most parameters are inert for any given mode.** `mode=checkpoint` ignores `workers`, `gpu_id`, `weights`,
  `queue`, `interval_sec`, and the rest. Because everything is forwarded, a parameter meant for a *different*
  mode is accepted and quietly does nothing. **At kickoff, echo the resolved mode, the parameters it will
  actually use, and the ones being ignored** — otherwise a user who set `workers=8` on a checkpoint run has
  no way to learn it never took effect.
- **`sync_url` defaults to a placeholder.** `https://example.com/sync` is IANA's reserved example domain.
  With `mode=remote_checkpoint`, the confirmation gate in `/workshifts_remote_checkpoint` runs in full —
  passing a URL through this skill approves nothing, and data still leaves the machine only after the user
  says so.

An unknown `mode` propagates the `Unknown mode: <mode>` error from `/workshifts`. This skill does not add a
fallback.

## Notes

- Prefer `/workshifts` for everyday use — same dispatch, without the eleven-parameter surface inviting
  settings that don't apply. Reach for this one when a run is being configured from a saved parameter set
  and it's simpler to pass everything than to work out what the mode needs.
