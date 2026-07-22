---
id: CPE-861
title: Agent Board sidecar finds no tickets — walk up from CPE_BOARD_ROOT like the in-process board
type: bug
component: Sidecar
priority: high
status: In Progress
tags: ready
created: 2026-07-21
closed:
---

## Summary
The out-of-process Agent Board sidecar (CPE-850/853) shows **no tickets** when the explorer's current
folder is a **subfolder** of a project (or any folder without a direct `Tickets/` child). `openAgentBoard()`
passes the explorer's `currentPath` as `CPE_BOARD_ROOT`, and the sidecar's `board_root()` uses that value
**directly with no walk-up** (`sidecar/agent-board/src/lib.rs`), so it reads `<currentPath>/Tickets/` and
finds nothing.

The **in-process** board (BoardView) does the right thing: it resolves its root via
`find_project_root({ start })`, walking up to the nearest ancestor that has `Tickets/`, *then* reads cards.
That's why the board "used to work" — building `agent-board.exe` flipped the button onto the out-of-process
path, which lacks the walk-up. The bug is present in shipped sidecar builds too, not just dev.

## Fix
Make the sidecar walk up from the host-supplied root (or its cwd when none), matching the in-process board.
Extract a pure `resolve_board_root(explicit, cwd)` that applies `board::nearest_project_root` to the start
path, so it's unit-testable without env/cwd mutation. `board_root()` becomes a thin wrapper reading the env
var + cwd.

## Acceptance Criteria
- [ ] `resolve_board_root` walks up from an explicit root that is a **subfolder** to the project root.
- [ ] Empty/absent explicit root falls back to a cwd walk-up (unchanged behavior).
- [ ] Unit test covers both; `cargo test`/clippy green for the `agent-board` crate.
- [ ] Rebuilt `agent-board.exe`; Agent Board shows this repo's tickets when the explorer is anywhere in the
      project tree. GUI-verified.

## Work Log
- 2026-07-21 — Surfaced attended after building `agent-board.exe` (which routed the button to the
  out-of-process board). Root cause: `board_root()` uses `CPE_BOARD_ROOT` directly; the in-process board
  walks up via `find_project_root`. Branch `cpe-861-board-root-walkup` off main.
