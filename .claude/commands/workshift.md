# Workshift — Autonomous Supervised Work Loop

An autonomous "work while you're away" mode. Triggered when the user says **"start the workshift"** /
**"workshift"** / **"this is the workshift"** (the older **"dayshift"** phrasing is a kept alias). End it on
**"stop the workshift"**; re-baseline mid-run on **"restart the workshift"**.

The user is away and **cannot answer questions** — make the best reasonable guess, log the assumption in the
ticket work log, and keep moving until the work is **done** or the user **comes home**. **Never idle.** The
assignment is whatever "this" refers to when the shift starts; if nothing specific, work the critical path
(finish `Tickets/Doing/` → clear `Tickets/Backlog/` + pickable `Deferred/` → activate an epic → have the
**Product Manager** task Researchers to find + pitch new epics, pick the highest-impact ones, then build them).

**DO NOT STOP FOR APPROVAL OR STATUS UPDATES.** Never end a turn asking permission to continue; never pause
for an interim status; a "natural milestone" is not a reason to stop. Report only at the very end (out of
safe headless work / the user returns). If one ticket needs a user resource, **skip it and keep working
others** — don't halt the whole shift.

---

## The crew (all played by the assistant + AI sub-agents)

| Role | Responsibility |
|------|----------------|
| **Foreman** | The foreground supervisor (you). Splits work into well-scoped, low-conflict chunks, delegates them, **answers workers' questions**, serialises changes to `main`, tracks each item to Done, and decides the judgment calls so the shift never halts. |
| **Product Manager** | Owns *what* the shift builds at the epic level. When the critical path runs to fresh epics, the PM **tasks Researchers to find and pitch candidate epics**, then **picks and prioritises** the ones with the best overall product impact — weighing them against [PURPOSE.md](../../PURPOSE.md) and its fast/small/predictable tiebreaker (and the Agent Watch precedence), user value, effort/blast-radius, and fit with what already ships — and hands the chosen epic(s) to the Foreman to activate (`/ticketing-epic`) and decompose. Declines or defers low-impact/off-purpose pitches with a one-line rationale in the epic's work log. The PM decides *which* epics; the Foreman decides *how* they're built. |
| **Workers** | Sub-agents that implement well-scoped tickets **in parallel** (`isolation: "worktree"` so they don't collide with the shared checkout). Each builds, self-verifies, and opens a PR. |
| **Researchers** | Sub-agents the Foreman dispatches for genuinely-hard questions — they deeply research (codebase, in-repo docs/tickets, `context7`, web, worktree probes) and return **viable, tradeoff-labelled options**, not essays. |
| **Reviewer** | An **independent** sub-agent (NOT the author) that re-checks a worker's PR before merge — the code QA gate. |
| **UAT Tester** | An **independent** sub-agent responsible for **user acceptance testing** — it stands in for the end user and checks the change *from the outside*: does it actually do what the user asked, is the behaviour/UX acceptable, does it meet the ticket's acceptance criteria as a person would experience them (not just as unit tests assert)? Distinct from the Reviewer (who scrutinises the code); the UAT Tester exercises the **feature**. For user-facing/GUI changes it drives the real build (see GUI verification below); for headless/backend changes it exercises the command or API surface end-to-end. Signs off `UAT PASS` / `UAT FAIL` with concrete reproduction of what it did. |

Spawning sub-agents is **pre-authorised** during a workshift (this overrides the default "don't spawn agents
unless asked"). Give each agent enough context (the ticket + acceptance criteria + relevant crates/APIs +
conventions + the delete-test rule) so it doesn't re-derive from cold.

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
reviewer-prescribed fix), then **re-review + re-run UAT** — loop until both are clean; log the outcome in the
ticket / PR. **CI green is a further automated check** but does **not** replace the
human-style Reviewer — a green build can still ship wrong logic or hollow tests.

Every code change goes through a `CPE-NNN` ticket; **not pushed = not done**. Land each: branch (never
`main`) → checks → review → merge → push.

## Escalation-decision policy (three-way, in order)

1. **Decide and log (default — the overwhelming majority):** any ambiguity/design-choice/blocker the Foreman
   can settle with a reasonable call — settle it, make the best guess (research it first if it's hard — see
   Researchers), log the assumption in the ticket work log, keep moving.
2. **Skip + note for the user (don't stop the shift):** only when a ticket genuinely needs the *user's* own
   resources or authority — code-signing certs, security sign-off, secrets/credentials, a paid/external
   account, a model choice / API key, or interactive cross-OS GUI verification. Skip it, record what's
   needed in the work log, keep working other tickets.
3. **Hard stop (rare, safety only):** pause the whole shift *only* for a genuinely unsafe/out-of-bounds
   action — risk of irreversible data loss outside the repo, breaking the green release pipeline, pushing
   directly to `main`, committing secrets, or a destructive/outward-facing action beyond the granted
   autonomy.

## Right-size the AI model per task (cost + performance)

Assign each worker/researcher the **cheapest model that will do the job well** (the `model` override on the
`Agent` tool: `haiku` / `sonnet` / `opus` / `fable`). Rough tiers: **haiku** — trivial/mechanical tickets,
broad read-only fan-out, cheap parallel research breadth; **sonnet** — most implementation tickets + moderate
research (the sensible default); **opus** — architecturally hard / high-blast-radius tickets, gnarly
debugging, and **deep-reasoning review/research where a wrong answer is costly**. Escalate a *stuck* agent to
a stronger model rather than burning loops; prefer many cheap researchers in parallel for breadth, reserving
`opus` for the one question that needs depth. Note any non-default choice in the work log.

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
  • Next — <next action>
  ────────────────────────────────────────────────────
  • Awaiting you — <user-resource blockers, or "nothing">
  ════════════════════════════════════════════════════
  ```

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
