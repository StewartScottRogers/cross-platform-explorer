---
id: CPE-353
title: "AI Console launcher: named preset 'sets' dropdown per agent"
type: Feature
status: Open
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

Part B: on the CPE-352 persistence foundation, add a **dropdown of named sets per coding
agent** to the launcher toolbar — pick a set to fill provider/model/small model; Save as… /
Update / Delete the current selection as a named set. CPE-348 (credential profiles) folds
under this as the "which credential" field of a set.

## Acceptance
- Per-agent dropdown lists that agent's saved sets; selecting one populates the toolbar;
  save/update/delete work and persist; the API-key value is never stored (only a credential ref).
