---
id: CPE-405
title: Agent Watch — surface the agent's file READS (via its tool-output stream)
type: feature
priority: medium
estimate: L
status: Open
created: 2026-07-14
tags: [big-design, agent-watch]
epic: AGENT-WATCH.md
depends-on: CPE-398
---

## Problem / value
AGENT-WATCH.md wants reads/writes/edits/deletes. The CPE-398 filesystem watcher covers mutations
but CANNOT see reads (a Windows FS watcher doesn't report opens/reads). Reads are the missing half
of "understand what the agent is doing" — knowing which files it consulted explains its edits.

## The only viable source (design note)
Reads can't come from the filesystem — they must come from the agent's OWN activity. The AI Console
already runs the agent in a PTY and captures its output (the session ring, CPE-385). For agents
that announce tool calls (Claude Code prints Read/Edit/Bash tool invocations), parse the stream for
read operations and emit them alongside the FS watcher's mutations.

## Scope / risks
- Per-agent parser (start with Claude Code's output format); agent-agnostic fallback = none.
- Feed parsed reads into the same `ai-console://fs-activity` channel with a new `read` kind, so the
  timeline (CPE-400) + row annotations (CPE-399) show them (a distinct, dimmer style).
- Fragile by nature (depends on the agent's output format) — must degrade silently when the format
  isn't recognized; never block or corrupt the terminal stream.

## Acceptance
- [ ] Claude Code reads appear in the timeline + as (distinct) row annotations while watching
- [ ] Unknown/other agents: no reads, no errors (graceful)
- [ ] Parser is unit-tested against captured sample output; terminal I/O unaffected
