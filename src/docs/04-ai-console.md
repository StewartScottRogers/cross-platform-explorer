---
title: AI Console
order: 4
category: AI Console
categoryOrder: 3
---

# AI Console

The AI Console runs **coding agents** (Claude Code, aider, and others) against a folder, using the
provider and model you choose. It opens in its own window from the AI Console button.

## Launch an agent

Pick an **Agent**, a **Provider**, and a **Model** (leave Model blank for the agent's default), set the
**Working folder**, then click **Launch**. Each launch opens its own **tab** — the sidecar runs them all
at once. A running agent also appears under **Agents** in the explorer's left sidebar.

## Keys and setups

- **Keys…** stores a provider API key in your OS keychain (never in a file, never shown again). Label a
  key (e.g. "work") to keep several per provider.
- **Save setup** remembers a provider + model choice under a name, per agent. It does not save your key.

## Sessions

Sessions survive closing and reopening the console (they keep running in the background), and reattach
when you reopen it. Right-click a tab or an Agents leaf for options; **double-click an Agents leaf** to
jump straight to that agent's tab.

## Grid

With sessions open, switch to **Grid** to see several agents side by side. See the **Agent Grid** page.

## Run a swarm

**Run swarm ▾** launches a small **team** of agents on one task — a coordinator plus a builder — that
coordinate through a shared mailbox and memory. Type a task, click **Start**, and each agent opens in its
own tab. It's a preview feature; see the **Swarms** page for how it works.
