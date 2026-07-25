---
id: CPE-586
title: "AI Console: launcher tab strip surfaces swarm (server-created) sessions"
type: Bug
status: Done
priority: High
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
User ran a swarm and **no tabs appeared** in the AI Console. CPE-574 adopts swarm sessions into the
console (they surface in Agent Watch), but the launcher's own tab strip is *client-initiated* and only
picks up server-created sessions in `reattachSessions()` (boot). The 5-second poll (`refreshUsage`)
only refreshes usage badges **and early-returns when no tabs are open** — so a swarm launched from a
fresh console (0 tabs) is never adopted into a tab.

## Diagnosis (confirmed)
Ran the real ProductionPlanner MCP config manually: `claude … --mcp-config <mission>/mcp-*.json` loaded
the swarm host, wrote a memory note, and posted a `done` broadcast → `mailbox.jsonl` + `memory/` appeared.
**The swarm mechanism works end-to-end.** The only failure was UI surfacing: the launcher never opened
tabs for the adopted sessions.

## Fix
Make the periodic `/api/sessions` poll **adopt any server-created session into a tab** (same
`addSession` path `reattachSessions` uses) and drop the `!sessions.size` early-return, so swarm agents
launched from a fresh console appear as tabs within one poll interval.

## Acceptance Criteria
- [x] The poll adopts new server-created sessions into tabs (not just refreshes usage), even with zero
      tabs currently open.
- [x] jsdom test: a session returned by `/api/sessions` that the launcher doesn't know becomes a tab.
- [x] Full frontend suite (**623 passed**) + `npm run check` green.

## Resolution
`launcher.html` — the 5-second `refreshUsage` poll now lists `/api/sessions`, calls `addSession(id, name)`
for any session the launcher doesn't already have a tab for (the same adoption path `reattachSessions`
uses at boot), then applies usage — and the `!sessions.size` early-return is gone so it runs even with
zero tabs open. So a swarm launched from a fresh AI Console surfaces its agents as tabs within one poll.
jsdom test `adopts a server-created swarm session into a tab on poll` (returns the session only after
boot, asserts a `.term-pane` appears). Ships in the next build (0.36.0 predates it).

## Notes
Coordination itself is task-dependent — an agent only writes mailbox/memory if its task tells it to use
the swarm tools. Independent of this tab-surfacing fix.
