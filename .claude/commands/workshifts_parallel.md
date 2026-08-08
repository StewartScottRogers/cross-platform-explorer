# workshifts_parallel — Parallel Worker Lanes

Run **N concurrent lanes**, each looping `/workshift` batches independently, and join them all.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `workers` | `2` | Concurrent lanes |
| `batch_size` | `10` | Tickets per batch, per lane |

## Procedure

1. Start `workers` lanes. Each lane loops until a family stop condition fires:
   1. `run_workshift_batch(batch_size)` for that lane's ticket slice.
   2. Report `[Worker <id>] Completed batch of <batch_size>. Restarting...`.
2. **Join** — the skill returns only when every lane has ended. A lane that dies takes its ticket slice with
   it; re-dispatch that slice rather than letting the run finish quietly short.

## How the lanes are actually run — read this before implementing

The spec says `Thread(target=worker)`. There are no OS threads here, and translating this wrong is how a
parallel shift corrupts a checkout.

- A lane is an **`Agent` sub-agent with `isolation: "worktree"`** — its own checkout, so lanes editing
  files at the same time cannot collide.
- **Ticket slices must be provably disjoint.** Partition ready tickets across lanes *before* dispatch and
  verify no two slices touch the same files. Two lanes on overlapping tickets produce conflicting PRs, and
  the merge queue eats the difference.
- **The merge lock stays serial.** Lanes build and review in parallel; merging to `main` is one at a time,
  always, holding the lock. Parallelism buys throughput on the slow legs, not on `main`.
- `workers` is not free. Each lane multiplies sub-agent spend against the session budget — size it to the
  ready queue's real depth, and quiesce all lanes together at the budget reset line.
- Never delete or prune another lane's worktree while it is live ([[janitor-never-rmrf-active-worktrees]]);
  a "stale" worktree is often a running one.

## Notes

- Lane reports interleave. Tag every line with its lane id or the output is unreadable.
- With a shallow queue, extra lanes just produce short batches. Two busy lanes beat six starved ones.
