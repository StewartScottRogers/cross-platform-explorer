# workshifts_priority_queue — Priority-Headed Batch Loop

Run `/workshift` in repeated batches sized by the **head of a priority queue**, so the highest-priority class
of work sets the batch size every cycle.

Read [docs/design/WORKSHIFTS.md](../../docs/design/WORKSHIFTS.md) first — it defines
`run_workshift_batch(batch_size)` and the loop rules shared by the whole family.

## Parameters

Parse from `$ARGUMENTS` (`key=value`, any order; all optional):

| Parameter | Default | Meaning |
|---|---|---|
| `queue` | `[("high", 20), ("medium", 10), ("low", 5)]` | Ordered `(priority, batch_size)` pairs, highest first |

## Procedure

Loop until a family stop condition fires:

1. `priority, bs = queue[0]` — always the head.
2. `run_workshift_batch(bs)`, drawing tickets of that priority first.
3. Report `Completed <priority> priority batch of <bs>. Restarting...` plus a plain-language line on what
   shipped.

## The head never moves — read this before implementing

**As specified, this loop reads `queue[0]` every iteration and never pops, rotates, or advances.** With the
default queue it runs `high`/20 forever; `medium` and `low` are never reached. That is the specified
behaviour and it is implemented as written — but it is a strict-priority policy, not a rotation, so:

- **Say which entries are unreachable at kickoff.** Announce that only `queue[0]` will run and name the
  starved entries. A user who expected round-robin should find that out in the first line, not from a log
  three hours later.
- **Starvation is the point, not a bug.** Strict priority means lower classes wait indefinitely by design.
  That is a legitimate policy — just never report it as "working the queue".
- **Fall back rather than idle.** If no tickets of the head priority are ready, the batch is short. Draw the
  remainder from the next queue entry down and **say so in the report** — the alternative is spinning empty
  batches against an exhausted class while real work waits. This is the one place the loop looks past the
  head, and it is always reported.
- If a genuine rotation is wanted, `/workshifts_rotate` cycles deterministically and `/workshifts_weighted`
  samples across classes. Neither is a drop-in replacement for this one's semantics.

## Notes

- Priority here means the ticket's `priority:` frontmatter (see `Ticketing/wiki.md`), mapped to the queue's
  labels. A queue label matching no tickets is an empty class, and is reported as such.
- Validate: `queue` non-empty, each entry a `(label, size)` pair with `size >= 1`.
- The critical-path ordering in `/workshift` still applies inside a batch — `Doing/` before `Backlog/`,
  blocked tickets skipped — priority selects *among* ready tickets, it doesn't override the path.
