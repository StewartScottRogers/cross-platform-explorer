---
id: CPE-511
title: "SPIKE: Herdr (herdr.dev) — deep research on the terminal agent multiplexer"
type: Spike
status: Done
priority: Medium
component: Multiple
tags: [spike]
estimate: 1-2h
created: 2026-07-16
closed: 2026-07-16
---

## Question (time-boxed)
What is **Herdr** (herdr.dev), how does it work in technical depth, and — since it overlaps heavily with
our **AI Console** ([[CPE-261]]) and the just-activated **Agent Grid** ([[CPE-501]]) — which of its
ideas should we adopt, and into which existing Agent-Workspace epic? Deep-research spike: output is a
findings dossier + a recommendation, not code.

## Findings — what Herdr is
**Herdr is an "agent multiplexer that lives in your terminal" — tmux for coding agents.** A single
**Rust binary** (no Electron, no app, no account, no telemetry) that runs *inside* your existing
terminal and manages many AI coding agents (Claude Code, Codex, OpenCode, Cursor, Copilot, aider, …)
in one workspace. Tagline: *"One terminal for the whole herd — run all your coding agents from one
terminal, on any box, even over ssh."* Dual-licensed **AGPL-3.0 / commercial**; ~**v0.7.4**, 73
releases, actively developed (mid-2026). Install: `curl -fsSL https://herdr.dev/install.sh | sh`,
`brew install herdr`, or `mise use -g herdr`.

### Architecture
- **Server/client split.** A background **server** owns pane lifecycle, the real **PTYs**, and the
  socket API; a thin **client** is the terminal UI. This is what makes persistence + remote work.
- **Session persistence.** The server keeps every pane + agent process **alive** across detach / closed
  terminal / lost connection; reattach with `herdr` from any terminal and everything is exactly where
  you left it. Sessions are **plain JSON on local disk**. Documented capabilities: *detach, restart
  restore, pane-history replay, native agent resume, live handoff.*
- **Pane / tab / workspace model** with **real PTYs**, **mouse-first** (drag borders, right-click
  menus, clickable panes) **and** tmux-style keyboard (default prefix `ctrl+b`; `ctrl+b q` detaches).

### The differentiators (vs. a plain multiplexer)
1. **Agent-state awareness.** Herdr *understands what's happening inside each pane* — it surfaces live
   **semantic state per agent: idle / working / blocked / done** — via **process-name matching +
   terminal-output heuristics**. This is the headline feature tmux/screen don't have.
2. **Control surface = CLI + a local JSON socket API.** Workspaces are drivable "from scripts, tools,
   **and agents**." Agents themselves use the **socket API to spawn panes, read output, and coordinate
   execution** — i.e. **agent-to-agent orchestration** is a first-class, documented capability
   (`herdr.dev/docs/socket-api/`).
3. **Remote over SSH, done right.** `herdr --remote ssh://user@server` makes your local terminal a
   client of a **remote** Herdr server; unlike plain `ssh + tmux`, **image pasting, mouse control, and
   agent-state tracking all keep working** — usable from a phone/tablet.
4. **Plugins.** Local **executable workflow plugins** with a **manifest (actions + event hooks)**;
   150+ community plugins; a marketplace is planned. Config covers keybindings, themes, sidebar,
   notifications, scrollback.

## How Herdr maps to *our* stack (overlap vs. gap)
Herdr is the **closest analog we've researched** to the AI Console + Agent Grid — the same core job
(many coding agents, side-by-side panes, persistence, reattach). The instructive differences:

| Capability | Herdr | Us (today) | Takeaway |
|------------|-------|------------|----------|
| Multi-agent panes | mouse-first tiled panes/tabs/workspaces | AI Console tabs → **Agent Grid** just built ([[CPE-506]]/[[CPE-507]]) | **On par** after CPE-501 |
| Persistence / reattach | server keeps PTYs alive; JSON on disk; replay + resume | session daemon reattach ([[CPE-309]]) | On par (local) |
| **Semantic agent state** (idle/working/blocked/done) | **yes** — process + output heuristics, surfaced live | **no** — Agent Watch shows *filesystem* activity, not agent state | **Adopt.** Strongest borrowable idea |
| **Control/socket API for agents** | **yes** — agents spawn panes + coordinate over a socket | our host commands are for the frontend, not agent-driven | **Adopt** into Swarm ([[CPE-502]]) |
| **Remote client/server (SSH)** | **yes** — full remote with mouse/images/state | **no** — daemon is local-only | **Gap** — candidate new epic |
| Form factor | terminal-native binary (TUI) | Tauri **GUI** window + embedded xterm | Different by design; not a gap |
| Plugins | executable workflow plugins + marketplace | MCP plugins + signed agent catalog (CPE-288/308) | Different model; ours is fine |

## Recommendation
Herdr **validates the Agent-Workspace direction** (CPE-500 program) and is a live proof point that a
Rust multi-agent multiplexer with persistence is viable and wanted. We should **not** start a new
program to clone it — instead **feed three of its ideas into the epics we already have**, and consider
**one new epic** for the genuine gap (remote):

1. **Adopt "semantic agent-state awareness" — highest-value, do first.** Detect per-session
   **idle / working / blocked / done** from PTY output heuristics + process state, and surface it on
   the **Agent Grid tile header** (CPE-507) and the explorer's left-pane Agents leaves + Agent Watch.
   Recommend filing this as a **new child of [[CPE-501]]** (it extends the grid we just built) or a
   focused sibling ticket — it's the single best idea to lift and is small-to-medium.
2. **Fold "agent-drivable control/socket API" into Swarm ([[CPE-502]]).** Herdr's socket API is exactly
   the agent-to-agent coordination substrate CPE-502 needs (spawn panes, read output, mailbox). Note it
   in CPE-502's brief as prior art / a concrete design reference when that epic activates.
3. **Note "workflow plugins with event hooks"** as prior art for the plugin/MCP surface — no action now.
4. **Consider a NEW epic: "Remote agent sessions (client/server over SSH)."** Herdr's remote model is a
   real capability we lack; the CPE-309 daemon is the natural seed. Sibling to the Agent Workspace
   program. File as a dormant `Proposed` brief if the user wants it on the board.

**Decision for the user (out of spike scope):** whether to (a) file the agent-state-awareness ticket
under CPE-501 now, and (b) file the "Remote agent sessions" epic brief. I did neither — this spike only
recommends.

## Sources
- https://herdr.dev/ — product homepage ("one terminal for the whole herd")
- https://herdr.dev/docs/ — documentation entry (session mgmt, socket API, agents, plugins, config)
- https://herdr.dev/docs/socket-api/ — the agent-facing control API
- https://github.com/ogulcancelik/herdr — source + README (Rust, architecture, install, AGPL/commercial)
- https://www.coddykit.com/pages/blog-detail?id=512884 — "The Rust Agent Multiplexer…" analysis
- https://nahornyi.ai/en/news/herdr-dev-review-local-ai-agent-management — review (local orchestration)
- https://aitoolly.com/ai-news/article/2026-07-07-herdr-... — analysis (terminal-based multiplexer)

## Note
Second research spike after [[CPE-500]] (BridgeSpace). Where BridgeSpace was a Tauri **GUI** ADE (broad
program analog), Herdr is a **terminal-native** multiplexer whose *specific* features (agent-state
awareness, agent socket API, SSH client/server) are the sharpest borrowable ideas — routed into the
existing Agent-Workspace epics rather than a new program.
