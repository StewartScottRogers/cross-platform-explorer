# WORKSHIFTS.md — the `workshifts_*` skill family standard

`/workshift` is the autonomous work loop: a Foreman dispatching Workers, running each PR through the
gauntlet, and merging. It runs until the work is done or the user returns.

The **`workshifts_*` family** is a layer *above* that: 19 skills that differ only in **how they drive
`/workshift` in repeated batches** — how progress is persisted, how batch size is chosen, how failures are
absorbed, and how the loop is paced.

Every one of them calls the same primitive. This file defines it once so the 19 skills stay thin and
cannot drift apart. **Each `workshifts_*` skill is a loop policy, not a second work engine.**

---

## The one primitive — `run_workshift_batch(batch_size)`

> Drive `batch_size` tickets through the full `/workshift` gauntlet, then **return**.

Concretely, one batch is:

1. **Pre-flight** (first batch of a run only) — acquire `.claude/workshift-metrics/WORKSHIFT-LOCK`, verify
   the tree is clean and `main` is green, verify the substrate dirs. See `/workshift` § Pre-flight.
2. **Select** `batch_size` provably-non-overlapping ready tickets, following the `/workshift` critical path
   (`Doing/` → `Backlog/` + pickable `Deferred/` → activate an epic → PM picks new epics). Fewer than
   `batch_size` available means the batch is **short**, not padded — record the shortfall.
3. **Dispatch** one Worker sub-agent per ticket, `isolation: "worktree"`, right-sized model.
4. **Gauntlet**, pipelined per PR: Worker self-verify → independent Reviewer + UAT Tester (+ Visual Critic /
   Accessibility Auditor on GUI diffs, Security Auditor on risky diffs) → CI.
5. **Merge** the approved + green + disjoint PRs, one lock at a time; push; append ledger rows.
6. **Quiesce** — no sub-agent still in flight — then **return**. A batch that returns while agents are
   running is a bug: every wrapper below assumes the boundary is quiet.

A batch **returns a receipt**, and every wrapper persists or reports some part of it:

```
{ batch_index, batch_size_requested, tickets_completed, tickets_short,
  prs_merged, prs_failed, agents_spent, started_at, ended_at }
```

`batch_size` is a **ticket count**, not a time box. It is the same knob as the
[[workshift-subagent-budget-reset]] bounded-batch rule: small batches mean frequent safe stopping points.

---

## Shared rules — every skill in the family obeys these

**Loop termination.** The spec writes `while True`. In this repo that means the `/workshift` keep-rolling
loop, and exactly three things end it:

1. the user says **"stop the workshift"**,
2. the user returns (presence check), or
3. a hard-stop safety condition fires (broken base that can't be fixed, lock lost, budget wall).

Running out of tickets is **not** a stop — the PM picks the next epic and the loop rolls on
([[workshift-do-next-epic-always]]). A finite skill (`workshifts_until`) additionally stops at its target.

**No foreground sleep.** `time.sleep(n)` in the spec becomes `ScheduleWakeup` (or a `Monitor` until-loop).
Foreground `sleep` is blocked in this harness, and blocking the turn would stall the heartbeat. Cooldowns
and intervals are *scheduled*, never spun on.

**No OS threads.** `Thread(target=...)` becomes `Agent` sub-agents with `isolation: "worktree"`. Parallelism
is agents-in-worktrees; the merge lock is still serial, always.

**Files land in the substrate.** `open("log1.txt","a")` writes to
`.claude/workshift-metrics/<name>`, never the process CWD. That directory is the committed substrate the
`/workshift` ledger already uses. Checkpoint and log files are **gitignored run state**, not tracked work
product, unless a skill says otherwise.

**Timestamps on every tick.** Each batch boundary prints a wall-clock timestamp and, when the loop yields,
the next-wake time ([[loop-behavior-needs-timestamps]]).

**Report at every batch boundary.** A rich plain-language line — what shipped, what failed, what's next —
not just a counter ([[workshift-report-each-epic]], [[workshift-summarize-with-context]]).

**Failures use the circuit breaker.** Retryable errors (529/429/5xx, stalled agents) get bounded
exponential-backoff retry *and* reduced concurrency; after the cap, escalate to the user
([[circuit-breaker-for-retryable-errors]]). No skill in this family may retry a permanent failure forever.

**Budget reset.** As the session agent count approaches the reset line, quiesce → checkpoint → hand off to
a fresh session mid-loop. The batch boundary is exactly the right place to do it.

---

## The family

| # | Skill | Loop policy |
|---|-------|-------------|
| 1 | `/workshifts_checkpoint` | Persist a resumable JSON checkpoint after each batch |
| 2 | `/workshifts_log` | Append a timestamped line to a disk log after each batch |
| 3 | `/workshifts_rotate` | Cycle batch size through a fixed list |
| 4 | `/workshifts_until` | **Finite** — run until a target ticket count is reached |
| 5 | `/workshifts_parallel` | N concurrent worker lanes, each looping |
| 6 | `/workshifts_gpu` | A single named lane (`gpu_id` is a label — see that skill) |
| 7 | `/workshifts_vr_dashboard` | Render a live dashboard after each batch |
| 8 | `/workshifts_rotating_logs` | Round-robin across several log files |
| 9 | `/workshifts_remote_checkpoint` | POST cumulative progress to a remote URL (**gated**) |
| 10 | `/workshifts_autonomous` | Catch, report, and continue past batch errors |
| 11 | `/workshifts_autorecover` | Catch silently and immediately re-run the failed batch |
| 12 | `/workshifts_throttled` | Fixed cooldown between batches |
| 13 | `/workshifts_randomized` | Random batch size in a range |
| 14 | `/workshifts_weighted` | Random pick from named batch-size classes |
| 15 | `/workshifts_scheduled` | Fixed interval between batch *starts* |
| 16 | `/workshifts_priority_queue` | Always run the head of a priority queue |
| 17 | `/workshifts_supervisor` | Dispatch to 8 of the above, at their defaults |
| 18 | `/workshifts` | Unified meta-skill — dispatch to 16 modes, with arguments |
| 19 | `/workshifts_god` | Every knob in one signature, forwarded to `/workshifts` |

---

## Deviations from the literal specification

The family was specified as pseudo-Python. Four constructs have no honest literal implementation here, so
they are implemented in the nearest real form and each skill says so in its own text:

| Spec | Why it can't be literal | What it does instead |
|---|---|---|
| `send_checkpoint_to_remote(url, …)` with `https://example.com/sync` | A placeholder host, and a POST is outward-facing | Requires a real URL + explicit user confirmation before the first sync; counters only, never diffs/paths/secrets |
| `except: continue` (skills 10, 11) | Hot-loops forever on a permanent failure | Bounded exponential backoff, then escalate |
| `gpu_run_workshift_batch(…, gpu_id)` | There is no GPU compute in this pipeline | `gpu_id` is a lane label + concurrency slot; no acceleration is claimed |
| `render_vr_dashboard(msg)` | There is no VR device | Renders a real dashboard (Artifact page or ASCII panel) |

Everything else — parameter names, defaults, dispatch tables, and ordering — matches the specification
exactly.
