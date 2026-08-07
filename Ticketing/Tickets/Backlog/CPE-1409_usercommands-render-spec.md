---
id: CPE-1409
title: "Test: jsdom render-spec for UserCommandsDialog (add/edit validation + surface toggles)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-711
created: 2026-08-07
---

## Problem (hardening scout, Vein B — final coverage)
`src/lib/components/UserCommandsDialog.svelte` has untested add/edit form gating (blank name/template),
`toggleSurface`, and `startEdit` pre-fill. Zero coverage.

## Fix direction
Add `src/lib/components/UserCommandsDialog.test.ts` (recipe: mock `@tauri-apps/api/core` or the module it calls +
@testing-library/svelte, per prior specs). READ the component first for real API. Assert: add/save gated on
blank name/template; `toggleSurface` updates the right surface flag; `startEdit` pre-fills the form; save/cancel
payloads. Non-hollow. Report any real mis-wire (don't fix). Test-only.
