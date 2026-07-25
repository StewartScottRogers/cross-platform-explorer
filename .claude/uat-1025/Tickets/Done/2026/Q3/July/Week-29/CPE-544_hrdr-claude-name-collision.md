---
id: CPE-544
title: "hrdrClaudeNative.cmd — focus existing Claude pane instead of name collision"
type: Defect
status: Done
priority: Medium
component: Docs
tags: [ready]
estimate: 30m
created: 2026-07-16
closed: 2026-07-16
---

## Summary
After CPE-543, running `hrdrClaudeNative.cmd` a second time (while a "Claude" pane already exists)
failed: a herdr agent name is unique per session, so `herdr agent start Claude` errors with
*"agent name Claude is already used"*. The script then printed a misleading *"Is a herdr session
running?"* message even though the session was running fine. Detect an existing "Claude" agent and
focus it instead of trying to start a duplicate.

## Environment
Windows 11, herdr 0.7.4-preview on PATH, Claude native integration hook installed. A live "Claude"
pane (`w6:p4`, running Claude Code, idle) already present in the session.

## Steps to Reproduce
1. Run `hrdrClaudeNative.cmd` once — a "Claude" pane starts.
2. Run `hrdrClaudeNative.cmd` again while that pane still exists.

## Expected Behavior
The launcher brings you to the existing Claude pane (focus it) and exits successfully.

## Actual Behavior
`herdr agent start` returned `agent_name_taken`; the script hit its error branch and printed
*"ERROR: herdr could not start the Claude pane. Is a herdr session running?"* then `pause`d — a
misleading message, since the real cause was the duplicate name.

## Acceptance Criteria
- [x] Before starting, the script checks for an existing "Claude" agent (`herdr agent get Claude`).
- [x] If one exists, it focuses that pane (`herdr agent focus Claude`) and exits 0 with a clear
      message — no crash, no `pause`.
- [x] If none exists, it starts a new tracked "Claude" pane as before.
- [x] The start-failure message no longer misattributes a name collision to a missing session.
- [x] Verified: with a "Claude" pane present, the script prints "A Claude pane already exists … -
      focusing it." and exits 0.

## Resolution
Added an existence guard ahead of `herdr agent start`:
```
herdr agent get Claude >nul 2>nul
if not errorlevel 1 (
    echo A Claude pane already exists in herdr - focusing it.
    herdr agent focus Claude >nul 2>nul
    endlocal
    exit /b 0
)
```
`herdr agent get <name>` exits 0 when the agent exists and 1 when it does not, so this reliably
routes a repeat launch to `focus` instead of a duplicate `start`. Also reworded the start-failure
message to stop implying a missing session.

Verified by running the script while the live `w6:p4` Claude pane existed: output was
"A Claude pane already exists in herdr - focusing it." with exit code 0.

## Work Log
- 2026-07-16 — Reproduced from the user's `agent_name_taken` error. Confirmed `herdr agent get Claude`
  returns exit 0/1 for present/absent and `herdr agent focus Claude` works. Added the focus-existing
  guard and fixed the misleading error text. Verified against the live pane; exit 0. Filed + closed.

## Notes
Follow-up to CPE-543. A herdr agent name is a unique per-session identifier, so a fixed name is the
right choice for a single tracked "Claude" pane — reuse (focus) is the correct behavior on repeat
launches rather than spawning a second, differently-named session.
