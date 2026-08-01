---
id: CPE-1209
title: "GUI: broken-link Repair link… action (suggest target + re-create)"
type: feature
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (after CPE-1206). Repair a broken symlink by re-pointing it at a found target.

## Build
- Right-click a broken link → "Repair link…" → call `suggest_repair` (CPE-1206), show the suggested target with
  Accept (re-create the symlink to the found path) / Browse-for-another. Confirm before overwriting.

## Acceptance Criteria
- [ ] gui-smoke render pin of the repair dialog with a suggested target; `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Prereq CPE-1206. Batch with CPE-1207 (shared App.svelte/ContextMenu).
