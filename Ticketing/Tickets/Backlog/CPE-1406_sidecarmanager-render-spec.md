---
id: CPE-1406
title: "Test: jsdom render-spec for SidecarManager (status derivation + enable/stop/revoke/repair wiring)"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-862
created: 2026-08-07
---

## Problem (hardening scout, Vein B)
`src/lib/components/SidecarManager.svelte` has real untested logic: `statusOf`'s worst-first status derivation
(5-way priority) + 5 async command wirings (toggle/stop/revoke/grant/repair, each re-`refresh()`s).

## Fix direction
Add `src/lib/components/SidecarManager.test.ts`. READ the component first for the real `../sidecar` module API
(mock `../sidecar`, or the core invoke seam, whichever it calls). Assert: `statusOf` renders the correct
worst-first status pill for representative state combos (priority order); each action (toggle/stop/revoke/grant/
repair) calls its command with correct args AND triggers a `refresh()`. Non-hollow. Report any real mis-wire
(don't fix). Test-only.
