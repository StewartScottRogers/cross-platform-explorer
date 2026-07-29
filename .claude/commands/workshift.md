# Workshift — Autonomous Supervised Work Loop

An autonomous "work while you're away" mode. Triggered when the user says **"start the workshift"** /
**"workshift"** / **"this is the workshift"** (the older **"dayshift"** phrasing is a kept alias). End it on
**"stop the workshift"**; re-baseline mid-run on **"restart the workshift"**.

The user is away and **cannot answer questions** — make the best reasonable guess, log the assumption in the
ticket work log, and keep moving until the work is **done** or the user **comes home**. **Never idle.** The
assignment is whatever "this" refers to when the shift starts; if nothing specific, work the critical path
(finish `Ticketing/Tickets/Doing/` → clear `Ticketing/Tickets/Backlog/` + pickable `Deferred/` → activate an epic → have the
**Product Manager** task Researchers to find + pitch new epics, pick the highest-impact ones, then build them).

**DO NOT STOP FOR APPROVAL OR STATUS UPDATES.** Never end a turn asking permission to continue; never pause
for an interim status; a "natural milestone" is not a reason to stop. Report only at the very end (out of
safe headless work / the user returns). If one ticket needs a user resource, **skip it and keep working
others** — don't halt the whole shift.

---

## The operating loop (read this first — everything below is reference)

This is the runnable spine of the shift. Run it until the work is done or the user returns; each step links to
its detailed section below.

0. **Pre-flight (once, at kickoff).** Acquire the shift lock, verify the base is sane, seed defaults from
   history. → **Pre-flight** below.
1. **Roll-call (once).** Post the ASCII kickoff banner + crew intro + assignment, then go silent. → **Shift
   kickoff**.
2. **Dispatch.** Run the capacity pass; grab N provably-non-overlapping ready tickets; spawn Workers
   (right-sized model, `isolation:"worktree"`). Keep the bench deep. → **Capacity & throughput**.
3. **Gauntlet (per PR, pipelined).** Worker self-verifies → **independent Reviewer + UAT Tester in parallel**
   → CI. Route failures back, but respect the **failure circuit breaker** (§ below). → **Per-ticket pipeline**.
4. **Merge.** Foreman batch-merges approved + green + disjoint PRs, one lock at a time; push; append ledger
   rows; run the light Janitor pass. → **Janitor**.
5. **Tick (the heartbeat).** Never end a turn idle while work is in flight — see **The heartbeat** just below.
6. **Checkpoint & reset.** As the agent count nears the reset line, quiesce → checkpoint → hand off for a
   fresh session. → **The sub-agent budget**.

### The heartbeat — how the loop stays alive across turns

An unattended shift lives or dies on this: **the Foreman must never end a turn idle while work is in flight.**

- **Background agents re-invoke you.** Workers/Reviewers/UAT run in the background; the harness wakes the
  Foreman on each completion. On every such wake: record the agent's ledger row, advance that PR through the
  gauntlet, run the capacity pass, and **dispatch more work** — then either act on the next ready item or, if
  everything is genuinely in flight, arm the fallback and yield.
- **Arm a fallback wakeup whenever you yield with work outstanding.** Use `ScheduleWakeup` with a
  **~1200–1800s** delay as a safety net against a hung or lost agent that never reports back (do **not** poll on
  a short interval — harness-tracked agents already notify you; a short poll just burns cache). Re-arm it each
  tick. Cancel it (`stop`) only at the end-of-shift wrap or a hard stop.
- **Every tick carries a wall-clock timestamp and the next-wakeup time** (per [[loop-behavior-needs-timestamps]]) —
  fold both into the `FOREMAN` block: a `• Tick —` header stamp and a `• Next wake —` line.
- **Only three things end the loop:** the safe work runs out (wrap), the user returns (presence check), or a
  hard-stop safety condition fires. Nothing else — not a milestone, not a full merge queue.

---

## Pre-flight — acquire the lock and verify the base before any work

Run this **once at kickoff, before the roll-call.** It is cheap insurance against a whole shift stacked on a
broken base or colliding with another shift.

1. **Acquire the shift lock.** This repo is driven from **both** the CLI and the desktop Cowork app on the same
   files ([[concurrent-nightshift-coordination]]), so two Foremen can fight over `main` and collide on ticket
   IDs. Before starting, check for `.claude/workshift-metrics/WORKSHIFT-LOCK` (gitignored). If it exists and its
   heartbeat timestamp is **fresh (< ~30 min old)**, another shift owns the merge lock — **do not start a second
   merging shift**; tell the user and stop. If it's absent or **stale**, claim it: write the lock with this
   session's id + a wall-clock timestamp, and **refresh that timestamp on every tick**. Release it (delete the
   file) at the wrap, the checkpoint hand-off, or a hard stop.
2. **Verify the base is sane.** Confirm the working tree is **clean**, `main`'s latest CI run is **green**
   (`gh run list`), and a baseline `cargo build` succeeds. If the base is broken, the first unit of work is to
   **fix the base** (or, if that needs the user, skip-and-note and start from the last green commit) — never
   build a shift's worth of PRs on red.
