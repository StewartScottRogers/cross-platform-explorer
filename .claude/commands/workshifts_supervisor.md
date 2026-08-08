# workshifts_supervisor — Mode Dispatcher (defaults only)

Pick one of eight `workshifts_*` loop policies by name and run it **at its own defaults**.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS`:

| Parameter | Default | Meaning |
|---|---|---|
| `mode` | `checkpoint` | Which loop policy to run |

A bare argument is taken as the mode, so `/workshifts_supervisor gpu` works. With no argument at all, run
`checkpoint`.

## Dispatch table

| `mode` | Runs |
|---|---|
| `checkpoint` | `/workshifts_checkpoint` |
| `log` | `/workshifts_log` |
| `rotate` | `/workshifts_rotate` |
| `random` | `/workshifts_randomized` |
| `gpu` | `/workshifts_gpu` |
| `parallel` | `/workshifts_parallel` |
| `priority` | `/workshifts_priority_queue` |
| `autorecover` | `/workshifts_autorecover` |

Anything else: **raise `Unknown mode: <mode>`** and stop. Do not guess at a near-match, and do not fall back
to `checkpoint` — silently running a different loop than the one asked for is worse than an error. Listing
the eight valid modes alongside the error is fine; picking one for the user is not.

## This dispatcher passes no arguments — by design

Every target is invoked **with its own defaults**: `batch_size=10`, `workers=2`, `gpu_id=0`,
`batch_sizes=[5,10,20]`, and so on. `mode` is the only knob this skill has.

That is deliberate in the specification, and it is the difference between this skill and `/workshifts`. If
the run needs any parameter changed, use **`/workshifts`** (same policies, full arguments) or call the target
skill directly. Do not extend this one to forward kwargs — `/workshifts` already is that skill.

## Notes

- Eight modes, not sixteen. `until`, `vr`, `rotating_logs`, `remote_checkpoint`, `autonomous`, `throttled`,
  `weighted`, and `scheduled` are **not** reachable from here — they are reachable from `/workshifts`. If a
  user asks for one of those by name, point them there rather than erroring blankly.
- Announce the resolved mode and its defaults before starting, so the shift's configuration is on the record
  from the first line.
