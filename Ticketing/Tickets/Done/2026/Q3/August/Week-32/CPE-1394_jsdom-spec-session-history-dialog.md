---
id: CPE-1394
title: "Test: jsdom render-spec for SessionHistoryDialog (agent audit sessions)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-728
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown)
`src/lib/components/SessionHistoryDialog.svelte` (agent audit-session browser) has no jsdom coverage.

## Fix direction
Add `src/lib/components/SessionHistoryDialog.test.ts` (same recipe). Assert `commands.auditSessions` lists
sessions; selecting one → `commands.auditRead`; renders entries; empty ("no sessions") + error states.
Typed-call args + dispatched payloads. Report (don't fix) mis-wires. Test-only; parallel-safe.