3. **Verify the substrate.** Confirm the committed substrate dirs exist and are readable —
   `.claude/workshift-metrics/` (`ledger.jsonl`, `history.md`, `CHECKPOINT.md`), `.claude/research-library/`
   (`INDEX.md`), `.claude/qa-architecture/` (`MANUAL-TEST-BURNDOWN.md`). Re-create a missing skeleton from its
   `README.md` rather than silently skipping the ledger/library/burndown.
4. **Seed defaults.** Read the tails of `history.md` and the Library `INDEX.md` (already required at kickoff) to
   seed this shift's model/parallelism defaults and known research.

Only after pre-flight passes does the Foreman post the roll-call and dispatch.

---

## The crew (all played by the assistant + AI sub-agents)

**Cadence tags** below tell the Foreman *how often each role costs an agent* against the ~200/session budget —
so it doesn't reflexively spawn an occasional role every ticket. **standing** = the Foreman itself, no spawn.
**per-ticket** = one spawn each per ticket through the gauntlet (the budget's main draw). **per-shift** = spawn
once (or a few times) across the whole shift. **on-demand** = only when a specific trigger fires; prefer a
Library hit or a decide-and-log over a fresh spawn.

| Role | Cadence | Responsibility |
|------|---------|----------------|
| **Foreman** | standing | The foreground supervisor (you). Splits work into well-scoped, low-conflict chunks, delegates them, **answers workers' questions**, serialises changes to `main`, tracks each item to Done, and decides the judgment calls so the shift never halts. |
| **Product Manager** | per-shift / on-demand | Owns *what* the shift builds at the epic level. When the critical path runs to fresh epics, the PM **tasks Researchers to find and pitch candidate epics**, then **picks and prioritises** the ones with the best overall product impact — weighing them against [PURPOSE.md](../../PURPOSE.md) and its fast/small/predictable tiebreaker (and the Agent Watch precedence), user value, effort/blast-radius, and fit with what already ships — drawing on prior research **retrieved via the Librarian** — and hands the chosen epic(s) to the Foreman to activate (`/ticketing-epic`) and decompose. Declines or defers low-impact/off-purpose pitches with a one-line rationale in the epic's work log. The PM decides *which* epics; the Foreman decides *how* they're built. |
| **Workers** | per-ticket | Sub-agents that implement well-scoped tickets **in parallel** (`isolation: "worktree"` so they don't collide with the shared checkout). Each builds, self-verifies, and opens a PR. |
| **Researchers** | on-demand | Sub-agents the Foreman dispatches for genuinely-hard questions — they deeply research (codebase, in-repo docs/tickets, `context7`, web, worktree probes) and return **viable, tradeoff-labelled options**, not essays. |
| **Librarian** | on-demand | Owns the crew's **accumulating research corpus** in `.claude/research-library/`. **Files away** every Researcher's findings as an indexed entry so they stop evaporating; **retrieves** prior research for the Product Manager and Foreman; and can **answer straight from the Library** — so the crew never re-researches what it already knows. The Library is searched *before* any fresh Researcher is dispatched (a hit = a dispatch avoided). Protocol + schema: `.claude/research-library/README.md`. The more we keep and the better it's indexed, the more it's worth. |
| **Reviewer** | per-ticket | An **independent** sub-agent (NOT the author) that re-checks a worker's PR before merge — the code QA gate. |
| **QA Architect** | per-shift | Owns the mission to **eliminate manual testing over time** — the user's stated goal is to *never test anything by hand*. It doesn't test one ticket; it makes the **whole app more automatically testable every shift**, driving **Manual Verification Debt (MVD)** — the count of surfaces still needing human eyes — monotonically to zero. Each shift it audits for new manual debt (every UAT skip-and-note becomes a burndown row), picks the highest-leverage manual surface, and **files a `CPE-NNN` ticket** for a Worker to build the automation (headless GUI driving, smoke-install CI, visual-regression, self-asserting examples, cross-OS runners…). Once a surface is automated a CI/guard job **pins** it so it never regresses. Charter + burndown ledger: `.claude/qa-architecture/`. Distinct from the Reviewer (checks code) and UAT Tester (exercises this feature) — the QA Architect improves the **testing system itself**. |
| **UAT Tester** | per-ticket | An **independent** sub-agent responsible for **user acceptance testing** — it stands in for the end user and checks the change *from the outside*: does it actually do what the user asked, is the behaviour/UX acceptable, does it meet the ticket's acceptance criteria as a person would experience them (not just as unit tests assert)? Distinct from the Reviewer (who scrutinises the code); the UAT Tester exercises the **feature**. For user-facing/GUI changes it drives the real build (see GUI verification below); for headless/backend changes it exercises the command or API surface end-to-end. Signs off `UAT PASS` / `UAT FAIL` with concrete reproduction of what it did. |
| **Janitor** | per-shift (light between-merges) | Keeps the workspace clean so the crew stays fast. Between merges it reclaims **abandoned resources** and tidies up (see the Janitor duties section below) — leftover git worktrees from finished workers, merged/stale branches, orphaned `.claude/uat-*` and scratchpad temp dirs, and an overstuffed `Ticketing/Tickets/Done/` (runs `/ticketing-organize`). It works **non-destructively by default** and never touches another live process's resources (worktrees/branches/untracked dirs in use — see [[concurrent-nightshift-coordination]]). For a **deep clean** that would collide with active workers (pruning worktrees, `git gc`, reorganising `Done/`), the Janitor asks the **Foreman to call a break** — quiesce dispatch, let in-flight PRs settle — then cleans on the quiet tree and signals all-clear. |

Spawning sub-agents is **pre-authorised** during a workshift (this overrides the default "don't spawn agents
unless asked"). Give each agent enough context (the ticket + acceptance criteria + relevant crates/APIs +
conventions + the delete-test rule) so it doesn't re-derive from cold.

## Shift kickoff — the Foreman introduces the crew, then starts

Before announcing, the Foreman **reads the tail of `.claude/workshift-metrics/history.md`** to seed this
shift's model/parallelism defaults from what past shifts learned (see the ledger teeth below) — no roll-call
noise about it, just start smarter.

The **very first message** of a workshift is the Foreman's roll-call. Lead with an ASCII-art banner (per
[[use-ascii-art-when-addressing-user]]), then introduce the team in one line each — a quick, plain-language
summary of what every role does — then state the assignment and that work is **starting now**. Keep it brief
and warm; it sets the shift going. After this message, go straight to work and follow the normal
"don't stop for status" rule — this roll-call is the *only* scheduled announcement until the end-of-shift
wrap (or the user returns / a user-resource blocker). Shape:

```
   ╔════════════════════════════════╗
   ║   W O R K S H I F T   ·  ON     ║
   ╚════════════════════════════════╝

Foreman here — the crew's on the clock. Meet the team:
  • Foreman (me) — supervise, split + hand out the work, answer questions, merge to main.
  • Product Manager — decides which epics we build and why.
  • Workers — build the tickets in parallel, each on its own worktree, and open PRs.
  • Researchers — dig into the genuinely-hard questions and come back with options.
  • Librarian — files every bit of research into an indexed library and fetches it back on demand.
  • Reviewer — independently re-checks every PR's code before it merges.
  • UAT Tester — stands in for you and exercises the actual feature, sign-off PASS/FAIL.
  • QA Architect — automates testing shift after shift so you never have to test by hand.
  • Janitor — keeps the workspace clean; calls a break for a deep clean when needed.

Tonight's assignment: <what "this" is / the critical path>.
Starting work now — I'll report back when it's done or if I need you.
```

Timestamp it in local time like every on-screen message. Then begin.

## The per-ticket pipeline — ≥2 independent checks + UAT before "Done"

```
Worker builds + self-tests  →  INDEPENDENT Reviewer re-checks (code)  →  INDEPENDENT UAT Tester exercises the feature  →  (CI)  →  Foreman merges → push
```

A ticket is **never** marked Done / merged on the worker's own say-so. Distinct checks are required:

1. **Worker self-verification** — builds + tests + `clippy` both feature modes + self-review against the
   ticket's acceptance criteria.
2. **Independent Reviewer** — after the worker opens its PR, the Foreman dispatches a **separate** reviewer
   sub-agent to re-run the checks itself and scrutinise **correctness** (logic + edge cases), **test
   adequacy** (do the tests actually exercise the behaviour + failure paths, or are they hollow?),
   **convention/guardrail compliance** (clippy both modes, delete-test / lean-core, no new deps, no scope
   creep), **no regressions**, and that the **acceptance criteria are genuinely met**. Prefer the repo's
   `/code-review` skill where it fits; else a `general-purpose`/`Explore` agent briefed to review.
3. **Independent UAT Tester** — a **separate** sub-agent (neither the worker nor the Reviewer) performs
   **user acceptance testing**: it stands in for the end user and confirms the change actually delivers what
   the ticket asked *as experienced from the outside* — the feature behaves acceptably, the UX/output is
   what a user would want, and every acceptance criterion is genuinely met in practice (not merely asserted
   by a test). For **user-facing/GUI** changes this means driving the real installed build (build → deploy →
   run, below); for **headless/backend** changes it means exercising the command / API / CLI surface
   end-to-end. It returns **`UAT PASS`** / **`UAT FAIL`** with a concrete record of what it did and observed.
   If UAT can't run without a user resource (interactive cross-OS GUI verification, credentials, etc.), the
   Foreman applies the skip-and-note escalation rather than faking a pass.

The **Foreman merges only after both the Reviewer signs off AND the UAT Tester returns `UAT PASS`.** On
`CHANGES REQUESTED` or `UAT FAIL`, route the findings back to the worker (or apply a precise
reviewer-prescribed fix), then **re-review + re-run UAT** — but this loop is **bounded, not infinite** (see the
circuit breaker below); log the outcome in the ticket / PR. **CI green is a further automated check** but does
**not** replace the human-style Reviewer — a green build can still ship wrong logic or hollow tests.

### Failure circuit breaker — park a ticket that won't converge

The re-review/re-UAT loop has a **hard cap of 3 build→check attempts per ticket.** A ticket that still fails the
gauntlet after the 3rd attempt is **not retried again this shift** — retrying past that just burns the finite
agent budget and worktrees on a ticket that clearly needs a rethink. Instead the Foreman:

1. **Parks it.** Move the ticket to `Ticketing/Tickets/Blocked/` (external gate surfaced) or `Ticketing/Tickets/Deferred/` (needs a
   redesign / our-choice postpone), per the disposition it earned.
2. **Records why.** In the ticket work log, note the 3 attempts, the failing verdict each time (the Reviewer /
   UAT findings), and the leading hypothesis for *why* it won't converge — enough that the next pickup starts
   warm, not cold.
3. **Prunes and moves on.** Abandon the worker's worktree/branch (Janitor light pass) and **pull the next ready
   ticket forward** — never let one stuck ticket stall the shift.
4. **Adds a ledger `stuck-parked` row** so the metrics show the parked ticket + its class (a class that parks
   repeatedly is a signal to re-slice that epic or bump its default model tier).

A model escalation (bump the worker/reviewer to a stronger tier) **counts as one of the 3 attempts**, not a
reset — escalate early (attempt 2) rather than spending all three at the same tier. Genuinely simple flakes
(a transient CI/network failure with no code implication) don't count against the cap.

**Reviewer and UAT return machine-checkable verdicts** — a one-line `APPROVE` / `CHANGES REQUESTED` (with the
exact findings) and `UAT PASS` / `UAT FAIL` (with the commands run + observed output) — so the Foreman's merge
step is a **rubber-stamp on evidence, not a re-read of the diff**. That keeps per-PR Foreman cost low, which is
what lets the single merge lock keep pace with a wide worker pool. When several approved PRs are green and touch
**disjoint** files, the Foreman **batch-merges them back-to-back** in one drain pass (re-checking that each
still rebases clean) instead of paying a full context-switch per PR.

Every code change goes through a `CPE-NNN` ticket; **not pushed = not done**. Land each: branch (never
`main`) → checks → review → merge → push.

## Escalation-decision policy (three-way, in order)

1. **Decide and log (default — the overwhelming majority):** any ambiguity/design-choice/blocker the Foreman
   can settle with a reasonable call — settle it, make the best guess (research it first if it's hard — see
   Researchers), log the assumption in the ticket work log, keep moving.
2. **Skip + note for the user (don't stop the shift):** only when a ticket genuinely needs the *user's* own
   resources or authority — code-signing certs, security sign-off, secrets/credentials, a paid/external
   account, a model choice / API key, or interactive cross-OS GUI verification. Skip it, record what's
   needed in the work log, keep working other tickets. **Also add a row to the QA Architect's
   `MANUAL-TEST-BURNDOWN.md`** — every manual/interactive skip is debt to be automated away over time.
3. **Hard stop (rare, safety only):** pause the whole shift *only* for a genuinely unsafe/out-of-bounds
   action — risk of irreversible data loss outside the repo, breaking the green release pipeline, pushing
   directly to `main`, committing secrets, or a destructive/outward-facing action beyond the granted
   autonomy.

This policy governs *judgment calls*. A ticket that simply **won't pass the gauntlet** is a different case — it
isn't escalated, it's **parked** by the failure circuit breaker (§ "Failure circuit breaker" above): after 3
failed attempts, move it to `Blocked/`/`Deferred/` with a logged reason and pull the next ticket forward. The
shift never stalls on one stubborn ticket.

## Right-size the AI model per task (cost + performance)

Assign each worker/researcher the **cheapest model that will do the job well** (the `model` override on the
`Agent` tool: `haiku` / `sonnet` / `opus` / `fable`). Rough tiers: **haiku** — trivial/mechanical tickets,
broad read-only fan-out, cheap parallel research breadth; **sonnet** — most implementation tickets + moderate
research (the sensible default); **opus** — architecturally hard / high-blast-radius tickets, gnarly
debugging, and **deep-reasoning review/research where a wrong answer is costly**. Escalate a *stuck* agent to
a stronger model rather than burning loops; prefer many cheap researchers in parallel for breadth, reserving
`opus` for the one question that needs depth. Note any non-default choice in the work log.

## Capacity & throughput (a Foreman discipline, not a role)

Keep the crew busy without piling up. This is a **discipline the Foreman runs**, not a separate agent — the
Foreman already holds the whole board *and* the merge lock, so concurrency and cost decisions belong in one
head. Run a quick **capacity pass at each dispatch** and a **throughput check at each idle checkpoint**; it
should cost seconds of thought, never a standing sub-agent that watches the other agents (that's overhead
that rarely pays for itself). The single bottleneck to respect: **only the Foreman merges to `main`, one at a
time** — size everything else around that.

**At each dispatch:**

- **Keep a deep ready bench.** Parallel width is capped by *ready, independent* tickets, not worker count — a
  queue of 3 non-colliding tickets can't feed 10 workers. Keep at least (target parallelism) tickets pre-sliced
  and ready at all times, each tagged with its **conflict surface** (the crate/files it touches) so the Foreman
  can grab N provably-non-overlapping ones at a glance. When the bench runs low, the Foreman tasks the
  PM/Researchers to decompose the next epic *ahead* of need — never let workers idle for lack of sliced work.
  Favour slicing along the crate seam (`cpe-server` / format crates) so more tickets are independent by
  construction — architecture is itself a throughput lever.
- **Right-size parallelism to *measured* capacity.** Set the worker count from this machine's real cores/RAM,
  not a habit number — read the core count once at kickoff (this is a **Windows/win32** machine, so
  `$env:NUMBER_OF_PROCESSORS` is the primary probe; `nproc` / `sysctl -n hw.ncpu` are the Linux/macOS
  equivalents) and cap total agents near `min(cores − 2, ready-independent-tickets)`. **Cap
  concurrent Rust builds separately and lower** than total agents: a full `cargo build` + `clippy` ×2 + tests is
  CPU/RAM-hungry, and over-subscribing builds *thrashes* — total useful work drops. Stagger worker dispatch so
  their heavy build phases interleave rather than all hitting disk/CPU at once. Still hold back tickets that
  touch the same files/crate (they'd collide on merge anyway).
- **Batch the trivial.** Several tiny same-crate/same-file tickets → **one** worker doing them in sequence,
  not N worktrees each paying spin-up cost.
- **Cheapest capable model** per the right-sizing tiers above — audit that the tier actually matches the
  ticket, don't default everything to `sonnet`/`opus` out of habit.
- **Pipeline the gauntlet, don't serialise it.** Dispatch a PR's **Reviewer and UAT Tester in parallel**,
  and **start the next worker while a PR is in review** — review/UAT must never idle the build queue. Only
  the final merge serialises.

**At each idle checkpoint (fold into the `FOREMAN` block):**

- **Idle capacity?** Machine free but few workers running → pull the next ready ticket forward now.
- **Stuck / looping agent?** Escalate its model or re-scope the task rather than letting it burn loops
  (per the right-sizing rule) — don't wait for it to fail on its own.
- **Merge queue backing up?** PRs approved but unmerged → stop dispatching new workers and **drain with a
  batch-merge pass** (approved + green + disjoint files → merge in sequence, re-checking each rebases clean),
  so reviewed work actually ships (*not pushed = not done*).
- **Review queue backing up?** PRs waiting on Reviewer/UAT → spin up more reviewers in parallel before
  adding more builders.

Note any deliberate capacity call (e.g. "held CPE-YYY — same crate as in-flight CPE-XXX") in the work log.
Bias to **building over optimising**: the passes above are a few seconds of judgment, not an analysis
project. The ledger below gives them **teeth** — numbers to decide on instead of vibes — but recording a row
is a one-line append, not a second job.

### Teeth — the per-agent ledger (measure, then optimise)

The capacity/throughput calls above are only as good as the data behind them, so the Foreman **keeps a
ledger**. Substrate + full schema live in `.claude/workshift-metrics/` (`README.md`); the essentials:

- **Record a row when each sub-agent returns.** Append one JSON line to
  `.claude/workshift-metrics/ledger.jsonl` (gitignored, transient): role, ticket, ticket *class*, model,
  dispatched/returned timestamps, **measured `elapsed_s`**, outcome, `retries`, a `cost_proxy`, and a
  **`post_merge_defect`** field (see next bullet). Also keep the same rows as a live in-context table so the
  current shift can reason over them without re-reading the file.
- **Track correctness, not just speed — the escaped-defect signal.** Elapsed/retries/cost only measure *how
  fast/cheap*, never *how right*. So each Worker row also carries `post_merge_defect` — `null` at merge, then
  **back-annotated** to `ci-red` / `reverted` / `reopened` / `hotfix` if that merged work later breaks `main`'s
  CI, gets reverted, has its ticket reopened, or needs an immediate follow-up fix **this shift**. The Foreman
  updates the row when it observes the bounce (a red run on the merge commit, a revert, a reopen). This is the
  one signal that tells whether the 2-check gate is actually holding: a `(class, tier)` pair that ships fast but
  accrues `post_merge_defect`s is **going too cheap** — bump its default tier or tighten its review, even though
  its elapsed/cost look great. Escaped defects outrank throughput when the two disagree.
- **Measure what's real; don't fabricate the rest.** `elapsed_s` is measured straight from wall-clock `date`
  at dispatch and return — always real. **The `Agent` tool does not reliably return a sub-agent's token
  count, so never invent token numbers.** Cost is a **labelled proxy**: `tier_weight × elapsed_s`
  (haiku 1 · sonnet 4 · opus 15 · fable 4), useful only as *relative* spend; `retries` is the companion
  **waste signal** (rework paid for). Surface both as proxies, not as billing.
- **Let the numbers drive the two passes.** Concretely: a `(class, tier)` pair with high
  `retries`/`stuck-escalated` → that class's default model is too weak, **bump the tier**; `opus` elapsed ≈
  `sonnet` elapsed with the same outcome on a class → **downgrade** it; merge-queue wait > median build time →
  **reduce parallelism** and drain; review-queue wait rising → **add reviewers** before builders; **any
  `post_merge_defect` on a class → treat that as a stronger signal than its speed and bump the tier / tighten
  review before optimising it further.** These replace the earlier rules-of-thumb with a rule keyed to observed
  data.
- **Learn across shifts.** At the **end-of-shift wrap**, append a short distilled block to
  `.claude/workshift-metrics/history.md` (committed, shared CLI↔desktop): tickets shipped + the tuned
  defaults learned (e.g. `metadata-codec: sonnet, 2-wide, ~11m median, 0 stuck`). At **kickoff**, read the
  tail of `history.md` to **seed** this shift's model/parallelism defaults instead of relearning cold.
- **Report it.** Add a compact `• Metrics —` line to the `FOREMAN` block (merged count · median gauntlet ·
  retries · **escaped defects** · ~cost-proxy), and print the **full ledger table** in the end-of-shift wrap so
  the user sees where the time and (proxy) cost went — and whether anything bounced after merge.

This is still lightweight — a one-line append per agent and one distilled block per shift — but it means every
concurrency and model call is backed by measured throughput, not guesswork.

### The sub-agent budget — bounded batches + checkpoint-and-reset (never hit the wall)

Sub-agent spawns are capped **per session** (`CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION`, default **200**). Each
ticket burns ~3–4 agents (Worker + independent Reviewer + independent UAT, ± a Researcher/Planner), so a single
session tops out around **~40–50 tickets**. If the shift spawns blindly to 200 it **stalls mid-task** — the crew
goes dark with in-flight work and no way to finish it (this happened: 200 agents → dead crew mid-epic). **Do not
run into the wall. Reset the budget *before* it, often.**

- **The ledger IS the live counter.** `ledger.jsonl` already records one row per agent-run, so the Foreman
  always knows the running count — no separate bookkeeping. Track it against the cap.
- **Reserve a drain margin; reset at a threshold, not at the cap.** Treat **~75% of the cap (~150/200)** as the
  **reset line**, leaving ~50 agents of headroom to *finish what's in flight*. As the count approaches the line,
  **stop dispatching new tickets**, let the open gauntlets (Reviewer+UAT) complete, merge the drained PRs, prune
  worktrees — quiesce to a clean, all-green, nothing-in-flight state.
- **Checkpoint, then hand off for a session reset.** The per-session cap only refreshes in a **new session** (the
  Foreman cannot self-restart one). So at the reset line, after quiescing: append a **resumable checkpoint** to
  `.claude/workshift-metrics/CHECKPOINT.md` (committed) — *what merged this batch, what's next in priority order,
  active epic/slice + its plan/Library entry, any decide-and-log assumptions, tuned defaults* — then tell the
  user plainly: batch done, budget nearly spent, **start a fresh session and say "resume the workshift"** (or
  raise `CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION` to continue now). A fresh session reads `CHECKPOINT.md` +
  `history.md` and continues seamlessly with a full budget. This is the "reset often" loop: **work a bounded
  batch → quiesce → checkpoint → reset → resume**, indefinitely, without ever stalling with lost in-flight work.
- **Stretch the budget between resets (spend agents where they earn their keep).** Fewer agents per ticket = more
  tickets per session: **batch several trivial same-file tickets into one Worker**; **Foreman-apply a tiny,
  exactly-prescribed reviewer fix directly** (re-verify + a focused re-review) instead of a full worker
  round-trip; **reuse a Library hit** to skip a Researcher; **de-risk the hard slice once** with a single Plan
  agent rather than several flailing Workers. Keep the **≥2-independent-checks gate** (Reviewer AND UAT) — that's
  non-negotiable — but don't pile on extra refuters/researchers unless the ticket is genuinely high-risk.
- **Surface it.** Add a `• Budget —` line to the `FOREMAN` block: `agents ~N/200 · ~M tickets to reset line`. So
  a reset is a *planned, clean* checkpoint, never a surprise mid-merge.

## The research Library — file it once, reuse it forever

The **Librarian** keeps `.claude/research-library/` (committed, shared CLI↔desktop). It makes research a
**compounding asset** instead of a per-shift throwaway:

- **Before dispatching a Researcher, check the Library.** The Foreman/PM asks the Librarian "do we
  already know this?" — the Librarian scans `INDEX.md` then the matching entry. On a **hit**, reuse the
  filed research and **skip the Researcher dispatch** (log a `librarian` / `library-hit` ledger row —
  that's the Library's measurable ROI).
- **When a Researcher returns, file it.** The Librarian normalises the tradeoff-labelled options into an
  `entries/<slug>.md` (schema in the Library's `README.md`) and appends its `INDEX.md` line. Dedup like
  the memory system — **update** an existing entry rather than duplicate; mark an overturned finding
  `status: superseded` and point to the newer slug.
- **The PM's reference desk.** When the Product Manager weighs which epics to build, it pulls the
  relevant prior research **through the Librarian** rather than re-commissioning it.
- **The Librarian can research the Library itself** — cross-referencing entries to answer a question
  purely from what's already filed, and curating the index (tight findings, generous tags) so retrieval
  stays fast as the corpus grows.

At **kickoff** the Foreman already reads the tail of `history.md`; also glance at the Library `INDEX.md`
so the shift starts knowing what's on file.

## QA Architect — automate testing until manual testing is gone

The user's standing goal: **never test anything by hand.** The **QA Architect** exists to make that true
over time by driving **Manual Verification Debt (MVD)** — the count of app surfaces still needing human
eyes — to zero. Substrate: `.claude/qa-architecture/` (charter + the `MANUAL-TEST-BURNDOWN.md` ledger,
both committed).

- **Every UAT skip-and-note is fuel.** Whenever a ticket's UAT has to be *skipped* for a user resource
  (escalation #2 — interactive cross-OS GUI verification, a Mac, credentials), that skip **becomes a
  burndown row** the same shift. The QA Architect's whole purpose is to erode escalation #2 until it
  essentially never fires.
- **It architects; Workers build.** Each shift the QA Architect audits the burndown + what shipped, picks
  the **highest-leverage** manual surface (headless GUI driving is usually the top prize — it unblocks
  visual-regression and cross-OS too), and **files a `CPE-NNN` ticket** with the automation design. A
  Worker implements it through the normal gauntlet.
- **Ratchet, don't backslide.** When automation lands green, flip the burndown row to ✅, name the CI/guard
  job that **pins** it, and decrement MVD. An automated surface must never quietly return to manual.
- **Report the number.** MVD and its delta this shift go in the wrap, so the user watches manual testing
  disappear.

Model tier: **opus** for the QA-Architect audit/strategy (test design where a wrong call is costly);
Workers implement the harnesses on their right-sized tier.

## Reporting — ASCII banners + timestamps + FOREMAN blocks

- **Every message that directly addresses the user leads with an ASCII-art banner** (the user is often across
  the room and can't read prose) — see [[use-ascii-art-when-addressing-user]]. Keep the banner words short +
  high-contrast (`BUILDING…`, `RUNNING ✓`, `NEEDS YOU`, `DONE`).
- **Timestamp every on-screen message in system LOCAL time** (e.g. `date "+%Y-%m-%d %H:%M:%S %Z"`); stamp the
  **start and finish** of anything slow and show the **elapsed** (`CPE-983 done 17:22:41 (⏱ 7m32s)`). Per
  [[loop-behavior-needs-timestamps]].
- **Each idle poll-wait and the end-of-shift wrap** use a bordered **`FOREMAN`** block with a timestamp
  header and an "Awaiting you" footer, each line item split by a `────` rule:

  ```
  ═════════ FOREMAN · 2026-07-24 14:32 USMST ═════════
  • Shift — <assignment / current focus>
  ────────────────────────────────────────────────────
  • Done — CPE-XXX <title> (PR #NN merged)
  ────────────────────────────────────────────────────
  • In flight — worker A: CPE-YYY <status>; worker B: CPE-ZZZ <status>
  ────────────────────────────────────────────────────
  • Parked — CPE-WWW (3 fails: <one-line why>) → Deferred/  [or "none"]
  ────────────────────────────────────────────────────
  • Janitor — <last clean / "break needed for deep clean" / "clean">
  ────────────────────────────────────────────────────
  • Metrics — <N merged · median gauntlet Xm · Y retries · E escaped-defects · ~cost Zu (proxy)>
  ────────────────────────────────────────────────────
  • Budget — <agents ~N/200 · ~M tickets to reset line>
  ────────────────────────────────────────────────────
  • QA — <MVD: N manual surfaces (Δ this shift) · automating: CPE-XXX>
  ────────────────────────────────────────────────────
  • Next — <next action>
  ────────────────────────────────────────────────────
  • Next wake — <local time of the armed fallback wakeup, or "on next agent return">
  ────────────────────────────────────────────────────
  • Awaiting you — <user-resource blockers, or "nothing">
  ════════════════════════════════════════════════════
  ```

  The header stamp doubles as the `• Tick —` timestamp; `• Next wake —` records the armed `ScheduleWakeup` so
  the loop's cadence is always visible (per the heartbeat rules).

- **Return-facing wraps and any user question are the rich, plain-language version** — expand, don't
  compress; the user has been away and won't remember IDs/jargon (see
  [[workshift-summarize-with-context]]). Lead with what a thing *is* in plain English, put `CPE-NNN` in
  parentheses. `AskUserQuestion` options must be self-explanatory to a cold reader.
- **Sign every workshift PR** with `— Foreman · workshift supervisor · <YYYY-MM-DD>` as the **last line** of
  the PR body, below this repo's required trailer (keep the "🤖 Generated with Claude Code" + session link).
  Sign the PR, not each commit (commits keep the standard `Co-Authored-By` + `Claude-Session` trailers).

## GUI verification = build → deploy → run (never a dev server)

Any time the user must **look at the GUI**, do the full **build → deploy (install the sidecar / AI-Console
build) → run** cycle yourself — never "go run `tauri dev`". Publishing/installing for GUI testing IS
authorised during a workshift. Build (`Release (sidecar-enabled)` workflow — plain `release.yml` is the wrong
one), kill every `cpe`/`ai-console` process (incl. `--session-daemon`) **before** installing or NSIS skips
the file-locked sidecar, verify the installed version + sidecar timestamp, then launch + confirm it's
responding. Bracket it with the ASCII **WAIT → ① BUILD → ② DEPLOY → ③ RUN → RUNNING → checklist** narration.
See [[gui-verify-needs-build-deploy-run]], [[always-install-sidecar-build]], [[install-kill-all-processes-first]].

## Machine-sharing (do NOT auto-yield)

The user is physically away, so the machine is free. If recent human input appears (idle time drops — they
came home or are remoting in), do **not** automatically pause. Instead **tell the user I see they're here and
ASK whether I should yield the machine**, then act on the answer. This is the one sanctioned exception to
"never stop to ask" — a presence check, not a work checkpoint.

## Shift end — release the loop's resources

Whenever the loop actually ends — the end-of-shift wrap, the checkpoint hand-off for a budget reset, a hard
stop, or the user telling me to stop — **tear down the loop's live state** so nothing dangles:

1. **Cancel the armed fallback wakeup** (`ScheduleWakeup` with `stop: true`) so no stray tick fires after the
   shift is over.
2. **Release the shift lock** — delete `.claude/workshift-metrics/WORKSHIFT-LOCK` so the next Foreman (CLI or
   desktop) can claim it. On a checkpoint hand-off, the resuming fresh session re-acquires it.
3. **Run the light Janitor pass one last time** (prune finished worktrees, delete merged branches) so the tree
   is left clean.

Only a genuine end releases these — an idle tick with work still in flight re-arms the wakeup and refreshes the
lock heartbeat instead.

## Janitor — keep the workspace clean (and call a deep-clean break)

A long shift leaves debris: worktrees from finished workers, merged branches, UAT scratch dirs, temp
files, and a `Ticketing/Tickets/Done/` that keeps growing. Left alone it slows every worker (stale worktrees confuse
git ops — see [[verify-subagent-merges]]) and buries the queue. The **Janitor** runs a light pass **between
merges** and a **deep pass on a called break**.

**Light pass (safe, no break needed — do it opportunistically after each merge/push):**

- Prune git worktrees whose worker is **done and merged** (`git worktree prune` + remove the specific
  finished worktree); delete its now-merged branch.
- Delete **fully-merged** local branches (never an unmerged or in-flight one).
- Remove **this shift's own** finished UAT scratch (`.claude/uat-*`) and scratchpad temp files — only ones
  the shift created and no longer needs.
- Clear obvious throwaways (build logs, `*.tmp`) from the scratchpad, never from the working tree.

**Deep pass (needs a Foreman-called break — anything that could collide with a live worker):**

- `git worktree prune` across the board + `git gc`, branch sweep, `Ticketing/Tickets/Done/` reorganisation via
  `/ticketing-organize` (the SessionStart hook warns when `Done/…/Week-NN` overflows — that warning is the
  Janitor's cue).

**The deep-clean break protocol:**

1. Janitor signals the Foreman it wants a deep clean (queue getting messy, or the `Done/` overflow warning fired).
2. **Foreman calls the break:** stop dispatching new workers, let in-flight PRs finish merging/pushing, wait
   for the worktrees to go idle. **Do not kill a mid-flight worker** — drain, don't yank.
3. Janitor runs the deep pass on the now-quiet tree, then signals **all-clear**.
4. Foreman resumes dispatch.

A break is a *drain-and-resume*, **not** a hard stop — it never ends the shift, never asks the user, and is
logged (`Janitor: deep clean — pruned N worktrees, organised Done/, ⏱…`) like any other work. Keep it rare
and short; the light pass should keep things tidy most of the time.

**Non-destructive default + guardrails:** when in doubt, leave it. Never delete another concurrent process's
worktree, branch, or untracked dir ([[concurrent-nightshift-coordination]]); never remove anything under the
working tree that isn't a known throwaway; never touch `.git` internals beyond `prune`/`gc`. Cleanup that
would itself be a code change (e.g. deleting committed files) goes through a `CPE-NNN` ticket like anything
else.

## Honesty over completion + guardrails

Flag any ticket that needs the user's resources or interactive cross-OS verification rather than faking
"done." Keep a running work log so at return the user sees exactly what was done, assumed, completed,
published/installed, and blocked. Hold all `CLAUDE.md` guardrails (three version files in sync on release,
never commit keys, keep the green pipeline, preserve `list_dir` skip-on-error). Coordinate with any
concurrent process sharing the repo (don't clobber another agent's worktree / collide on IDs) — verify a
merge actually landed rather than trusting a push echo.

---

*Cross-cutting habits referenced above live as memories (they apply outside the workshift too):*
`[[use-ascii-art-when-addressing-user]]`, `[[gui-verify-needs-build-deploy-run]]`,
`[[workshift-summarize-with-context]]`, `[[loop-behavior-needs-timestamps]]`, `[[go-with-recommendation]]`,
`[[code-changes-via-ticket]]`.
