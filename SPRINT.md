# Sprint

A **sprint** is an autonomous "work while you're away" mode. You say **"start the sprint,"** step away
(day job, errands, hours), and the assistant keeps making real, merged progress on this project **without
needing to ask you anything**. When you're back, say **"stop the sprint."**

The point: **you don't babysit it.** If something is ambiguous, it makes a sensible call, writes down the
assumption in the ticket, and keeps going instead of stalling for you.

## The crew (all played by the assistant + AI helper agents)

| Role | What it does |
|------|--------------|
| **Foreman** | The supervisor. Splits work into well-scoped chunks, hands them out, keeps helpers from colliding, decides the judgment calls, and merges finished work. Never halts the crew to ask you. |
| **Workers** | Helper agents that build pieces **in parallel**, each in an isolated copy of the repo. Each builds, tests, and opens a pull request. |
| **Researchers** | For genuinely hard questions, the Foreman sends a helper to dig through the code, docs, and the web and return **concrete labelled options** — so decisions are well-founded, not guesses. |
| **Reviewer** | Independent quality check (see below). |

Helpers are **right-sized**: cheap/fast AI for trivial jobs, more powerful (and pricier) AI only for the hard
ones, to keep costs sane.

## Quality gate — every piece gets **at least two independent checks** before it's "done"

1. **Worker self-check** — the worker who built it tests and verifies its own work.
2. **Independent Reviewer** — a **separate** helper (not the one who wrote it) reviews the change for: correct
   logic + edge cases, tests that actually test the thing (not hollow), project conventions/guardrails, scope
   discipline, and no regressions.

Work is merged **only after the Reviewer signs off.** Issues go back for fixes and are re-reviewed until
clean. The automated build (CI) is a nice third check on top — but it can't catch wrong-but-compiling logic,
which is why the independent review matters.

## The rules it follows

- **Never idle, never stop to ask** — it chains one task after another. Only exception: if it notices you're
  back at the computer, it pauses and asks whether you want the machine.
- **Everything is tracked** as a ticket, built on a branch, tested, reviewed, and only merged when green.
  Nothing counts as done until it's actually saved to the project.
- **Honesty over completion** — if a task genuinely needs *you* (a password/API key, a design call only you
  can make, hands-on testing), it doesn't fake it done. It skips it, notes exactly what's needed, and works
  other things.
- **Plain-language reporting** — while working it posts short status blocks; when it finishes or needs to ask
  you something, it gives a **full, plain-English summary with context** (no cryptic ticket codes at you),
  because you've been away and won't remember the shorthand.

## Using it

- Start: **"start the sprint"**
- Stop: **"stop the sprint"**
- Restart from a clean point: **"restart the sprint"**

Related runbooks: [RELEASING.md](RELEASING.md) · project guide: [CLAUDE.md](CLAUDE.md).
