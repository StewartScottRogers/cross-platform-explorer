---
id: CPE-932
title: Move "Load demo" into the Swarm Tasks pane; show only after dropping Run swarm
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Refinement of CPE-931. The **Load demo** button was in the toolbar; per request it should be **inside the
Swarm Tasks pane** and **not shown until the "Run swarm" panel is dropped**.

- Removed the "Load demo" button from the toolbar — the toolbar now has just **Run swarm ▾** and **?**.
- Put a **"Load a demo ▾"** trigger in the swarm-tasks header (right of the label), inside the swarm row.
  Since the row is hidden until "Run swarm" drops it, the trigger only appears then.
- Clicking "Load a demo" reveals the demo dropdown (below the header) and loads the default demo, as before.

## Acceptance Criteria
- [x] "Load demo" is gone from the toolbar; it lives in the Swarm Tasks pane.
- [x] It appears only after "Run swarm" drops the panel.
- [x] Clicking it reveals the demo picker + loads a demo. Browser-verified; harness + full suite (926).

## Work Log
- 2026-07-23 — Moved the trigger into a swarm-tasks header; verified the drop→load flow in a browser.
