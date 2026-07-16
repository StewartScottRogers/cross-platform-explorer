---
id: CPE-500
title: "SPIKE: BridgeSpace research → propose epics for an AI Console peer/successor"
type: Spike
status: Done
priority: Medium
component: Multiple
tags: [spike]
estimate: 1h
created: 2026-07-16
closed: 2026-07-16
---

## Question (time-boxed)
What is **BridgeSpace** (bridgemind.ai/products/bridgespace), and what **set of epics** would let us
build a peer/successor to our **AI Console** ([[CPE-261]]) with similar capabilities — to either
replace it or ship alongside it? Time-box: ~1h of research + a written recommendation. The output is a
proposal, not code.

## Findings — what BridgeSpace is
BridgeSpace is BridgeMind's **Agentic Development Environment (ADE)** — a desktop workspace to
orchestrate many AI coding agents in parallel. Strikingly, it is **built on Tauri 2 + Rust,
cross-platform (macOS/Windows/Linux)** — *the exact stack of this app*. Its pillars:

- **Multi-pane terminal grid** — up to **16 agent terminals** visible at once, split any direction.
  (Our AI Console has multi-agent *tabs* (CPE-290) — same idea, one pane at a time.)
- **BridgeSwarm — multi-agent orchestration.** One prompt becomes a **team with roles** (Coordinator,
  Builders, Scout, Reviewer). A proprietary layer enforces **file ownership** (each task exclusively
  owns the files it touches, so concurrent agents never collide; shared deps are auto-sequenced), a
  **shared mailbox** for agent↔agent messaging, and **quality gates**.
- **BridgeBoard — a Kanban that dispatches agents**, two-way synced: move a card → an agent picks the
  task up; agents file findings and move cards through review as they work.
- **BridgeMemory — shared agent memory.** Plain markdown in `.bridgememory/`, linked into a **graph**
  every agent reads/writes **over MCP**. What one agent learns, the next starts with.
- **Integrated IDE** — file tree + editor + **embedded browser** beside the grid (read the diff, open
  localhost, review what agents shipped — the loop closes in one window).
- Broader BridgeMind platform: BridgeCode (CLI engine), BridgeVoice (voice), BridgeMCP (MCP server +
  shared context), BridgeAgent (recursive AI software engineer).

## How it maps to *our* AI Console (what we already have vs. the gap)
| BridgeSpace pillar | We have | Gap to close |
|--------------------|---------|--------------|
| Parallel terminal **grid** | AI Console multi-agent **tabs** (CPE-290), embedded PTY (CPE-280), session daemon + reattach (CPE-309) | Tiled/split **grid** so N agents are visible at once |
| **Swarm** (roles, file ownership, mailbox, gates) | single-agent sessions; agent registry (CPE-278); provider/model routing (CPE-285) | The orchestration layer: role assignment, file-ownership locks, inter-agent mailbox, quality gates |
| **Board** (Kanban dispatch) | **our own ticket system** (`Tickets/` folders + `/ticketing-*`) | A visual board that *dispatches agents* + syncs card↔folder both ways |
| **Memory** graph over MCP | MCP/plugin system (CPE-288/307); Agent Watch (CPE-396) | A shared `.*memory/` markdown **graph** agents read/write over MCP |
| Integrated **editor + browser** | the **explorer itself** (file tree), preview pane, text edit (CPE-066) | An in-pane **diff/editor** + **embedded browser** next to the grid |

**Bottom line:** we are ~50% of the way there. The AI Console (sidecar + agent registry + session
engine + MCP + Agent Watch) is a strong foundation; the differentiators are the **grid**, the **swarm
orchestration**, the **agent-dispatching board**, and the **shared memory graph**.

## Recommendation
**Build it as a SIBLING / evolution of the AI Console, not a replacement rewrite.** Reuse the sidecar
platform (CPE-260), the agent registry, the session engine (CPE-309), and the MCP plumbing; layer the
new surfaces on top. Working name: **"Agent Workspace"** (or "AI Console: Swarm"). Ship the grid first
(highest value, closest to what exists), then swarm, then board+memory.

Proposed set of **epics** to file (as dormant `Proposed` briefs, activate just-in-time):
1. **EPIC: Agent Grid** — evolve the AI Console's tabs into a **tiled, split-pane grid** so multiple
   agent terminals are visible + interactable at once; per-pane focus, resize, and the session-chip
   correlation (CPE-490) carried into the grid.
2. **EPIC: Swarm orchestration** — one prompt → a **role-based agent team** (coordinator/builder/
   scout/reviewer) with **file-ownership locks** (no collisions, shared-dep sequencing), an
   **inter-agent mailbox**, and **quality gates** before a task is "done". The hardest + most
   differentiating.
3. **EPIC: Agent Board** — a **Kanban that dispatches agents**, two-way synced with *our own*
   `Tickets/` system (move a card → an agent picks up the ticket; agents move cards through review).
   Uniquely natural here because we already have the ticket folders + workflow.
4. **EPIC: Shared agent memory graph** — a `.*memory/` markdown **graph** agents read/write **over
   MCP** (reuse the AI Console MCP system); persistent, per-project, cross-agent.
5. **EPIC: Integrated workbench** — an in-pane **diff/editor** + **embedded browser** beside the grid
   (the explorer is already the file tree), closing the review loop in one window.

**Decision needed from the user (out of spike scope):** replace vs. sibling (recommend sibling), the
name, and which epic to file/activate first (recommend Agent Grid).

## Sources
- https://www.bridgemind.ai/products/bridgespace
- https://www.bridgemind.ai/bridgeswarm
- https://www.bridgemind.ai/products
- https://docs.bridgemind.ai/docs/bridgespace
- https://www.bridgemind.ai/bridgemcp

## Note
Executed as the first **spike** (`type: Spike`) — a time-boxed research question that closes on a
written recommendation, not shipped code. The full spike *workflow* (type in the wiki, folder/status
handling, epic-activation integration) is still to be formalised — see the pending design questions
raised 2026-07-16.
