---
id: CPE-1400
title: "Test: jsdom render-spec for WatchRulesDialog (condition/action validation + dry-run)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-737
created: 2026-08-07
---

## Problem (hardening scout, Vein B — meatiest component logic)
`src/lib/components/WatchRulesDialog.svelte` has real, untested logic: 5 condition-kind builders with NaN/empty
guards, a dry-run `planForEntry` preview, and watch-folder add/remove/toggle — zero coverage.

## Fix direction
Add `src/lib/components/WatchRulesDialog.test.ts` (recipe: `vi.mock("@tauri-apps/api/core")` +
@testing-library/svelte, per `ConflictDialog.test.ts`). READ the component first for real command/event names.
Assert: invalid input (blank/NaN) keeps Add disabled; valid input dispatches `save`/`watchConfig`/`undo` with
correct args; each condition-kind builder produces the right condition; the dry-run preview renders. Assert
typed-call args + dispatched payloads. Non-hollow. Report any real mis-wire (don't fix). Test-only.
