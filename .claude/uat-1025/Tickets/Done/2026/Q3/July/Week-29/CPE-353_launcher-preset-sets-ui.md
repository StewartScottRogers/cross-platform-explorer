---
id: CPE-353
title: "AI Console launcher: named preset 'sets' dropdown per agent"
type: Feature
status: Done
closed: 2026-07-13
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

## Work Log
2026-07-13 — Implemented on branch `CPE-353-preset-sets-ui` on the CPE-352 backend.
- `console.rs`: `POST /api/presets` (save/update named set) + `POST /api/presets/delete`,
  using the persistent presets store. Round-trip test (save → catalog shows it → delete).
- `launcher.html`: a **Set** dropdown next to Agent (lists the current agent's sets; selecting
  one fills provider/model/small model), a **Save current as a set…** name box + Save button,
  and a **✕** to delete the selected set. Refreshes from the catalog after each change; sets
  re-render on agent switch. Key values are never part of a set (only a credential ref).
- ai-console `cargo test` 106 lib + 7 integration, `clippy` clean. Launcher UI needs a visual
  eyeball; the API + model are unit-tested. CPE-348 (credential profiles) folds under this.
