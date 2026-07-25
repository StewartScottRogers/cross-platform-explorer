# Workshift metrics — the teeth for the Capacity & throughput discipline

This folder is the measurement substrate for the Workshift's **Capacity & throughput** discipline
(see `.claude/commands/workshift.md`). It turns "run the right number of workers on the right model"
from a vibe into a number the Foreman can consult.

## Files

| File | Committed? | What it is |
|------|-----------|------------|
| `ledger.jsonl` | **gitignored** | Append-only raw row per finished sub-agent this machine. Transient + noisy; local detail only. |
| `history.md` | committed | Small, human-readable rolling learnings appended at each end-of-shift. Shared across CLI + desktop shifts so the next shift starts smarter. |
| `README.md` | committed | This file — the format spec. |

## `ledger.jsonl` row schema (one JSON object per line)

```json
{
  "ts":         "2026-07-25T20:14:03-07:00",   // when the row was written (agent returned), ISO-8601 local
  "role":       "worker",                        // worker | reviewer | uat | researcher | janitor | pm
  "ticket":     "CPE-1035",                      // ticket / epic the agent served ("-" if none)
  "class":      "metadata-codec",                // coarse ticket class, for cross-ticket learning
  "model":      "sonnet",                         // haiku | sonnet | opus | fable
  "dispatched": "2026-07-25T20:02:41-07:00",
  "returned":   "2026-07-25T20:14:03-07:00",
  "elapsed_s":  682,                              // MEASURED (returned - dispatched)
  "outcome":    "merged",                         // merged | changes-requested | uat-fail | skipped | stuck-escalated | failed
  "retries":    1,                                // review/UAT round-trips or model escalations (waste signal)
  "cost_proxy": 2728                              // PROXY, not tokens: tier_weight * elapsed_s (see below)
}
```

## Honest measurement — what's real vs a proxy

- **`elapsed_s` is REAL** — computed from wall-clock `date` at dispatch and return. Measure it, don't guess.
- **`cost_proxy` is a PROXY, not a token count.** The `Agent` tool does not reliably surface a sub-agent's
  token usage back to the Foreman, so **never fabricate token numbers.** Instead approximate relative spend as
  `tier_weight × elapsed_s`, with rough public price-ratio weights:

  | model | tier_weight |
  |-------|-------------|
  | haiku  | 1  |
  | sonnet | 4  |
  | opus   | 15 |
  | fable  | 4  |

  It's a coarse *relative* cost, useful only for "which tier/class is eating the budget" — label it a proxy
  everywhere it surfaces. `retries` is the companion **waste signal** (rework the shift paid for).

## Derived signals (compute from the rows, drive decisions)

- **Throughput** — `merged` rows per hour.
- **Time-in-gauntlet** — median `elapsed_s` for worker→merged.
- **Review round-trips** — mean `retries` per merged ticket.
- **Stuck / retry rate by (model, class)** — the key feedback signal: if a `(class, tier)` pair shows high
  `retries`/`stuck-escalated`, that class's default tier is too weak → bump it. If `opus` elapsed ≈ `sonnet`
  elapsed with the same outcome on a class, that class is over-provisioned → downgrade it.

## `history.md` — cross-shift learning

At each end-of-shift the Foreman appends a short block: date, tickets shipped, and the tuned defaults it
learned (e.g. "metadata-codec: sonnet, 2-wide, ~11m median, 0 stuck"). The next shift's kickoff reads the
tail of `history.md` to **seed** model/parallelism defaults instead of relearning from cold. Keep each entry
to a few lines — this is distilled learning, not a raw dump (that's what `ledger.jsonl` is for).
