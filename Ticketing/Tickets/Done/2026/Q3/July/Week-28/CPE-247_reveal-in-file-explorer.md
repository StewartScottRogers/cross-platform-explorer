---
id: CPE-247
title: Context menu: Reveal in File Explorer / Finder
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 20m
created: 2026-07-12
closed: 2026-07-12
---

## Summary
Add a context-menu action (item and empty space) that reveals the target in the OS file manager via the opener plugin's revealItemInDir (already permitted by opener:default).

## Acceptance Criteria
- [ ] Right-click item or empty space offers 'Reveal in File Explorer' and opens the OS file manager at that item/folder
- [ ] npm run check passes.

## Resolution

## Work Log
2026-07-12 — Implemented in ContextMenu + App; check 0/0. Ships next release.
