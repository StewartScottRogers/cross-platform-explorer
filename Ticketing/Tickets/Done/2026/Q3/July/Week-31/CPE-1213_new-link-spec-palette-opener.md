---
id: CPE-1213
title: "Fix new-link gui-smoke spec: open via Command Palette (nested New ▸ flyout flaky under CDP)"
type: bug
component: Testing
priority: low
status: Done
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
`gui-smoke/specs/new-link.smoke.ts` (CPE-1207) failed on the live build: the hover-to-expand "New ▸" flyout
doesn't open deterministically under headless CDP, so the render-pin + click-through tests couldn't reach the
New Link dialog. The FEATURE is fine (context menu opens, New Link is in the New submenu + a palette command);
only the nested-flyout interaction is flaky.

## Fix
- Open the dialog via the Command Palette (Ctrl+Shift+P → "New Link") — the reliable opener used by
  native-tags/organize specs — instead of the nested flyout. Spec now passes 3/3 and captures `new-link.png`.

## Acceptance Criteria
- [x] `new-link.smoke.ts` passes on the real build (dialog renders, hardlink click-through lists);
      `cd gui-smoke && npm run typecheck` green.

## Work Log
- 2026-08-01 — Foreman fix during the epic-715 Visual-Critic capture. Test-only; feature unaffected.
