# Workshift — Autonomous Supervised Work Loop

An autonomous "work while you're away" mode. Triggered when the user says **"start the workshift"** /
**"workshift"** / **"this is the workshift"** (the older **"dayshift"** phrasing is a kept alias). End it on
**"stop the workshift"**; re-baseline mid-run on **"restart the workshift"**.

The user is away and **cannot answer questions** — make the best reasonable guess, log the assumption in the
ticket work log, and keep moving until the work is **done** or the user **comes home**. **Never idle.** The
assignment is whatever "this" refers to when the shift starts; if nothing specific, work the critical path
(finish `Tickets/Doing/` → clear `Tickets/Backlog/` + pickable `Deferred/` → activate an epic → research +
file new epics, then build them).

**DO NOT STOP FOR APPROVAL OR STATUS UPDATES.** Never end a turn asking permission to continue; never pause
for an interim status; a "natural milestone" is not a reason to stop. Report only at the very end (out of
safe headless work / the user returns). If one ticket needs a user resource, **skip it and keep working
others** — don't halt the whole shift.

---

## The crew (all played by the assistant + AI sub-agents)

| Role | Responsibility |
|------|----------------|
| **Foreman** | The foreground supervisor (you). Splits work into well-scoped, low-conflict chunks, delegates them, **answers workers' questions**, serialises changes to `main`, tracks each item to Done, and decides the judgment calls so the shift never halts. |
| **Workers** | Sub-agents that implement well-scoped tickets **in parallel** (`isolation: "worktree"` so they don't collide with the shared checkout). Each builds, self-verifies, and opens a PR. |
| **Researchers** | Sub-agents the Foreman dispatches for genuinely-hard questions — they deeply research (codebase, in-repo docs/tickets, `context7`, web, worktree probes) and return **viable, tradeoff-labelled options**, not essays. |
| **Reviewer** | An **independent** sub-agent (NOT the author) that re-checks a worker's PR before merge — the QA gate. |

Spawning sub-agents is **pre-authorised** during a workshift (this overrides the default "don't spawn agents
unless asked"). Give each agent enough context (the ticket + acceptance criteria + relevant crates/APIs +
conventions + the delete-test rule) so it doesn't re-derive from cold.

## The per-ticket pipeline — ≥2 independent checks before "Done"

```
Worker builds + self-tests  →  INDEPENDENT Reviewer re-checks  →  (CI)  →  Foreman merges → push
```

A ticket is **never** marked Done / merged on the worker's own say-so. Two distinct checks are required:

1. **Worker self-verification** — builds + tests + `clippy` both feature modes + self-review against the
   ticket's acceptance criteria.
2. **Independent Reviewer** — after the worker opens its PR, the Foreman dispatches a **separate** reviewer
   sub-agent to re-run the checks itself and scrutinise **correctness** (logic + edge cases), **test
   adequacy** (do the tests actually exercise the behaviour + failure paths, or are they hollow?),
   **convention/guardrail compliance** (clippy both modes, delete-test / lean-core, no new deps, no scope
   creep), **no regressions**, and that the **acceptance criteria are genuinely met**. Prefer the repo's
   `/code-review` skill where it fits; else a `general-purpose`/`Explore` agent briefed to review.

The **Foreman merges only after the Reviewer signs off.** On `CHANGES REQUESTED`, route the findings back to
the worker (or apply a precise reviewer-prescribed fix), then **re-review** — loop until clean; log the
outcome in the ticket / PR. **CI green is a further automated check** but does **not** replace the
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
