# Sprint — Autonomous Supervised Work Loop

> **Naming note (renamed from "workshift" 2026-08-08):** this **`/sprint`** skill is the *autonomous
> supervised work loop* (formerly "workshift"). It is DISTINCT from **`/ticketing-sprint`** + the **SPR-NN**
> "Sprints" (`Ticketing/Sprints/`), which are *time-boxed batches of tickets* managed separately. Both names
> coexist by design: `/sprint` = the work loop; `/ticketing-sprint` = the ticket-batch manager.

An autonomous "work while you're away" mode. Triggered when the user says **"start the sprint"** /
**"sprint"** / **"this is the sprint"** (the older **"dayshift"** phrasing is a kept alias). End it on
**"stop the sprint"**; re-baseline mid-run on **"restart the sprint"**.

**This is a lights-out factory.** A sprint — or a batch of sprints — runs with **zero expectation that the
user is present or reachable.** Assume the user **cannot and will not answer anything** for the entire run.
**No question ever gates progress.** Every ambiguity, design choice, disposition, sequencing/naming default,
or gate-vs-fix judgment: make the best reasonable call, log the assumption in the ticket work log, and keep
moving. `AskUserQuestion` is **banned** for the duration of a sprint. Things that genuinely need the *user's*
own resources/authority are **skipped-and-noted into an async review queue** (escalation #2 below) — never
asked-and-awaited. The **only** thing that halts the whole factory is a safety hard-stop (escalation #3). Keep
working until the safe work is **done**; the user's return is **not** a condition the loop waits for. **Never
idle.** The
assignment is whatever "this" refers to when the shift starts; if nothing specific, work the critical path
(finish `Ticketing/Tickets/Doing/` → clear `Ticketing/Tickets/Backlog/` + pickable `Deferred/` → activate an epic → have the
**Product Manager** task Researchers to find + pitch new epics, pick the highest-impact ones, then build them).

**DO NOT STOP FOR APPROVAL OR STATUS UPDATES.** Never end a turn asking permission to continue; never pause
for an interim status; a "natural milestone" is not a reason to stop. Report only at the very end (the safe
headless work runs out). If one ticket needs a user resource, **skip it and keep working others** — don't halt
the whole shift.

---

## The operating loop (read this first — everything below is reference)

This is the runnable spine of the shift. Run it until the safe work is done (or a safety hard-stop); the user's
return does **not** end it. Each step links to its detailed section below.

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
- **Only two things end the loop:** the safe work runs out (wrap), or a hard-stop safety condition fires.
  Nothing else — not a milestone, not a full merge queue, and **not the user returning**. A detected presence
  triggers a *non-blocking* machine-sharing offer (§ "Machine-sharing"), never a loop end or a wait.

---

## Pre-flight — acquire the lock and verify the base before any work

Run this **once at kickoff, before the roll-call.** It is cheap insurance against a whole shift stacked on a
broken base or colliding with another shift.

1. **Acquire the shift lock.** This repo is driven from **both** the CLI and the desktop Cowork app on the same
   files ([[concurrent-nightshift-coordination]]), so two Foremen can fight over `main` and collide on ticket
   IDs. Before starting, check for `.claude/sprint-metrics/SPRINT-LOCK` (gitignored). If it exists and its
   heartbeat timestamp is **fresh (< ~30 min old)**, another shift owns the merge lock — **do not start a second
   merging shift**; tell the user and stop. If it's absent or **stale**, claim it: write the lock with this
   session's id + a wall-clock timestamp, and **refresh that timestamp on every tick**. Release it (delete the
   file) at the wrap, the checkpoint hand-off, or a hard stop.
2. **Verify the base is sane.** Confirm the working tree is **clean**, `main`'s latest CI run is **green**
   (`gh run list`), and a baseline `cargo build` succeeds. If the base is broken, the first unit of work is to
   **fix the base** (or, if that needs the user, skip-and-note and start from the last green commit) — never
   build a shift's worth of PRs on red.
3. **Verify the substrate.** Confirm the committed substrate dirs exist and are readable —
   `.claude/sprint-metrics/` (`ledger.jsonl`, `history.md`, `CHECKPOINT.md`), `.claude/research-library/`
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
| **Visual Critic** | per-ticket (GUI changes only) | An **independent** sub-agent with **good design taste** that *looks at* the work — it reads the **screenshots** captured by the `gui-smoke` harness (CPE-1148) of the real built app and judges the **visual result** against the design standards ([docs/design/MENUS.md](../../docs/design/MENUS.md), [TABS.md](../../docs/design/TABS.md), the pill/tick-tack reflow rules, the light-theme palette, alignment/spacing) **and** plain "does this look and feel right". It is the gauntlet's **visual leg** — Reviewer checks the *code*, UAT checks the *behaviour*, the Critic checks the *look/feel*. Returns **`VISUAL PASS`** or **`VISUAL CHANGES`** with concrete, screenshot-grounded defects (clipped / misaligned / misplaced / wrong-or-ambiguous glyph / off-theme / cramped). Its whole purpose is to **catch the visual defects that used to bounce to the user** (placement, clipping, icon legibility) so the user is asked **minimally** — only for a genuinely subjective taste/preference call (and then via a concrete pick-list), or for something a screenshot can't show (interaction feel / animation cadence). See [[visual-critic-and-screenshots]]. |
| **Janitor** | per-shift (light between-merges) | Keeps the workspace clean so the crew stays fast. Between merges it reclaims **abandoned resources** and tidies up (see the Janitor duties section below) — leftover git worktrees from finished workers, merged/stale branches, orphaned `.claude/uat-*` and scratchpad temp dirs, and an overstuffed `Ticketing/Tickets/Done/` (runs `/ticketing-organize`). It works **non-destructively by default** and never touches another live process's resources (worktrees/branches/untracked dirs in use — see [[concurrent-nightshift-coordination]]). For a **deep clean** that would collide with active workers (pruning worktrees, `git gc`, reorganising `Done/`), the Janitor asks the **Foreman to call a break** — quiesce dispatch, let in-flight PRs settle — then cleans on the quiet tree and signals all-clear. |
| **Security Auditor** | per-ticket (risky diffs) / per-shift sweep | An **independent** sub-agent that scrutinises a change for **security** issues the code Reviewer isn't specifically hunting: path-traversal / symlink-escape in the filesystem commands, over-broad Tauri **capability** grants (`src-tauri/capabilities/default.json`), sidecar / IPC **trust-boundary** violations, unsafe deserialisation, secret or key leakage, and updater/signing integrity. **Owns running the repo's `/security-review` skill** and gates merge on it for any diff touching filesystem, IPC/sidecar, capabilities, the updater, or `unsafe`. Returns **`SEC PASS`** / **`SEC FINDINGS`** with concrete, exploitable specifics (not vibes). Purely-cosmetic or docs-only diffs skip it. Distinct from the Reviewer (general code QA) — this leg only asks "can this be abused?". |
| **Performance Guard** | per-shift | Owns PURPOSE.md's **fast / small / predictable** tiebreaker as a *measured* discipline, not a vibe. Tracks the numbers the crew would otherwise let drift — **binary/installer size**, cold-start time, directory-listing + streaming latency, and memory — captures a baseline at kickoff, and flags any change that regresses them, **filing a `CPE-NNN` ticket** when a diff costs speed or bloat (outside Agent Watch, where the visibility precedence overrides). Reports the size/latency deltas in the wrap so regressions surface the shift they land, not a release later. Model tier: **sonnet** (bump to **opus** for a gnarly perf investigation). |
| **Release Engineer** | per-shift / on-demand | Owns the mechanics of **shipping** so the most guardrail-sensitive step stays reliable. Enforces the **three-files-in-sync** version bump (`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`), maintains the changelog, drives tag/push, and watches **CI release health** (the `Release (sidecar-enabled)` workflow, not plain `release.yml`) — verifying a draft actually carries signed installer assets before anything is published. Follows `RELEASING.md`. Never publishes an empty/asset-less draft; never touches signing keys. Invoked when the shift reaches a releasable milestone or the user says "cut a release"; otherwise dormant. |
| **Docs Scribe** | per-ticket (user-facing changes) | Ensures a user-facing change **ships with its docs** instead of failing CI later. Per **CPE-579**, every feature that adds a user-facing *section* must (a) ship/update its `src/docs/*.md` page and (b) add its `section → slug` entry in `src/lib/sectionDocs.ts` (the one source of truth, guarded by `sectionDocs.test.ts`). The Scribe writes/updates that page + registry entry **as part of the ticket**, and keeps the broader in-app Documents library current ([[maintain-in-app-docs-library]]). Headless/backend tickets with no user-facing surface skip it. Distinct from the Librarian (internal research corpus) — the Scribe owns the **shipped, user-facing** docs. |
| **Dependency Steward** | per-shift | Guards the **lean-core / no-new-deps** guardrail and the supply chain. **ENUMERATE FIRST, then audit** — `node scripts/audit-npm-projects.mjs` for npm (it runs `git ls-files '*package-lock.json'` and sweeps every project it finds: the root **and `gui-smoke/`**), and `cargo audit` per `Cargo.lock` the same way. Do **not** run a bare `npm audit` in whatever directory you are standing in and report the number as the repo's — that is exactly how `gui-smoke/` went unaudited through every pass to 2026-08-27 while carrying 17 advisories, including the same `brace-expansion` high the root had just fixed (CPE-1945; CPE-1932 is the same defect in `Cargo.lock` files). Any advisory count you report **names the project(s) it covers**. `npm audit fix` is run **without `--force`**, always — see CLAUDE.md "Guards and ratchets" for why npm's "fix" is often a downgrade. Watches for outdated or risky dependencies and advisories, and **challenges any new dependency** a Worker adds — is it justified, or can the lean core absorb it? Files a `CPE-NNN` ticket for a genuinely-needed upgrade or a flagged CVE. Complements the desktop `cpe-weekly-deps` scan by making it a live part of the shift rather than a weekly afterthought. Model tier: **haiku/sonnet** (mechanical scan + judgement call). |
| **Accessibility Auditor** | per-ticket (GUI changes) | The Visual Critic's a11y sibling — where the Critic judges *taste*, this judges **usability for everyone**. For a GUI change it checks keyboard navigation + focus order, contrast ratios against the light-theme palette, screen-reader labels / ARIA, and target sizes, reading the same `gui-smoke` output where it can. Returns **`A11Y PASS`** / **`A11Y FINDINGS`** with concrete, reproducible defects. Headless/backend tickets skip it. |
| **Integration Tester** | per-shift | Exercises **cross-feature workflows** that each pass in isolation but can break in combination — open folder → search → batch-rename → Agent Watch, tab churn, streaming under load, mode switch on/off. The UAT Tester validates *one* ticket's feature from the outside; the Integration Tester validates that the features still compose. Runs an end-to-end pass across the shift's merged work and **files a `CPE-NNN` ticket** for any interaction bug no single-ticket check would catch. Model tier: **sonnet**. |

Spawning sub-agents is **pre-authorised** during a sprint (this overrides the default "don't spawn agents
unless asked"). Give each agent enough context (the ticket + acceptance criteria + relevant crates/APIs +
conventions + the delete-test rule) so it doesn't re-derive from cold.

### Dispatch contract — every Worker/Reviewer/UAT prompt states this, verbatim (CPE-1848, corrected by CPE-1880)

A dispatched sub-agent — Worker, Reviewer, UAT Tester, or any other role — **never receives a background
task notification**. That wake-up is a capability of the **Foreman's own harness loop** (the heartbeat
above: "Background agents re-invoke you" describes what happens to the Foreman when ITS sub-agents
finish, not something a sub-agent can itself rely on). A sub-agent left holding a background task has no
way to be woken from it, so it does the only thing that follows: it waits, forever, and returns nothing
usable.

