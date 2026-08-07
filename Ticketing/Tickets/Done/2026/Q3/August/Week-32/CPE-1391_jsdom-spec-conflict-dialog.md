---
id: CPE-1391
title: "Test: jsdom render-spec for ConflictDialog (forge merge-conflict resolution)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-715
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown)
`src/lib/components/ConflictDialog.svelte` (a stateful git/forge merge-conflict UI wiring 5 typed calls) has no
jsdom coverage — attended-only today.

## Fix direction
Add `src/lib/components/ConflictDialog.test.ts` (same recipe). Assert wiring of `forgeConflictState` /
`forgeConflictVersions` / `forgeResolveFile` / `forgeConflictContinue` / `forgeConflictAbort`: loads conflict
state, lists conflicted files + versions, resolve→continue happy path, abort path, empty + error states.
Typed-call args + dispatched payloads. Report (don't fix) mis-wires. Test-only; parallel-safe.
