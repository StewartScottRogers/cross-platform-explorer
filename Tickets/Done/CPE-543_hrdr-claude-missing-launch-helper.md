---
id: CPE-543
title: "hrdrClaudeNative.cmd — runs locally (drop missing _hrdr-launch.cmd helper)"
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
`hrdrClaudeNative.cmd` set a batch of `HP_*` variables and then `call`ed a helper
`_hrdr-launch.cmd` on its last line — but that helper file does not exist anywhere in the repo
(never committed; a leftover from the CPE-542 rework). Running the script locally therefore always
failed with *"The system cannot find the batch file"* and never launched a Claude pane. Fix it to
launch the tracked "Claude" agent pane directly via `herdr agent start`, removing the dependency on
the missing helper.

## Environment
Windows 11, herdr 0.7.4-preview on PATH (`…\AppData\Local\Programs\Herdr\bin\herdr.exe`), Claude
native integration hook installed. Repo at `Z:\repos\cross-platform-explorer`.

## Steps to Reproduce
1. Ensure a herdr session is running.
2. Double-click / run `hrdrClaudeNative.cmd` from the repo root.

## Expected Behavior
Claude Code launches as a tracked "Claude" pane inside herdr, anchored to the repo, with the chosen
model forwarded.

## Actual Behavior
Script printed *"'_hrdr-launch.cmd' is not recognized … The system cannot find the batch file"* and
exited without launching anything.

## Acceptance Criteria
- [x] Script no longer references the non-existent `_hrdr-launch.cmd`.
- [x] Launches the tracked "Claude" pane directly via `herdr agent start` with `--cwd`, `--env
      CLAUDE_MODEL`, `--focus`, and `-- cmd /c "<repo>\RunClaude.cmd"`.
- [x] Guards for herdr-not-on-PATH and for `herdr agent start` failure (no running session), each
      with a clear message + `pause`.
- [x] Installs the native Claude integration hook only if not already `current`.
- [x] Verified end-to-end under real cmd.exe: a focused "Claude" pane starts in the repo cwd with
      argv `["cmd","/c","…\\RunClaude.cmd"]`.

## Resolution
Rewrote `hrdrClaudeNative.cmd` to inline the launch instead of delegating to the missing
`_hrdr-launch.cmd`:
- Added a `where herdr` PATH check with a clear error + `pause`.
- Kept the `RunClaude.cmd` resolve/validate block.
- Stripped the trailing backslash from `%~dp0` so the repo dir survives argument quoting.
- Ensures the Claude integration hook is installed (`herdr integration install claude`) only when
  `herdr integration status` does not already report `claude: current`.
- Launches with:
  `herdr agent start Claude --cwd "<repo>" --env CLAUDE_MODEL=%CLAUDE_MODEL% --focus -- cmd /c "<repo>\RunClaude.cmd"`
  and reports a running-session hint + `pause` if it errors.

Verified by running the script under cmd.exe: herdr returned `agent_started` for a focused "Claude"
pane in the repo cwd with argv `["cmd","/c","Z:\\repos\\cross-platform-explorer\\RunClaude.cmd"]`.
Test panes were closed afterward.

## Work Log
- 2026-07-16 — Diagnosed: line 41 `call "%~dp0_hrdr-launch.cmd"` targets a file that does not exist
  in the repo (regression from CPE-542). Confirmed herdr on PATH, server running, Claude integration
  hook `current (v7)`. Confirmed `herdr agent start` is the correct native-pane entry point.
- 2026-07-16 — Rewrote the script to call `herdr agent start` directly; added PATH + launch guards.
  Verified end-to-end under cmd.exe (pane `w6:p3` started, then closed). Filed + closed this ticket.

## Notes
Follow-up to CPE-542, which had repointed the script at `RunClaude.cmd` but left the dangling
`_hrdr-launch.cmd` reference. If `_hrdr-launch.cmd` is ever intended to exist as a shared launcher,
that is a separate piece of work; this ticket makes the script self-contained.
