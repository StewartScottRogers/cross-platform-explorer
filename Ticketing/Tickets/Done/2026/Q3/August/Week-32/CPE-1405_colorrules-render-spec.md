---
id: CPE-1405
title: "Test: jsdom render-spec for ColorRulesDialog (rule builder + live preview)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-707
created: 2026-08-07
---

## Problem (hardening scout, Vein B)
`src/lib/components/ColorRulesDialog.svelte` has real `buildCondition`/summarize + live-preview logic (near the
WatchRules shape), zero coverage.

## Fix direction
Add `src/lib/components/ColorRulesDialog.test.ts` (recipe: `vi.mock("@tauri-apps/api/core")` +
@testing-library/svelte, per ConflictDialog/WatchRules specs). READ the component first for real API. Assert:
`change` fires live on every rule edit; each condition-kind builder produces the right condition; `save`/`cancel`
semantics + payloads. Non-hollow. Report any real mis-wire (don't fix). Test-only. NOTE: if the Add/save gate has
the same unvalidated-condition bug as WatchRules (CPE-1402), REPORT it (don't fix) so a follow-up can be filed.
