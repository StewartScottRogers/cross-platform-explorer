---
id: CPE-283
title: "Lifecycle: uninstall"
type: Task
status: Done
priority: Medium
component: Backend
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Rust implementation of `*--uninstall.cmd`: remove an agent CLI via its per-OS
manifest recipe, report result, and refresh detection state.

## Acceptance Criteria

- [ ] `uninstall(agent)` follows the manifest recipe per OS; clear result.
- [ ] Detection ([[CPE-281]]) reflects removal afterward.
- [ ] Confirmation before removal in the UI; never removes prerequisites shared by
      other agents.
- [ ] Test with a stub manifest.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-281]]. **Phase:** C3. **Epic:** [[CPE-261]].

## Resolution

Added `lifecycle::uninstall` (ai-console): runs the manifest's per-OS `uninstall`
`OsCommand` via the `CommandRunner`, erroring on non-zero/spawn-fail/missing recipe. The
recipe removes only the agent's own package (never shared prerequisites). Completes the
lifecycle CRUD (detect/install/update/uninstall). 1 test; 26 crate tests + clippy green.

**Deferred:** the UI confirmation dialog before removal ([[CPE-289]] / UI); detection
reflects removal via the existing `detect` ([[CPE-281]]).

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented uninstall during dayshift. Done.
