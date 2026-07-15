---
id: CPE-396
title: Agent Watch — session registry (console announces active agent sessions)
type: feature
priority: high
estimate: M
status: Open
created: 2026-07-14
tags: [big-design, agent-watch, sidecar]
epic: AGENT-WATCH.md
---

## Goal
Foundation for Agent Watch: the host/frontend knows which coding-agent sessions are live
and on which Project Folder, so the explorer can surface them (CPE-397) and watch them
(CPE-398). Extends the existing Status-announcement channel (`ui:<url>`, CPE-271) rather
than adding new plumbing.

## Scope
- On Launch, the AI Console announces `session:{ sessionId, agentId, agentName, cwd, status }`
  via a Status message; on exit it announces the session ended.
- Frontend parses these (sibling to `parseUiAnnouncement`) into a live session list/store.
- Decoupled: the explorer reads announcements only; it never reaches into console internals.

## Off means off
No sessions announced → empty list, nothing allocated. Feature-gated behind sidecar-platform.

## Acceptance
- [ ] Console emits session announcements on start + end
- [ ] Frontend exposes a reactive list of active sessions (id, agent, cwd, status)
- [ ] Headless tests for the parse + store; plain explorer unaffected when off
