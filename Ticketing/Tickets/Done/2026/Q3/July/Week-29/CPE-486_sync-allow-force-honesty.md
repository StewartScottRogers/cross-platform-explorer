---
id: CPE-486
title: "Sync planner: make the dead `allow_force` field + its misleading doc honest"
type: Defect
status: Done
priority: Low
component: Backend
tags: [ready]
estimate: 15m
created: 2026-07-16
closed: 2026-07-16
epic: CPE-429
---

## Summary
`repos::SyncPolicy.allow_force` is documented ("never force-pushes unless `allow_force` is explicitly
set") but is **never read** by `plan_sync`, and there is no `SyncAction::ForcePush` — so the planner
in fact never force-pushes at all, and the field is dead config that silently no-ops. The doc makes a
promise the code doesn't keep.

Analysis: the current planner has **no scenario that needs a force-push** — after a `PullRebase` the
local branch is strictly ahead of the (ancestor) remote, so a normal `Push` succeeds. A force-push
would only be for a deliberate "mirror overwrite" mode, which is dangerous and intentionally NOT added
autonomously.

## Fix
Keep the field (it's a reasonable reserved policy input) but make everything **honest**: correct the
module doc to state the planner never force-pushes, and annotate `allow_force` as reserved for a
future guarded mirror-overwrite mode (not yet wired) so it can't be mistaken for working config.

## Acceptance Criteria
- [x] Module doc no longer claims a force-push behaviour the planner doesn't have.
- [x] `allow_force` is clearly marked reserved/not-yet-wired.
- [x] No behaviour change; existing sync tests pass; clippy clean.

## Notes
Found during Nightshift forge (CPE-429) research. Tiny honesty/quality fix — no dangerous force-push
logic added.

## Resolution
Corrected the `sync.rs` module doc (it no longer claims a force-push it doesn't do — the planner never
force-pushes; after a rebase local is ahead of the ancestor remote so a normal Push suffices) and
annotated `allow_force` as **reserved / not-yet-wired** (stable policy shape for a future guarded
mirror-overwrite mode). No behaviour change; repos crate 29 tests pass; clippy clean. Nightshift loop
14 (this loop also researched status.rs/browse.rs/naming.ts/unique_target/select-pattern and confirmed
them correct + already-implemented — a valid 'no bug' finding).
