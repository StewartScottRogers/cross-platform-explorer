---
title: Swarms
order: 9
category: Agent Deck
categoryOrder: 3
---

# Swarms

A **swarm** runs *several* coding agents on one task at once, coordinating through a shared mailbox and a
shared memory instead of you shepherding each agent by hand. It builds on the Agent Deck: a **coordinator**
agent plans and dispatches, and one or more **builder** agents do the work.

> **Preview feature.** Swarms are new and evolving. The coordination pieces are in place and tested; the
> end-to-end run is best treated as experimental — start small and watch what the agents do.

## Run a swarm

1. Open the **Agent Deck**.
2. Pick an **Agent**, **Provider**, and **Model** as you would for a single launch, and set the **Working
   folder** (the repo the swarm works in). Make sure a provider **key** is set.
3. Click **Run swarm ▾**, type the **task** (e.g. *"add unit tests for the CSV parser"*), and click
   **Start**.

The console staffs a small team from your selected agent — a coordinator plus a builder — launches a real
session per role, and each agent opens in its own tab, just like a normal launch.

## Try a demo (fastest way to see it)

New to swarms? Click **Try a demo** next to *Run swarm*. It fills in a ready-made two-task example —
creating a couple of small files (`README-DEMO.md`, `NOTES-DEMO.md`) in the current folder — so you can
watch a swarm work end-to-end **without writing any tasks yourself**. Just pick your agent and press
**Start**; nothing else in the folder is touched. It's the quickest way to understand what a coordinator
and its builders actually do.

## How the agents coordinate

Every agent in a swarm is wired to a shared **swarm MCP host** scoped to the mission:

- **Mailbox** — agents post and read messages (`mailbox.post` / `mailbox.read`) to hand off work and
  report progress, addressed to another agent, a role, or the whole team.
- **Shared memory** — agents write and recall notes (`memory.write` / `memory.read` / `memory.recall`)
  so context is shared instead of re-derived by each agent.

State is shared through the mission folder on disk, so the separate agent processes genuinely see each
other's messages and notes. The coordinator dispatches tasks, and as each session finishes its result is
folded back so the next piece of work is launched — on to completion.

## Guardrails

- **Roles and file ownership** — tasks that touch the same files are sequenced so agents never collide;
  independent work runs in parallel.
- **Budgets and retries** — a mission can carry token/cost caps and a retry limit; when a cap is hit the
  affected agent (or the whole mission) is paused rather than running up spend.
- **You stay in control** — every agent is an ordinary Agent Deck session in its own tab, so you can watch
  it, type into it, or close it at any time.

## Tips

- Start with a **small, well-scoped task** and a repo you don't mind an agent editing.
- Keep an eye on the **Agents** entries in the explorer's left sidebar and the per-tab usage to see what
  each agent is spending.
- Swarms share the Agent Deck's trust model — an agent only gets the working folder and the credentials
  you granted.

See also: **Agent Deck** (launching agents), **Agent Grid** (watching several at once), and **Agent
Board** (the tickets a swarm can work from).