**Why the rule alone was not enough, and what actually causes this (CPE-1880 — read this before editing
the paragraph below).** CPE-1848 stated the rule and handed agents the blocking command to use instead:
`gh run watch <run-id> --interval 30`. **Five more agents stalled the same day, three of them after being
sent that exact command in a message that named the defect and named the ticket.** They were not being
defiant and they had not forgotten — they *complied*, and complying is what stalled them:

- The harness's Bash tool caps a single call at `timeout: 600000` ms. A command that outlives the cap is
  **auto-backgrounded, not killed** — so the agent ends up holding precisely the background task the rule
  told it to avoid, through no decision of its own.
- `gh run watch` blocks until the run finishes. Measured over the 95 completed `ci.yml` runs from
  2026-08-23 to 2026-08-26: **median 58.9 min, p90 77.3 min, max 97.0 min. Of the 71 runs that
  succeeded, ZERO finished inside 600 s** — the fastest took 28.6 min, and the only sub-ten-minute runs
  in the window were four cancellations. The prescribed command had a **0-of-71** chance of returning.
- The obvious mitigation does **not** work: a shell-level `timeout 570 gh run watch …` wrapper was
  observed backgrounded anyway, because the harness timer spans the whole compound command rather than
  the wrapped process. Do not reach for it; it is the fix everyone tries and it did not hold.

