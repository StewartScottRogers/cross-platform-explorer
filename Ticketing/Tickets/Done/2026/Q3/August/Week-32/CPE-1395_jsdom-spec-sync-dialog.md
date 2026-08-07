---
id: CPE-1395
title: "Test: jsdom render-spec for SyncDialog (forge push/pull)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-715
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown)
`src/lib/components/SyncDialog.svelte` (git forge push/pull) has no jsdom coverage.

## Fix direction
Add `src/lib/components/SyncDialog.test.ts` (same recipe). Assert `commands.forgeRepoStatus` renders
ahead/behind/dirty; Sync → `commands.forgeSync`; in-progress + error states. Typed-call args + dispatched
payloads. Report (don't fix) mis-wires. Test-only; parallel-safe.
