---
id: CPE-963
title: Document the card-detail popup, directives, and the ticket-mcp MCP server
type: docs
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-503
---

## Summary
Update the in-app **Agent Board** docs page (`06-agent-board.md`) for the features shipped in
CPE-959/960/961/962 — the card-detail popup (resize/statusbar/pop-out), **Send directive**, and the
**`ticket-mcp`** MCP server — fulfilling the maintain-in-app-docs requirement.

## Acceptance Criteria
- [x] Fixed the now-stale "click an epic card jumps to the board" line (it opens details w/ a View-tickets button).
- [x] Added a **Card details** section (fields + body, resize thumb, status bar, ⧉ pop-out window).
- [x] Added a **Directives — talk to an agent** section (Send directive → `## Agent Directives`; ticket file
      is the wire) + a **`ticket-mcp` (MCP)** subsection (`directives.list` / `directives.reply`; configure any
      MCP client).
- [x] `npx vitest run docs sectionDocs` green (9 pass).
