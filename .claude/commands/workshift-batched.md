# Workshift — Batched (bounded by max batch count)

A **bounded** wrapper over the autonomous [`/workshift`](workshift.md) loop. It runs the exact same
supervised work loop (Foreman + crew + gauntlet), but adds one thing the plain workshift lacks: a **hard
stop after a fixed number of completed batches**, so you can start it, walk away (work / sleep), and know it
will wind down cleanly on its own instead of running forever.

Triggered by **"start a batched workshift"** / **`/workshift-batched [max_batches]`** /
**"run N workshift batches"**. Everything in [`workshift.md`](workshift.md) applies unchanged — read it first;
this file only adds the **batch bound**.

## What a "batch" is (the unit that gets counted)

One **batch = one completed unit of shippable work**, i.e. any ONE of:

- a ticket taken all the way through the gauntlet and **merged** (the common case), **or**
- a ticket **parked** by the failure circuit-breaker (3 attempts → `Blocked/`/`Deferred/` with a logged
  reason) — a stuck ticket is a *completed decision*, so it counts (this stops one unfixable ticket from
  silently consuming the whole budget), **or**
- when the ready queue is dry, one **PM/Researcher increment** that produces new ready work (an epic
  activated + decomposed, a research question filed to the Library) — counted so "keep the bench full" work
  is bounded too.

A batch is **not** an individual sub-agent, a single review, or a CI re-run — those are steps *within* a
batch. Count a batch when its ticket reaches a terminal state (merged / parked), not per PR-iteration.

## The bound

- **`max_batches`** — the ceiling. Default **40** (see sizing below). Passed as the first argument, e.g.
  `/workshift-batched 25`.
- Maintain a **persisted counter** at `.claude/workshift-metrics/BATCH-COUNTER` (gitignored, transient):
  a small JSON `{ "run_id": "...", "max_batches": N, "completed": K, "started": "<local ts>" }`. Increment
  `completed` by 1 at each batch's terminal state, rewrite the file, and surface `K/N` on the `• Budget —`
  line of every `FOREMAN` block (alongside the agent budget).
- The counter **survives a session reset**: at the sub-agent-budget reset line the plain workshift already
  checkpoints and hands off to a fresh session; the resuming session **reads `BATCH-COUNTER` and continues
  the same `completed`/`max_batches`** rather than restarting the count. So `max_batches` bounds the *whole
  batched run* across however many session-resets it takes, not a single session.

## Stop conditions — whichever fires FIRST ends the batched run

1. **Count reached** — `completed >= max_batches`: post the end-of-shift wrap (rich, plain-language, per the
   workshift reporting rules), then run the **shift-end teardown** from `workshift.md` (cancel the armed
   `ScheduleWakeup`, release `WORKSHIFT-LOCK`, final light Janitor pass), delete `BATCH-COUNTER`, and **stop**.
2. **Safe work genuinely exhausted** — the critical path is empty AND the PM/Researchers can't surface a
   headless-buildable, purpose-fitting next unit **without manufacturing filler**. Then **stop early and
   report** — do NOT pad the count with make-work. `max_batches` is a **ceiling, not a quota** (see honesty
   clause). Report `completed`/`max_batches` and *why* it stopped early.
3. **Budget reset line** — as before: quiesce → checkpoint (`CHECKPOINT.md`, including the `BATCH-COUNTER`
   state) → tell the user to resume in a fresh session. This is a *hand-off*, not a stop: the batched run
   continues under the same count.
4. **User returns** — the machine-sharing presence check from `workshift.md` (ask whether to yield); not an
   automatic stop.

## Sizing — pick a `max_batches` that outlasts your away-window

Real throughput with CI in the loop is roughly **2–4 batches/hour** when the ready queue is healthy (each
batch = build + independent Reviewer + UAT + CI + merge; CI legs alone are 5–35 min). So:

| Away window | Rough batches | Suggested `max_batches` |
|-------------|---------------|--------------------------|
| A night's sleep (~8 h) | ~16–32 | **30** |
| A full work day (~9–10 h) | ~20–40 | **40** (default) |
| A long day / "just keep going" | budget-capped | **50** (≈ the ~150-agent reset line at ~3–4 agents/batch — expect one checkpoint-and-resume) |

**Default = 40**: enough to run through a work day or a night, and it lands near the natural sub-agent-budget
reset boundary so at most one clean checkpoint-and-resume is needed. Above ~50, the run will span multiple
session-resets — fine, but expect the "start a fresh session and say resume" hand-off to fire once or twice.

## Honesty clause (the reason the plain infinite loop was rejected)

`max_batches` is a **hard ceiling, never a quota to be filled.** The batched workshift ships **real,
purpose-fitting work** — it never manufactures filler tickets, speculative code with no consumer, or busy-work
to reach a number. If genuine, headless, [PURPOSE.md](../../PURPOSE.md)-fitting work runs out at batch 12 of
40, it **stops at 12 and says so** — that is a *success*, not a shortfall. Honesty over completion
([`workshift.md`](workshift.md) → "Honesty over completion") outranks the batch count every time. This is
exactly why an unbounded `while True` batch loop is the wrong tool: it would keep "running batches" long after
the real work was done.

## Reporting

Per `workshift.md`, plus: the **roll-call banner** states the bound (`Batched run: up to N batches`), **every
batch boundary** posts its rich summary and the running `K/N`, and the **final wrap** leads with
`Batched run complete — K of N batches` (or `stopped early at K/N — <why>`). Every `FOREMAN` block's
`• Budget —` line shows `batches K/N` next to the agent budget.

---

*This skill is additive over [`workshift.md`](workshift.md); when the two disagree on anything other than the
batch bound, `workshift.md` wins. Distinct from the concurrent `workshifts_*` skill-family work (CPE-1476) —
coordinate, don't collide.*
