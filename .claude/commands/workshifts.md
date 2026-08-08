# workshifts — Unified Meta-Skill

The single entry point to the whole `workshifts_*` family: pick a loop policy by `mode` and pass it whatever
arguments it takes.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by every mode below.

## Parameters

Parse from `$ARGUMENTS`:

| Parameter | Default | Meaning |
|---|---|---|
| `mode` | `checkpoint` | Which loop policy to run |
| *(everything else)* | — | Forwarded verbatim to the target skill |

A bare first argument is taken as the mode: `/workshifts throttled cooldown_sec=120`. With no arguments,
run `checkpoint` at its defaults.

## Dispatch table — all 16 modes

| `mode` | Runs | Key arguments |
|---|---|---|
| `checkpoint` | `/workshifts_checkpoint` | `batch_size`, `checkpoint_file` |
| `log` | `/workshifts_log` | `batch_size`, `log_file` |
| `rotate` | `/workshifts_rotate` | `batch_sizes` |
| `until` | `/workshifts_until` | `target_shifts` (**required**), `batch_size` |
| `parallel` | `/workshifts_parallel` | `workers`, `batch_size` |
| `gpu` | `/workshifts_gpu` | `batch_size`, `gpu_id` |
| `vr` | `/workshifts_vr_dashboard` | `batch_size` |
| `rotating_logs` | `/workshifts_rotating_logs` | `batch_size`, `log_files` |
| `remote_checkpoint` | `/workshifts_remote_checkpoint` | `batch_size`, `sync_url` (**gated — leaves the machine**) |
| `autonomous` | `/workshifts_autonomous` | `batch_size` |
| `autorecover` | `/workshifts_autorecover` | `batch_size` |
| `throttled` | `/workshifts_throttled` | `batch_size`, `cooldown_sec` |
| `random` | `/workshifts_randomized` | `min_batch`, `max_batch` |
| `weighted` | `/workshifts_weighted` | `weights` |
| `scheduled` | `/workshifts_scheduled` | `batch_size`, `interval_sec` |
| `priority` | `/workshifts_priority_queue` | `queue` |

Anything else: **raise `Unknown mode: <mode>`** and stop — no near-match guessing, no fallback to
`checkpoint`. Echo the 16 valid modes with the error.

## Forwarding rules

- **Pass arguments through unchanged.** Don't reinterpret, rename, or helpfully rescale them; each target
  skill owns its own validation and defaults.
- **Omitted means default.** Forward only what was actually supplied; never substitute this skill's idea of
  a default for the target's.
- **Reject unknown arguments** for the chosen mode rather than dropping them silently — a typo'd
  `batch_sizes=` on `mode=checkpoint` should be an error, not a batch of 10 that ignored it.
- **`mode=until` needs `target_shifts`** and has no default for it. Ask rather than invent a bound.
- **`mode=remote_checkpoint` sends data off this machine.** Its confirmation gate runs in full — dispatching
  through this skill does not pre-approve the egress.

## Notes

- Announce the resolved mode and its effective arguments before starting, so the configuration is on the
  record from the first line of the run.
- Related dispatchers: `/workshifts_supervisor` covers 8 of these modes at defaults only; `/workshifts_god`
  wraps this skill with every parameter in one signature. This is the one to reach for day to day.
