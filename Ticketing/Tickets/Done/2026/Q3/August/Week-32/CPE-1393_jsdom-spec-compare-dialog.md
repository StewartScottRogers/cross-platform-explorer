---
id: CPE-1393
title: "Test: jsdom render-spec for CompareDialog (two-side folder/file compare)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-724
created: 2026-08-07
---

## Problem (QA-Architecture MVD burndown)
`src/lib/components/CompareDialog.svelte` (two-side compare) has no jsdom coverage.

## Fix direction
Add `src/lib/components/CompareDialog.test.ts` (same recipe). Assert `scanTree` / `readFileText` /
`readFileRange` wiring for folder/file compare; diff vs equal states; large-file range path; error state.
Typed-call args + dispatched payloads. Report (don't fix) mis-wires. Test-only; parallel-safe.