So the lever is not stronger wording — it is removing the unbounded call and moving the CI wait to the
only participant that genuinely gets notified. **The Foreman owns CI.** Workers push and report; they are
never asked to establish a CI outcome. This was validated live: the moment a stalled worker was told
*"I own CI, do not watch it, hand me the report you already have,"* it returned a complete, high-quality
report immediately. It never lacked the material; it lacked a way to stop waiting.

**Every** Worker/Reviewer/UAT dispatch prompt includes, verbatim or in substance:

> You receive NO background task notifications, and any command that runs past the harness's 600 s tool
> cap is auto-backgrounded on your behalf — so a long-running call parks you whether or not you intended
> it. Run everything synchronously and in the foreground, and keep every single call bounded well under
> 600 s: builds, tests, `gh` calls, all of it.
>
> **The Foreman owns CI. Do not watch, poll, or monitor it.** Push your branch, open the PR, and report
> — including `CI still pending on <SHA>` so the Foreman can take it over. Your report is complete
> without a CI verdict; do not hold it back waiting for one.
>
> **Never run `gh run watch` or `gh pr checks --watch`.** Both block until CI finishes, and CI on this
> repo has never once finished inside the 600 s cap (median ~59 min) — they are backgrounded 100% of the
> time. If you genuinely must read CI for some other reason, the one sanctioned idiom is
> `node scripts/ci-poll.mjs --run <run-id>` (or `--pr <number>`): it is clamped below the cap by
> construction, prints one timestamped line per tick, and always ends with a single `CI VERDICT:` line
> carrying `total_count`, `pending`, `mergeable`, and the SHA. Re-invoke it if it returns
> `CI VERDICT: pending`. Never wrap `gh run watch` in `timeout`; that was measured and it still
> backgrounds.
>
> **Never** return a stub that promises to report later, and never say a monitor is "armed" or "watching
> in the background" — that phrasing is the exact defect this rule exists to prevent, and the Foreman
> runs `node scripts/stall-check.mjs` over your report on arrival, so it is caught rather than believed.
> If you need to QUOTE that phrasing (reporting on someone else's stall, or on this rule), put it in a
> **code fence**. Fenced blocks are stripped before matching; `>` blockquotes are **not** — quoting by
> blockquote used to hide every recorded stall, so that exemption was removed.

This is a standing instruction, not a per-dispatch judgment call: include it in every Worker/Reviewer/UAT
briefing regardless of how routine the ticket looks — the failure mode above hit ordinary tickets, not
exotic ones. It applies unchanged inside a batched run (`/sprint-batched`): a stalled sub-agent there
doesn't just cost a round-trip, it stalls the batch counter along with it.

**The Foreman's own side of the bargain (CPE-1880).** Because the contract now forbids workers from
establishing CI outcomes, the Foreman must actually pick them up:

- After a worker reports, the Foreman polls that PR itself — **`node scripts/ci-poll.mjs --pr <n>
  --budget 45`** — and routes failures back to the worker as a concrete fix request, not as "go check
  CI." **Use a short budget and cycle**; do not take the 480 s default here. The default is sized for a
  worker that has nothing else to do, whereas the Foreman polling seven branches at 480 s each would sit
  blocked for ~56 minutes per sweep, unable to dispatch — which trades a stalled worker for a stalled
  supervisor. Sweep the open PRs at 45 s apiece, dispatch in between, and come back round. Re-check the
  current head by **SHA**, not PR number alone: a stale PR-number check can pass against a superseded
  head, and `--watch` exits 0 when the branch moves under it rather than only when checks pass.
- **`git fetch origin main` before the poll you intend to merge on (CPE-1970).** The staleness verdict
  below compares the PR's board against the job set on **`origin/main` as your clone last saw it**. The
  poll deliberately does not fetch — a merge gate must not have side effects or one more thing that can
  hang — so a clone that has not fetched since the guard landed reports `coverage=ok` on exactly the
  board it exists to refuse. One `git fetch origin main` at the top of each sweep is the whole cost.
- **Read the exit code, not just the line (CPE-1906, CPE-1970).** `ci-poll.mjs` has six outcomes and
  only one of them means merge:
  `0` green · `1` a check FAILED · `2` still pending — the normal outcome, re-invoke or come back round ·
  **`3` COULD NOT ASK** — `gh` errored, hung, returned garbage, **or answered 200 with JSON that is not
  a board** (a REST `{"message":"Not Found"}`, a GraphQL partial with a null `statusCheckRollup`).
  Nothing was read. This is neither pending nor green: do **not** merge and do **not** wait. Check
  `gh auth status`, the PR number and the network, then re-invoke. ·
  **`4` a check DID NOT RUN** — one or more checks came back `SKIPPED` with no job-level `if:` to explain
  it, i.e. a `needs:` cascade off an earlier failure. `ci.yml`'s five Rust test jobs sit behind
  `needs: lockfile-preflight`, so a preflight failure skips the entire Rust suite; before CPE-1906 that
  reported as `completed success`. Exit 4 also covers a board where **nothing ran at all** (every
  finished check was a by-design skip) and one that finished in a shape the poll cannot call. Not red,
  not green — do not merge; find out why. ·
  **`5` THE CHECKS ARE STALE** — nothing on the board is red, and that is the problem: a job `main`
  already requires produced **no check at all** on this PR, so a guard that exists on `main` never
  judged it. `main` has no branch protection (`branches/main/protection` → 404, `rulesets` → `[]`), so
  nothing else stops this. Measured over the 186 PRs merged 2026-08-14 → 2026-08-28: **15 merged this
  way** — `ratchet-guard` ×5 (including #1056, the merge that found it), `ci-verdict` ×5,
  `lockfile-preflight` ×2, `msrv` ×2, `ffmpeg-pin-guard` ×1. (A sixteenth board, #921, also tripped the
  rule, but that PR had renamed the job itself — the one measured false positive, 0.54% of merges.)
  The fix is a rebase
  onto `main` and a re-run, not an `--admin` merge. Exit 5 also covers `completed coverage-unknown` —
  the poll could not read `main`'s workflows, which is "did not run", not "nothing to check".
  **The prefix and the code agree, one-to-one** — `completed success`→0, `completed failure`→1,
  `pending`→2, `unknown`→3, `completed did-not-run` / `completed unclear`→4,
  `completed stale-checks` / `completed coverage-unknown`→5. Grep either; they cannot
  disagree, which they used to: a board of nothing but by-design skips printed `completed skipped` and
  exited **1** ("a check FAILED") with zero failures, and `completed skipped` was simultaneously the
  exit-4 prefix.
  Every verdict line — including the green ones — now carries **`coverage=`**: `ok`, `ok(N-silent)`,
  `N-unjudged`, `unknown`, or `n/a(<reason>)`. It is printed even where the check did not apply, because a coverage
  check that goes quiet is indistinguishable from one that ran and found nothing. What it does **not**
  see: a guard added *inside* an existing job (a new `.test.ts` under the same `Frontend` check, a new
  ratchet under the same `Ratchet guard` check). Only branch protection's *require branches to be up to
  date* closes that — see [docs/design/CI-STALENESS.md](../../docs/design/CI-STALENESS.md).
  A `pending` line also carries **`gh_failures=N`** — reads that failed without reaching the bail
  threshold. `pending` with a non-zero count there means the board is stale as well as unfinished.
  The pending line also now carries **`oldest_pending_min`** and the name of the longest-running
  unfinished check. That is the number to compare against the same job on a sibling PR when deciding
  whether a job is slow or hung — a judgement this crew previously made by hand-reading timestamps, once
  for over an hour with two approved PRs blocked behind it.
- **Run the arrival check on every returned sub-agent report:** `node scripts/stall-check.mjs
  report.txt --prior <n>`, where `<n>` is how many stall-shaped reports that same agent has already
  returned. It exits `0` accept, `3` re-invoke, `4` take-over. This is mechanical on purpose — "a monitor
  is armed" reads as progress, which is why a Foreman that trusts its own eye waits on a dead agent.
- **The escalation is bounded at one retry, and that bound is the point.** First stall-shaped return →
  re-invoke the same agent once (`SendMessage`) with "I own CI; report now, synchronously, with what you
  have." Second one from the same agent → **kill it and take over its PR yourself.** Do not re-invoke a
  third time: run `batched-2026-08-23-1124` recorded one agent producing four "still waiting" returns in
  a row, each stale wake generating the next, and it could not exit on its own. An agent that has armed a
  monitor cannot be talked out of it.

### Shared machine state — a tool install is a shared-resource change, not local setup (CPE-1856)

Worktrees isolate the **filesystem**. They do not isolate the **machine**. Every concurrent agent shares
one PATH, one user profile, one tool store, one `%TEMP%`, one global git config, one cargo registry
cache. Measured on 2026-08-21: a worker on CPE-1842 `dotnet tool install --tool-path`'d PowerShell 7 into
`~/.dotnet/tools`, took its measurement, and correctly **uninstalled it** on the way out. A sibling worker
on CPE-1841 had been running its suite against that same shim — green at 22:05/22:08/22:10/22:12, **red
at 22:14:04**, exactly inside the removal window (`~/.dotnet/tools` mtime 22:14:12). Every run from
22:15:30 onward was green again, **silently on Windows PowerShell 5.1** instead of PowerShell 7, because
the harness's host probe fell back without saying so. Two hours were then lost blaming Defender and a
vitest timeout before the real cause — a concurrent uninstall — was reconstructed from directory mtimes.
Full incident and timeline: ticket CPE-1856.

So, every Worker/Reviewer/UAT dispatch prompt also includes, verbatim or in substance:

> Installing, uninstalling, or upgrading anything **machine-global** — `dotnet tool`, `winget`, `choco`,
> `npm -g`, `cargo install`, a PATH edit, a global git config value, a shared port, the cargo registry
> cache, anything written under `%TEMP%` outside your own worktree — affects **every other agent running
> on this machine right now**, not just you. Treat it as a shared-resource change: if you need a tool
> another agent might also be using, prefer a fixture fully inside your own worktree over installing it
> machine-wide, and if you must install one machine-wide, **leave it installed and say so in your Work
> Log** rather than uninstalling it on your way out — removal, not install, is the harmful half, because
> it can pull the floor out from under a sibling agent's run that is currently green on it.

And, orthogonally: **any measurement or benchmark claim records which host/tool version produced it and
how that was determined** (a printed probe result, a version string in the log — not "whatever was on
PATH"), so a later reader can tell "measured on X" from "measured on whatever happened to resolve." A
provenance note per claim is the shape; see CPE-1841's round-2 Work Log for a worked example of the
correction this prevents.

**Harness tool probes must not fall through silently.** Where a script or test suite probes for an
external tool (e.g. `findPowerShellHost()` in `src/lib/releaseVersionBump.test.ts`), resolve it **once**,
pin it for the whole run, and **announce the resolved host and version in the run's own output** — never
resolve-and-fall-back-per-call without saying which implementation won. A pinned host that then vanishes
mid-run must fail loudly (the spawn errors, the run reds) rather than the harness quietly retrying a
different implementation; that is a property of spawning the literal pinned name once, not of adding a
retry loop.

**Sweep, not just the tool-install case** — other machine-global state agents on this crew touch or could
touch: environment variables set for the session, global git config (`git config --global`), listening
ports a dev server or test harness binds, the cargo registry/build cache under `~/.cargo`, and anything
written to `%TEMP%` (`$env:TEMP`) rather than a path inside the agent's own worktree. None of these are
isolated by a worktree either; treat a change to any of them the same way as a tool install above.

## Shift kickoff — the Foreman introduces the crew, then starts

Before announcing, the Foreman **reads the tail of `.claude/sprint-metrics/history.md`** to seed this
shift's model/parallelism defaults from what past shifts learned (see the ledger teeth below) — no roll-call
noise about it, just start smarter.

The **very first message** of a sprint is the Foreman's roll-call. Lead with an ASCII-art banner (per
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
  • Security Auditor — asks "can this be abused?" on risky diffs and gates the merge.
  • Performance Guard — watches size/speed/memory so nothing sneaks in a regression.
  • Release Engineer — keeps the version files in sync and ships releases reliably.
  • Docs Scribe — ships the in-app docs alongside each user-facing change.
  • Dependency Steward — audits deps and defends the lean, no-new-deps core.
  • Accessibility Auditor — checks keyboard, contrast, and labels on GUI changes.
  • Integration Tester — exercises whole workflows so features still compose.

Tonight's assignment: <what "this" is / the critical path>.
Starting work now — I'll report back when it's done or if I need you.
```

Timestamp it in local time like every on-screen message. Then begin.

## The per-ticket pipeline — ≥2 independent checks + UAT before "Done"

```
Worker builds + self-tests → INDEPENDENT Reviewer re-checks (code) → INDEPENDENT UAT exercises the feature
  → [risky diff: INDEPENDENT Security Auditor] → [GUI change: gui-smoke SCREENSHOTS → INDEPENDENT Visual Critic
     (+ Accessibility check)] → (CI) → Foreman merges → push
```

The **core gauntlet is always exactly two independent checks — Reviewer + UAT** — plus the Visual Critic on a GUI
change. Everything else is a **conditional leg** that fires *only when the diff earns it* (see below); the crew's
depth scales with a diff's risk, not with a fixed per-ticket agent tax.

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
4. **Independent Visual Critic (GUI changes only)** — for a change that alters what the app *looks like*, the
   worker's (or a dedicated) `gui-smoke` run **captures screenshots** of the affected surface(s) (CPE-1148),
   and the Foreman dispatches a **separate** taste-aware sub-agent to *look at* them and judge the visual
   result against the design standards (MENUS/TABS/pill-reflow/light-theme/alignment) + good taste. It returns
   **`VISUAL PASS`** / **`VISUAL CHANGES`** (concrete, screenshot-grounded defects). This is the leg that
   **replaces the user's routine eyes-on**: placement, clipping, misalignment, wrong/ambiguous glyphs, and
   off-theme are all screenshot-visible and get caught + routed back to the worker **without the user**. The
   Critic escalates to the user **only** for (a) a genuinely subjective taste/preference call — and then as a
   concrete pick-list, never an open question — or (b) something a screenshot can't reveal (interaction feel,
   animation cadence, real-hardware behaviour). Headless/backend tickets skip this leg.

### Conditional legs — pay for the check the diff earns (cost control)

Three of the newer roles are **not** unconditional per-ticket spawns — they'd blow the agent budget (below) if they
were. They fire **only on qualifying diffs**, and their cheap form **folds into an agent already running** instead
of spawning a fresh one:

- **Security Auditor** — spawned as a **separate** agent **only** when the diff touches the filesystem walk,
  IPC/sidecar, `capabilities/default.json`, the updater, or `unsafe`. For an ordinary diff the **Reviewer carries
  the security lens** (it already checks guardrail compliance) — **no extra spawn**. Gate on a risky diff:
  **`SEC PASS`**.
- **Accessibility Auditor** — reads the **same `gui-smoke` screenshots** the Visual Critic already produced, so it
  adds no new build. For a small UI tweak the **Visual Critic carries a quick a11y check** in-line — **no extra
  spawn**; a dedicated Accessibility Auditor is spawned only for a **substantial new UI surface**. Gate on a GUI
  diff: **`A11Y PASS`**.
- **Docs Scribe** — user-facing docs (the CPE-579 `src/docs/*.md` page + `sectionDocs.ts` entry) are written **by
  the Worker itself as part of the ticket** — **no extra spawn** for the common case. A dedicated Scribe is spawned
  only when the docs are substantial enough to be their own unit of work.

This keeps the **≥2-independent-checks core intact** while adding depth **only where a diff's risk pays for it** —
the whole point is throughput per agent, not more agents per ticket.

The **Foreman merges only after the Reviewer signs off, the UAT Tester returns `UAT PASS`, AND (for a GUI
change) the Visual Critic returns `VISUAL PASS` — plus, on a qualifying diff, `SEC PASS` (risky diff) and
`A11Y PASS` (substantial GUI surface).** On
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

### Reading CI honestly — full logs and non-lying polls (CPE-1868)

Two related failure shapes cost a real conclusion this run; both are now standing rules, recorded here —
where dispatches are written — not only in a ticket.

**A CI log fetch must fail loudly on a partial fetch, never return a silent prefix.** `gh run view --job
<id> --log` (and `--log-failed`) go through the CLI's "spec"-reporter view, which **truncates around
~4 MB** with no warning, no error, and no truncation marker — confirmed independently twice (CPE-1702;
CPE-1728 / CPE-1859, where a 13,676-line job log returned only ~4,100 lines and exited 0). Reading the
stopping point as the end of the run produced exactly the wrong conclusion once already (CPE-1859: "the
process never fired an assertion" vs. the truth — one spec genuinely failed, two more ran and passed
after it).

- **Fetch the raw log, not the reporter view**, whenever a job's output could be long: `gh api
  repos/:owner/:repo/actions/jobs/<job-id>/logs > job.log` returns the complete, untruncated text (this is
  what recovered the real CPE-1859 verdict). For `gui-smoke` specifically, prefer the uploaded
  `gui-smoke-suite-log-*` artifact (`gh run download <run-id> -p 'gui-smoke-suite-log-*' -D <dir>`,
  documented in `gui-smoke/README.md`) — it's captured by `tee` before any CLI-side truncation can apply.
- **State the log's total line count and that the fetch reached the end** in any conclusion drawn from a
  CI log — `wc -l job.log`, plus a look at whether the tail reads like a real finish rather than a
  mid-stream cut. Cheap, and it is precisely the check CPE-1859 skipped.
- Never conclude "the process never ran" / "no assertion fired" from where a log stops without that
  check — that reading is exactly what a truncated prefix produces.

**A CI poll must never read an empty or moving board as a green one.**

- **Read `total_count` and `mergeable` together with the pending count, never pending alone.**
  `total_count == 0` is a state to *report* ("no checks scheduled yet — or the PR is `CONFLICTING` and
  GitHub can't build a merge commit to run checks against"), never a state to treat as passing — CPE-1846
  sat at `total_count: 0` for eight minutes while `mergeable: CONFLICTING` (a sibling PR had merged
  underneath it) was the entire, immediately-visible explanation a `mergeable` check would have named in
  seconds.
- **`pending == 0` only means "done" once `total_count` has stopped moving.** Jobs schedule in waves
  (e.g. `gui-smoke` shards only exist once their build job finishes), so `pending` can fall toward zero,
  read as "nearly done," and then rise again as more checks appear — CPE-1863 measured `total_count`
  14→18→19 while `pending` went 7→10, dipping before it rose. Require `total_count` to be **stable across
  at least two reads** (or match a known expected count) before trusting a `pending == 0` reading.
- **`gh pr checks --watch` exits 0 when the branch moves under it, not only when checks pass.** After it
  returns, re-check the *current* head explicitly — `gh pr checks <pr>` or `gh pr view <pr> --json
  headRefOid,statusCheckRollup` — keyed to the SHA you expect. **A sub-agent must not run it at all**
  (CPE-1880: it blocks past the 600 s tool cap and is auto-backgrounded 100% of the time on this repo);
  the sanctioned bounded idiom for anyone who must read CI is `node scripts/ci-poll.mjs --pr <n>`, which
  applies both of the rules above mechanically and always returns a single `CI VERDICT:` line.
- **`gh api` pagination is the same shape from a third direction:** an unpaginated call silently returns
  only the first page (default 30 items), with no truncation marker of its own. Pass `--paginate` (or
  follow the `Link` header) whenever a listing could exceed one page, rather than trusting a short result
  to mean "that's everything."
- **State the totals whenever a poll's outcome is reported** — `total_count`, `pending`, `mergeable`, and
  (for a log) the line count and the end-reached check. Every wrong conclusion in this family came from a
  number that was true and incomplete.

## Escalation-decision policy (three-way, in order)

1. **Decide and log (default — the overwhelming majority):** any ambiguity/design-choice/blocker the Foreman
   can settle with a reasonable call — settle it, make the best guess (research it first if it's hard — see
   Researchers), log the assumption in the ticket work log, keep moving.
2. **Skip + queue for the user — asynchronously, never a blocking ask (don't stop the shift):** only when a
   ticket genuinely needs the *user's* own resources or authority — code-signing certs, security sign-off,
   secrets/credentials, a paid/external account, a model choice / API key, interactive cross-OS GUI
   verification, or a subjective taste sign-off. Do **not** ask-and-wait: skip it, record what's needed in the
   work log **and on the async "For you" review queue** (surfaced in the `FOREMAN` block footer), and keep
   working other tickets. The user handles the whole queue whenever they return. **Also add a row to the QA
   Architect's `MANUAL-TEST-BURNDOWN.md`** — every manual/interactive skip is debt to be automated away over
   time.
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
  and **start the next worker while a PR is in review** — review/UAT must never idle the build queue. Any
  **conditional leg** that a diff earns (Security on a risky diff, a dedicated Accessibility Auditor) joins that
  **same parallel fan-out**, not an extra serial stage. Only the final merge serialises.

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
ledger**. Substrate + full schema live in `.claude/sprint-metrics/` (`README.md`); the essentials:

- **Record a row when each sub-agent returns.** Append one JSON line to
  `.claude/sprint-metrics/ledger.jsonl` (gitignored, transient): role, ticket, ticket *class*, model,
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
  `.claude/sprint-metrics/history.md` (committed, shared CLI↔desktop): tickets shipped + the tuned
  defaults learned (e.g. `metadata-codec: sonnet, 2-wide, ~11m median, 0 stuck`). At **kickoff**, read the
  tail of `history.md` to **seed** this shift's model/parallelism defaults instead of relearning cold.
- **Report it.** Add a compact `• Metrics —` line to the `FOREMAN` block (merged count · median gauntlet ·
  retries · **escaped defects** · ~cost-proxy), and print the **full ledger table** in the end-of-shift wrap so
  the user sees where the time and (proxy) cost went — and whether anything bounced after merge.

This is still lightweight — a one-line append per agent and one distilled block per shift — but it means every
concurrency and model call is backed by measured throughput, not guesswork.

### The sub-agent budget — bounded batches + checkpoint-and-reset (never hit the wall)

Sub-agent spawns are capped **per session** by `CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION` (default **200**; the user
can raise it — the current value is the `CAP` referenced below, and the reset line tracks it, not a hardcoded 200).
An **ordinary** ticket burns ~3 agents (Worker + independent Reviewer + independent UAT); a **qualifying** ticket
adds the conditional legs it earns (a separate Security Auditor on a risky diff, a dedicated Accessibility Auditor
on a substantial GUI surface, ± a Researcher/Planner) for ~4–5. Because the conditional legs fire **only on
qualifying diffs** and their cheap form **folds into the Reviewer / Visual Critic / Worker** (see "Conditional
legs" in the pipeline), the average stays near **~3–4**, so at the default cap a session still tops out around
**~40–50 tickets**. If every ticket blindly spawned every leg it would collapse to **~25–30** — which is exactly
why the legs are gated: **throughput per agent, not more agents per ticket.** And if the shift spawns blindly to
the cap it **stalls mid-task** — the crew goes dark with in-flight work and no way to finish it (this happened:
200 agents → dead crew mid-epic). **Do not run into the wall. Reset the budget *before* it, often.**

- **The ledger IS the live counter.** `ledger.jsonl` already records one row per agent-run, so the Foreman
  always knows the running count — no separate bookkeeping. Track it against the cap.
- **Reserve a drain margin; reset at a threshold, not at the cap.** Treat **~75% of `CAP`** (≈150 at the default
  200) as the **reset line**, leaving ~25% of the cap as headroom to *finish what's in flight*. As the count approaches the line,
  **stop dispatching new tickets**, let the open gauntlets (Reviewer+UAT) complete, merge the drained PRs, prune
  worktrees — quiesce to a clean, all-green, nothing-in-flight state.
- **Checkpoint, then hand off for a session reset.** The per-session cap only refreshes in a **new session** (the
  Foreman cannot self-restart one). So at the reset line, after quiescing: append a **resumable checkpoint** to
  `.claude/sprint-metrics/CHECKPOINT.md` (committed) — *what merged this batch, what's next in priority order,
  active epic/slice + its plan/Library entry, any decide-and-log assumptions, tuned defaults* — then tell the
  user plainly: batch done, budget nearly spent, **start a fresh session and say "resume the sprint"** (or
  raise `CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION` to continue now). A fresh session reads `CHECKPOINT.md` +
  `history.md` and continues seamlessly with a full budget. This is the "reset often" loop: **work a bounded
  batch → quiesce → checkpoint → reset → resume**, indefinitely, without ever stalling with lost in-flight work.
- **Stretch the budget between resets (spend agents where they earn their keep).** Fewer agents per ticket = more
  tickets per session: **batch several trivial same-file tickets into one Worker**; **Foreman-apply a tiny,
  exactly-prescribed reviewer fix directly** (re-verify + a focused re-review) instead of a full worker
  round-trip; **reuse a Library hit** to skip a Researcher; **de-risk the hard slice once** with a single Plan
  agent rather than several flailing Workers. Keep the **≥2-independent-checks gate** (Reviewer AND UAT) — that's
  non-negotiable — but don't pile on extra refuters/researchers unless the ticket is genuinely high-risk.
- **Surface it.** Add a `• Budget —` line to the `FOREMAN` block: `agents ~N/CAP · ~M tickets to reset line` (show
  the real cap in place of `CAP`, e.g. `~120/200`). So a reset is a *planned, clean* checkpoint, never a surprise
  mid-merge.

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

- **A report naming a pending background task instead of results is incomplete, not a status update
  (CPE-1848, mechanised by CPE-1880).** A sub-agent report is recognisable as a **stall**, not progress,
  when it contains language like "a monitor is armed," "the background watch will report," "I'll wait for
  the notification," or any variant that defers to a signal the agent cannot receive (per the dispatch
  contract above). The Foreman must not treat that as "in flight and fine." **Do not eyeball this — run
  it:** `node scripts/stall-check.mjs <report-file> --prior <n>` classifies the report mechanically
  (exit `0` accept · `3` re-invoke · `4` take-over), ignores the phrasing when it appears inside a
  **code fence** (so quoting the rule is not committing the offence — `>` blockquotes are deliberately
  NOT exempt, because that exemption hid all five recorded stalls behind one `> ` prefix), and treats a
  backgrounded watcher, an armed monitor, a promised notification, a wait keyed to a signal the agent
  cannot receive, or "continuing to wait" as **hard** findings that no amount of surrounding detail
  excuses — including the `CI still pending on <SHA>` line this contract itself mandates. On `re-invoke`,
  `SendMessage` the same agent with the dispatch contract restated and an explicit "I own CI; report
  now, synchronously, with what you have." On `take-over` — the **second** stall-shaped report from that
  same agent — kill it and read its PR yourself; a third re-invoke is the loop CPE-1880 bounds, and
  run `batched-2026-08-23-1124` recorded an agent that produced four such returns and could not exit on
  its own.
- **Every message that directly addresses the user leads with an ASCII-art banner** (the user is often across
  the room and can't read prose) — see [[use-ascii-art-when-addressing-user]]. Keep the banner words short +
  high-contrast (`BUILDING…`, `RUNNING ✓`, `NEEDS YOU`, `DONE`).
- **Timestamp every on-screen message in system LOCAL time** (e.g. `date "+%Y-%m-%d %H:%M:%S %Z"`); stamp the
  **start and finish** of anything slow and show the **elapsed** (`CPE-983 done 17:22:41 (⏱ 7m32s)`). Per
  [[loop-behavior-needs-timestamps]].
- **Each idle poll-wait and the end-of-shift wrap** use a bordered **`FOREMAN`** block with a timestamp
  header and a **"For you (async)"** footer — an async review queue, **not** a question the loop is parked on —
  each line item split by a `────` rule:

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
  • Budget — <agents ~N/CAP · ~M tickets to reset line>   (CAP = the current CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION)
  ────────────────────────────────────────────────────
  • QA — <MVD: N manual surfaces (Δ this shift) · automating: CPE-XXX>
  ────────────────────────────────────────────────────
  • Next — <next action>
  ────────────────────────────────────────────────────
  • Next wake — <local time of the armed fallback wakeup, or "on next agent return">
  ────────────────────────────────────────────────────
  • For you (async) — <queued user-resource / taste items to review on return, or "nothing"; NOT a wait>
  ════════════════════════════════════════════════════
  ```

  The header stamp doubles as the `• Tick —` timestamp; `• Next wake —` records the armed `ScheduleWakeup` so
  the loop's cadence is always visible (per the heartbeat rules).

- **Return-facing wraps and the async "For you" queue are the rich, plain-language version** — expand, don't
  compress; the user has been away and won't remember IDs/jargon (see
  [[sprint-summarize-with-context]]). Lead with what a thing *is* in plain English, put `CPE-NNN` in
  parentheses. There is **no mid-sprint `AskUserQuestion`** — anything that would have been a question becomes
  a decided-and-logged call or an async "For you" queue item the user reviews on return.
- **Sign every sprint PR** with `— Foreman · sprint supervisor · <YYYY-MM-DD>` as the **last line** of
  the PR body, below this repo's required trailer (keep the "🤖 Generated with Claude Code" + session link).
  Sign the PR, not each commit (commits keep the standard `Co-Authored-By` + `Claude-Session` trailers).

## GUI verification = build → deploy → run (never a dev server)

Any time the user must **look at the GUI**, do the full **build → deploy (install the sidecar / AI-Console
build) → run** cycle yourself — never "go run `tauri dev`". Publishing/installing for GUI testing IS
authorised during a sprint. Build (`Release (sidecar-enabled)` workflow — plain `release.yml` is the wrong
one), kill every `cpe`/`ai-console` process (incl. `--session-daemon`) **before** installing or NSIS skips
the file-locked sidecar, verify the installed version + sidecar timestamp, then launch + confirm it's
responding. Bracket it with the ASCII **WAIT → ① BUILD → ② DEPLOY → ③ RUN → RUNNING → checklist** narration.
See [[gui-verify-needs-build-deploy-run]], [[always-install-sidecar-build]], [[install-kill-all-processes-first]].

**The Visual Critic does the looking — the sprint never waits on the user's eyes (CPE-1148).** The routine
visual check is the **Visual Critic reading `gui-smoke` screenshots** (per-ticket gauntlet step 4 above). It
catches the defects that used to cost a user round-trip — placement, clipping, misalignment, wrong/ambiguous
icons, off-theme, cramped spacing — and routes them back to the worker with **zero user involvement**. In this
lights-out mode the Critic is the *whole* mid-loop visual authority: it decides objective pass/fail and keeps
the ticket moving. Two things it **cannot** self-settle do **not** stop the loop to pull the user in — they
become **async "For you" queue items** (escalation #2), captured with the evidence the user needs to judge on
return, while the sprint keeps working other tickets: (a) a genuinely **subjective taste** call — queue the
screenshot + a concrete pick-list; (b) something a screenshot can't show — **interaction feel**, animation
cadence, drag latency, real-hardware behaviour — queue a note of what to try live. The user stays the ultimate
backstop, but **asynchronously**: they clear the visual/taste queue when they return, never as a mid-sprint
wait. (The button-placement/icon/clipping saga that motivated this was almost entirely screenshot-visible —
only the pure icon *preference* ever legitimately needed the user, and that is exactly the kind of item the
queue now holds.)

## Machine-sharing (announce presence, keep working — never wait)

The user is physically away, so the machine is free. If recent human input appears (idle time drops — they
came home or are remoting in), do **not** automatically pause and do **not** stop to ask. Lights-out means the
factory keeps running by default. Instead post a **one-line, non-blocking** note that I see they're here and
that they can say **"yield"** (I'll pause) or **"stop the sprint"** (I'll wind down) — then **immediately keep
working** without awaiting a reply. Yield *only* if they explicitly tell me to. Presence is never a question
the loop parks on and never a loop-ender; it is a courtesy heads-up that leaves the factory running.

## Shift end — release the loop's resources

Whenever the loop actually ends — the end-of-shift wrap, the checkpoint hand-off for a budget reset, a hard
stop, or the user telling me to stop — **tear down the loop's live state** so nothing dangles:

1. **Cancel the armed fallback wakeup** (`ScheduleWakeup` with `stop: true`) so no stray tick fires after the
   shift is over.
2. **Release the shift lock** — delete `.claude/sprint-metrics/SPRINT-LOCK` so the next Foreman (CLI or
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

*Cross-cutting habits referenced above live as memories (they apply outside the sprint too):*
`[[use-ascii-art-when-addressing-user]]`, `[[gui-verify-needs-build-deploy-run]]`,
`[[sprint-summarize-with-context]]`, `[[loop-behavior-needs-timestamps]]`, `[[go-with-recommendation]]`,
`[[code-changes-via-ticket]]`.
