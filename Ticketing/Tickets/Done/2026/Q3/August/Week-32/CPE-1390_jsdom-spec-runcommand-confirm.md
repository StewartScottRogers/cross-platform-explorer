---
id: CPE-1390
title: "Test: jsdom render-spec for RunCommandConfirm (external-process safety gate)"
type: Task
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-739
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown — high safety leverage)
`src/lib/components/RunCommandConfirm.svelte` (CPE-783) is the safety gate that SPAWNS EXTERNAL OS PROCESSES,
with zero jsdom coverage. A regression here is high-risk.

## Fix direction
Add `src/lib/components/RunCommandConfirm.test.ts` (same mock recipe). Assert: renders the exact command lines
+ count; Run DISABLED when `commands.length===0`; Run → `commands.runCommand(cmd, cwd||null)` per line; renders
exit code / stdout / stderr; `failed()` styling on non-zero/error; Escape/backdrop close BLOCKED while
`running`. Assert typed-call args + dispatched payloads. Report (don't fix) any mis-wire. Test-only; parallel-safe.
