---
id: CPE-271
title: UI mount pane (host embeds a sidecar's own UI)
type: Task
status: Open
priority: High
component: Frontend
estimate: 3-4h
created: 2026-07-13
---

## Summary

A generic host pane/mode that embeds a sidecar's **own** frontend (per the charter:
each sidecar serves its UI; the host gives it a frame — child webview / iframe to a
local port declared in the manifest). One mount that works for any sidecar, wired
to the existing resizable-panel/mode system so **off = off** (no cost when hidden).

## Acceptance Criteria

- [ ] A dockable pane hosts a sidecar UI surface addressed from its manifest.
- [ ] Sandbox/CSP so the embedded surface can't reach explorer internals.
- [ ] Mount/unmount tied to sidecar lifecycle; hidden pane spins nothing up.
- [ ] Works for any sidecar (validated with the hello sidecar [[CPE-273]]).
- [ ] No startup or size regression to the plain explorer when unused.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-265]], [[CPE-264]]. **Phase:** P4. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
